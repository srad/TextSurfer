use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};

use super::fetch::{Fetch, FetchPayload, FetchRequest};

struct Job {
    tab_id: u64,
    generation: u64,
    request: FetchRequest,
}

#[derive(Default)]
struct JobState {
    live: HashSet<(u64, u64)>,
    canceled: HashSet<(u64, u64)>,
}

struct Shared {
    state: Mutex<JobState>,
    closed: AtomicBool,
}

pub struct FetchPool {
    shared: Arc<Shared>,
    jobs: Sender<Option<Job>>,
    results: Receiver<FetchPayload>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl FetchPool {
    pub fn spawn(fetch: Arc<dyn Fetch>, worker_count: usize) -> Self {
        assert!(worker_count > 0, "worker count must be positive");
        let shared = Arc::new(Shared {
            state: Mutex::new(JobState::default()),
            closed: AtomicBool::new(false),
        });
        let (jobs_tx, jobs_rx) = unbounded();
        let (results_tx, results_rx) = unbounded();
        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let shared = Arc::clone(&shared);
            let fetch = Arc::clone(&fetch);
            let results_tx = results_tx.clone();
            let jobs_rx = jobs_rx.clone();
            workers.push(thread::spawn(move || {
                worker_loop(shared, jobs_rx, fetch, results_tx)
            }));
        }
        Self {
            shared,
            jobs: jobs_tx,
            results: results_rx,
            workers: Mutex::new(workers),
        }
    }

    pub fn submit(&self, tab_id: u64, generation: u64, request: FetchRequest) {
        if !self.shared.closed.load(Ordering::Acquire) {
            let key = (tab_id, generation);
            if !self
                .shared
                .state
                .lock()
                .expect("pool job state lock")
                .live
                .insert(key)
            {
                return;
            }
            if self
                .jobs
                .send(Some(Job {
                    tab_id,
                    generation,
                    request,
                }))
                .is_err()
            {
                finish(&self.shared, tab_id, generation);
            }
        }
    }

    pub fn cancel(&self, tab_id: u64, generation: u64) {
        let key = (tab_id, generation);
        let mut state = self.shared.state.lock().expect("pool job state lock");
        if state.live.contains(&key) {
            state.canceled.insert(key);
        }
    }

    pub fn try_recv(&self) -> Option<FetchPayload> {
        match self.results.try_recv() {
            Ok(payload) => Some(payload),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => None,
        }
    }

    pub fn shutdown(&self) {
        if self.shared.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        let mut workers = self.workers.lock().expect("pool workers lock");
        for _ in 0..workers.len() {
            let _ = self.jobs.send(None);
        }
        for worker in workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl Drop for FetchPool {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker_loop(
    shared: Arc<Shared>,
    jobs: Receiver<Option<Job>>,
    fetch: Arc<dyn Fetch>,
    results: Sender<FetchPayload>,
) {
    while let Ok(Some(job)) = jobs.recv() {
        if is_canceled(&shared, job.tab_id, job.generation) {
            finish(&shared, job.tab_id, job.generation);
            continue;
        }
        let result = fetch.fetch(&job.request);
        if !is_canceled(&shared, job.tab_id, job.generation) {
            let _ = results.send(FetchPayload {
                tab_id: job.tab_id,
                generation: job.generation,
                result,
            });
        }
        finish(&shared, job.tab_id, job.generation);
    }
}

fn is_canceled(shared: &Shared, tab_id: u64, generation: u64) -> bool {
    shared
        .state
        .lock()
        .expect("pool job state lock")
        .canceled
        .contains(&(tab_id, generation))
}

fn finish(shared: &Shared, tab_id: u64, generation: u64) {
    let key = (tab_id, generation);
    let mut state = shared.state.lock().expect("pool job state lock");
    state.live.remove(&key);
    state.canceled.remove(&key);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fetch::{FetchError, FetchResponse};
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use url::Url;

    const WORKERS: usize = 4;

    struct Recorder {
        current: AtomicUsize,
        max: AtomicUsize,
        started: Sender<String>,
        release: Receiver<()>,
    }

    impl Fetch for Recorder {
        fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
            let current = self.current.fetch_add(1, Ordering::SeqCst) + 1;
            self.max.fetch_max(current, Ordering::SeqCst);
            self.started
                .send(request.url.path().to_string())
                .expect("started receiver");
            self.release.recv().expect("release worker");
            self.current.fetch_sub(1, Ordering::SeqCst);
            Ok(FetchResponse {
                final_url: request.url.clone(),
                body: vec![],
                content_type: None,
            })
        }
    }

    struct Mixed {
        hung_started: Sender<()>,
        hung_release: Receiver<()>,
    }

    struct GateFirst {
        first_started: Sender<()>,
        first_release: Receiver<()>,
        observed: Sender<String>,
    }

    impl Fetch for GateFirst {
        fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
            if request.url.path() == "/first" {
                self.first_started.send(()).expect("first started receiver");
                self.first_release.recv().expect("release first worker");
            } else {
                self.observed
                    .send(request.url.path().to_string())
                    .expect("observed receiver");
            }
            Ok(FetchResponse {
                final_url: request.url.clone(),
                body: vec![],
                content_type: None,
            })
        }
    }

    impl Fetch for Mixed {
        fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
            if request.url.path() == "/hung" {
                self.hung_started.send(()).expect("hung started receiver");
                self.hung_release.recv().expect("release hung worker");
            }
            Ok(FetchResponse {
                final_url: request.url.clone(),
                body: vec![],
                content_type: None,
            })
        }
    }

