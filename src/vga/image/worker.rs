use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};

use super::{
    ImagePreparationPoll, ImagePreparationQueue, ImagePreparationRequest, ImagePreparationResult,
    ImagePreparationSignal, ImagePreparationSubmitted, ImagePreparer, PreparedImageKey,
};

struct PoolChannels {
    requests: SyncSender<ImagePreparationRequest>,
    results: Receiver<ImagePreparationResult>,
}

pub struct ImagePreparationPool {
    preparer: Arc<dyn ImagePreparer>,
    channels: Mutex<Option<PoolChannels>>,
    pending: Arc<Mutex<HashSet<PreparedImageKey>>>,
    active: Arc<Mutex<(u64, u64)>>,
    signal: Arc<ImagePreparationSignal>,
    closed: AtomicBool,
}

impl ImagePreparationPool {
    pub fn new(preparer: Arc<dyn ImagePreparer>) -> Self {
        Self {
            preparer,
            channels: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashSet::new())),
            active: Arc::new(Mutex::new((0, 0))),
            signal: Arc::new(ImagePreparationSignal {
                pending: std::sync::atomic::AtomicUsize::new(0),
                ready: std::sync::atomic::AtomicBool::new(false),
            }),
            closed: AtomicBool::new(false),
        }
    }

    fn start(&self) {
        let mut channels = self.channels.lock().expect("image preparation channels");
        if channels.is_some() {
            return;
        }
        let (requests, request_rx) = sync_channel(128);
        let (result_tx, results) = sync_channel(1);
        let preparer = Arc::clone(&self.preparer);
        let active = Arc::clone(&self.active);
        let signal = Arc::clone(&self.signal);
        std::thread::spawn(move || run_worker(preparer, request_rx, result_tx, active, signal));
        *channels = Some(PoolChannels { requests, results });
    }
}

impl ImagePreparationQueue for ImagePreparationPool {
    fn activate(&self, generation: u64, surface_epoch: u64) {
        *self.active.lock().expect("image preparation context") = (generation, surface_epoch);
    }

    fn submit(&self, request: ImagePreparationRequest) -> ImagePreparationSubmitted {
        if self.closed.load(Ordering::Acquire) {
            return ImagePreparationSubmitted::Closed;
        }
        if *self.active.lock().expect("image preparation context")
            != (request.key.generation, request.key.surface_epoch)
        {
            return ImagePreparationSubmitted::Refused;
        }
        self.start();
        let key = request.key;
        let mut pending = self.pending.lock().expect("image preparation pending");
        if !pending.insert(key) {
            return ImagePreparationSubmitted::Duplicate;
        }
        if pending.len() > 128 {
            pending.remove(&key);
            return ImagePreparationSubmitted::Refused;
        }
        let sender = self
            .channels
            .lock()
            .expect("image preparation channels")
            .as_ref()
            .map(|channels| channels.requests.clone());
        let Some(sender) = sender else {
            pending.remove(&key);
            return ImagePreparationSubmitted::Closed;
        };
        match sender.try_send(request) {
            Ok(()) => {
                self.signal.pending.fetch_add(1, Ordering::AcqRel);
                ImagePreparationSubmitted::Queued
            }
            Err(TrySendError::Full(_)) => {
                pending.remove(&key);
                ImagePreparationSubmitted::Refused
            }
            Err(TrySendError::Disconnected(_)) => {
                pending.remove(&key);
                ImagePreparationSubmitted::Closed
            }
        }
    }

    fn poll(&self) -> ImagePreparationPoll {
        self.signal.ready.store(false, Ordering::Release);
        let channels = self.channels.lock().expect("image preparation channels");
        let Some(channels) = channels.as_ref() else {
            return ImagePreparationPoll::Empty;
        };
        match channels.results.try_recv() {
            Ok(result) => {
                self.pending
                    .lock()
                    .expect("image preparation pending")
                    .remove(&result.key);
                self.signal.pending.fetch_sub(1, Ordering::AcqRel);
                ImagePreparationPoll::Ready(result)
            }
            Err(TryRecvError::Empty) => ImagePreparationPoll::Empty,
            Err(TryRecvError::Disconnected) => ImagePreparationPoll::Disconnected,
        }
    }