    fn url(path: &str) -> FetchRequest {
        FetchRequest {
            url: Url::parse(&format!("http://example.com{path}")).expect("url"),
        }
    }

    fn drain_n(pool: &FetchPool, n: usize) -> Vec<FetchPayload> {
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            out.push(
                pool.results
                    .recv()
                    .expect("result channel closed before delivery"),
            );
        }
        out
    }

    #[test]
    fn concurrency_never_exceeds_the_pool_size() {
        let (started_tx, started_rx) = unbounded();
        let (release_tx, release_rx) = unbounded();
        let fetch = Arc::new(Recorder {
            current: AtomicUsize::new(0),
            max: AtomicUsize::new(0),
            started: started_tx,
            release: release_rx,
        });
        let probe = Arc::clone(&fetch);
        let pool = FetchPool::spawn(fetch, WORKERS);
        for i in 0..24 {
            pool.submit(0, i, url(&format!("/job{i}")));
        }
        for _ in 0..WORKERS {
            started_rx.recv().expect("workers did not start");
        }
        assert_eq!(probe.max.load(Ordering::SeqCst), WORKERS);
        for _ in 0..24 {
            release_tx.send(()).expect("release sender");
        }
        assert_eq!(drain_n(&pool, 24).len(), 24);
        assert!(probe.max.load(Ordering::SeqCst) <= WORKERS);
        assert_eq!(probe.current.load(Ordering::SeqCst), 0);
        pool.shutdown();
    }

    #[test]
    fn a_hung_job_does_not_block_others() {
        let (hung_started_tx, hung_started_rx) = unbounded();
        let (hung_release_tx, hung_release_rx) = unbounded();
        let pool = FetchPool::spawn(
            Arc::new(Mixed {
                hung_started: hung_started_tx,
                hung_release: hung_release_rx,
            }),
            WORKERS,
        );
        pool.submit(0, 0, url("/hung"));
        hung_started_rx.recv().expect("hung worker did not start");
        for i in 1..=12 {
            pool.submit(0, i, url(&format!("/job{i}")));
        }
        let mut fast = Vec::new();
        while fast.len() < 12 {
            let payload = pool.results.recv().expect("fast jobs did not finish");
            if payload.generation != 0 {
                fast.push(payload.generation);
            }
        }
        hung_release_tx.send(()).expect("release hung sender");
        let hung = pool.results.recv().expect("hung job was never delivered");
        assert_eq!(hung.generation, 0);
        pool.shutdown();
    }

    #[test]
    fn results_carry_their_generation_regardless_of_completion_order() {
        let pool = FetchPool::spawn(
            Arc::new(Mixed {
                hung_started: unbounded().0,
                hung_release: unbounded().1,
            }),
            WORKERS,
        );
        for i in 0..32 {
            pool.submit(0, i, url(&format!("/job{i}")));
        }
        let results = drain_n(&pool, 32);
        let gens: HashSet<u64> = results.iter().map(|p| p.generation).collect();
        assert_eq!(gens.len(), 32);
        for i in 0..32 {
            assert!(gens.contains(&i));
        }
        pool.shutdown();
    }

    #[test]
    fn canceled_queued_jobs_are_never_started_or_delivered() {
        let (first_started_tx, first_started_rx) = unbounded();
        let (release_tx, release_rx) = unbounded();
        let (observed_tx, observed_rx) = unbounded();
        let pool = FetchPool::spawn(
            Arc::new(GateFirst {
                first_started: first_started_tx,
                first_release: release_rx,
                observed: observed_tx,
            }),
            1,
        );
        pool.submit(7, 1, url("/first"));
        first_started_rx.recv().expect("first job did not start");
        pool.submit(7, 2, url("/canceled"));
        pool.cancel(7, 2);
        pool.submit(7, 3, url("/sentinel"));
        release_tx.send(()).expect("release first sender");
        let delivered = pool.results.recv().expect("first result");
        assert_eq!(delivered.generation, 1);
        assert_eq!(
            observed_rx.recv().expect("sentinel was not observed"),
            "/sentinel"
        );
        let sentinel = pool.results.recv().expect("sentinel result");
        assert_eq!(sentinel.generation, 3);
        assert!(pool.results.try_recv().is_err());
        pool.shutdown();
        let state = pool.shared.state.lock().expect("pool job state lock");
        assert!(state.live.is_empty());
        assert!(state.canceled.is_empty());
    }
}