    fn signal(&self) -> Arc<ImagePreparationSignal> {
        Arc::clone(&self.signal)
    }

    fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        self.channels
            .lock()
            .expect("image preparation channels")
            .take();
    }
}

pub struct InlineImagePreparationQueue {
    preparer: Arc<dyn ImagePreparer>,
    active: Mutex<(u64, u64)>,
    pending: Mutex<HashSet<PreparedImageKey>>,
    results: Mutex<VecDeque<ImagePreparationResult>>,
    signal: Arc<ImagePreparationSignal>,
    closed: AtomicBool,
}

impl InlineImagePreparationQueue {
    pub fn new(preparer: Arc<dyn ImagePreparer>) -> Self {
        Self {
            preparer,
            active: Mutex::new((0, 0)),
            pending: Mutex::new(HashSet::new()),
            results: Mutex::new(VecDeque::new()),
            signal: Arc::new(ImagePreparationSignal {
                pending: std::sync::atomic::AtomicUsize::new(0),
                ready: std::sync::atomic::AtomicBool::new(false),
            }),
            closed: AtomicBool::new(false),
        }
    }
}

impl ImagePreparationQueue for InlineImagePreparationQueue {
    fn activate(&self, generation: u64, surface_epoch: u64) {
        *self.active.lock().expect("inline image context") = (generation, surface_epoch);
    }

    fn submit(&self, request: ImagePreparationRequest) -> ImagePreparationSubmitted {
        if self.closed.load(Ordering::Acquire) {
            return ImagePreparationSubmitted::Closed;
        }
        if *self.active.lock().expect("inline image context")
            != (request.key.generation, request.key.surface_epoch)
        {
            return ImagePreparationSubmitted::Refused;
        }
        let mut pending = self.pending.lock().expect("inline image pending");
        if !pending.insert(request.key) {
            return ImagePreparationSubmitted::Duplicate;
        }
        self.signal.pending.fetch_add(1, Ordering::AcqRel);
        let result = self.preparer.prepare(request);
        self.results
            .lock()
            .expect("inline image results")
            .push_back(result);
        self.signal.ready.store(true, Ordering::Release);
        ImagePreparationSubmitted::Queued
    }

    fn poll(&self) -> ImagePreparationPoll {
        let result = self
            .results
            .lock()
            .expect("inline image results")
            .pop_front();
        let Some(result) = result else {
            self.signal.ready.store(false, Ordering::Release);
            return if self.closed.load(Ordering::Acquire) {
                ImagePreparationPoll::Disconnected
            } else {
                ImagePreparationPoll::Empty
            };
        };
        self.pending
            .lock()
            .expect("inline image pending")
            .remove(&result.key);
        self.signal.pending.fetch_sub(1, Ordering::AcqRel);
        self.signal.ready.store(
            !self
                .results
                .lock()
                .expect("inline image results")
                .is_empty(),
            Ordering::Release,
        );
        ImagePreparationPoll::Ready(result)
    }

    fn signal(&self) -> Arc<ImagePreparationSignal> {
        Arc::clone(&self.signal)
    }

    fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
    }
}

pub(super) fn run_worker(
    preparer: Arc<dyn ImagePreparer>,
    requests: Receiver<ImagePreparationRequest>,
    results: SyncSender<ImagePreparationResult>,
    active: Arc<Mutex<(u64, u64)>>,
    signal: Arc<ImagePreparationSignal>,
) {
    while let Ok(request) = requests.recv() {
        let context = (request.key.generation, request.key.surface_epoch);
        let result = if *active.lock().expect("image preparation context") == context {
            let mut result = preparer.prepare(request);
            if *active.lock().expect("image preparation context") != context {
                result.rgba = None;
            }
            result
        } else {
            ImagePreparationResult {
                key: request.key,
                rgba: None,
            }
        };
        if results.send(result).is_err() {
            break;
        }
        signal.ready.store(true, Ordering::Release);
    }
}
