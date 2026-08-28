use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};

use super::fetch::{Fetch, FetchPayload, FetchPoll, FetchRequest, ResourceId, Submitted};

struct Job {
    tab_id: u64,
    generation: u64,
    resource_id: ResourceId,
    request: FetchRequest,
}

#[derive(Default)]
struct JobState {
    live: HashSet<(u64, u64, ResourceId)>,
    canceled: HashSet<(u64, u64, ResourceId)>,
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

    pub fn submit(
        &self,
        tab_id: u64,
        generation: u64,
        resource_id: ResourceId,
        request: FetchRequest,
    ) -> Submitted {
        if self.shared.closed.load(Ordering::Acquire) {
            return Submitted::Closed;
        }
        let key = (tab_id, generation, resource_id);
        if !self
            .shared
            .state
            .lock()
            .expect("pool job state lock")
            .live
            .insert(key)
        {
            return Submitted::Duplicate;
        }
        if self
            .jobs
            .send(Some(Job {
                tab_id,
                generation,
                resource_id,
                request,
            }))
            .is_err()
        {
            finish(&self.shared, tab_id, generation, resource_id);
            return Submitted::Closed;
        }
        Submitted::Queued
    }

    pub fn cancel(&self, tab_id: u64, generation: u64) {
        let mut state = self.shared.state.lock().expect("pool job state lock");
        let resources: Vec<_> = state
            .live
            .iter()
            .copied()
            .filter(|(live_tab, live_generation, _)| {
                *live_tab == tab_id && *live_generation == generation
            })
            .collect();
        for key in resources {
            state.canceled.insert(key);
        }
    }

    pub fn try_recv(&self) -> FetchPoll {
        match self.results.try_recv() {
            Ok(payload) => FetchPoll::Ready(payload),
            Err(TryRecvError::Empty) => FetchPoll::Empty,
            Err(TryRecvError::Disconnected) => FetchPoll::Disconnected,
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

    pub fn shutdown_without_waiting(&self) {
        if self.shared.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        let mut workers = self.workers.lock().expect("pool workers lock");
        for _ in 0..workers.len() {
            let _ = self.jobs.send(None);
        }
        workers.clear();
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
        if is_canceled(&shared, job.tab_id, job.generation, job.resource_id) {
            finish(&shared, job.tab_id, job.generation, job.resource_id);
            continue;
        }
        let result = fetch.fetch(&job.request);
        if !is_canceled(&shared, job.tab_id, job.generation, job.resource_id) {
            let _ = results.send(FetchPayload {
                tab_id: job.tab_id,
                generation: job.generation,
                resource_id: job.resource_id,
                result,
            });
        }
        finish(&shared, job.tab_id, job.generation, job.resource_id);
    }
}

fn is_canceled(shared: &Shared, tab_id: u64, generation: u64, resource_id: ResourceId) -> bool {
    shared
        .state
        .lock()
        .expect("pool job state lock")
        .canceled
        .contains(&(tab_id, generation, resource_id))
}

fn finish(shared: &Shared, tab_id: u64, generation: u64, resource_id: ResourceId) {
    let key = (tab_id, generation, resource_id);
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
    use std::time::Duration;
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
                status: 200,
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
                status: 200,
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
                status: 200,
                body: vec![],
                content_type: None,
            })
        }
    }

    fn url(path: &str) -> FetchRequest {
        FetchRequest::get(Url::parse(&format!("http://example.com{path}")).expect("url"))
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
            pool.submit(0, i, ResourceId::DOCUMENT, url(&format!("/job{i}")));
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
        pool.submit(0, 0, ResourceId::DOCUMENT, url("/hung"));
        hung_started_rx.recv().expect("hung worker did not start");
        for i in 1..=12 {
            pool.submit(0, i, ResourceId::DOCUMENT, url(&format!("/job{i}")));
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
            pool.submit(0, i, ResourceId::DOCUMENT, url(&format!("/job{i}")));
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
        pool.submit(7, 1, ResourceId::DOCUMENT, url("/first"));
        first_started_rx.recv().expect("first job did not start");
        pool.submit(7, 2, ResourceId::DOCUMENT, url("/canceled"));
        pool.cancel(7, 2);
        pool.submit(7, 3, ResourceId::DOCUMENT, url("/sentinel"));
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

    #[test]
    fn distinct_resources_in_one_generation_are_both_delivered() {
        let pool = FetchPool::spawn(
            Arc::new(Mixed {
                hung_started: unbounded().0,
                hung_release: unbounded().1,
            }),
            WORKERS,
        );
        pool.submit(3, 9, ResourceId(1), url("/one"));
        pool.submit(3, 9, ResourceId(2), url("/two"));
        let ids: HashSet<_> = drain_n(&pool, 2)
            .into_iter()
            .map(|payload| payload.resource_id)
            .collect();
        assert_eq!(ids, HashSet::from([ResourceId(1), ResourceId(2)]));
        pool.shutdown();
    }

    #[test]
    fn a_submitted_job_reports_whether_the_pool_took_it() {
        let (first_started_tx, first_started_rx) = unbounded();
        let (release_tx, release_rx) = unbounded();
        let pool = FetchPool::spawn(
            Arc::new(GateFirst {
                first_started: first_started_tx,
                first_release: release_rx,
                observed: unbounded().0,
            }),
            1,
        );
        assert_eq!(
            pool.submit(1, 2, ResourceId(3), url("/first")),
            Submitted::Queued
        );
        first_started_rx.recv().expect("first job did not start");
        assert_eq!(
            pool.submit(1, 2, ResourceId(3), url("/duplicate")),
            Submitted::Duplicate,
            "a live key is coalesced, and the caller is told so instead of waiting \
             for a payload that will never arrive"
        );
        release_tx.send(()).expect("release first sender");
        assert_eq!(drain_n(&pool, 1)[0].resource_id, ResourceId(3));
        pool.shutdown();
        assert_eq!(
            pool.submit(1, 9, ResourceId(4), url("/after-shutdown")),
            Submitted::Closed
        );
    }

    #[test]
    fn a_pool_with_no_senders_left_reports_disconnected_not_empty() {
        let pool = FetchPool::spawn(
            Arc::new(Mixed {
                hung_started: unbounded().0,
                hung_release: unbounded().1,
            }),
            1,
        );
        assert_eq!(pool.try_recv(), FetchPoll::Empty);
        pool.submit(0, 0, ResourceId::DOCUMENT, url("/one"));
        let _ = drain_n(&pool, 1);
        pool.shutdown();
        assert_eq!(
            pool.try_recv(),
            FetchPoll::Disconnected,
            "a dead pool must not look like an idle one"
        );
    }

    #[test]
    fn detaching_a_parked_worker_returns_instead_of_joining_it() {
        let (first_started_tx, first_started_rx) = unbounded();
        let (release_tx, release_rx) = unbounded();
        let pool = FetchPool::spawn(
            Arc::new(GateFirst {
                first_started: first_started_tx,
                first_release: release_rx,
                observed: unbounded().0,
            }),
            1,
        );
        pool.submit(1, 1, ResourceId::DOCUMENT, url("/first"));
        first_started_rx.recv().expect("first job did not start");
        // The worker is parked exactly as one waiting out the 30 s fetch timeout is.
        // `shutdown` would block here; the quit path must not.
        let started = std::time::Instant::now();
        pool.shutdown_without_waiting();
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "quitting waited on a parked worker"
        );
        // `Drop` must not join it either, now that `closed` is already set.
        drop(pool);
        assert!(started.elapsed() < Duration::from_secs(1));
        release_tx.send(()).expect("release detached worker");
    }

    #[test]
    fn duplicate_live_resource_jobs_are_coalesced() {
        let (first_started_tx, first_started_rx) = unbounded();
        let (release_tx, release_rx) = unbounded();
        let pool = FetchPool::spawn(
            Arc::new(GateFirst {
                first_started: first_started_tx,
                first_release: release_rx,
                observed: unbounded().0,
            }),
            1,
        );
        pool.submit(1, 2, ResourceId(3), url("/first"));
        first_started_rx.recv().expect("first job did not start");
        pool.submit(1, 2, ResourceId(3), url("/duplicate"));
        release_tx.send(()).expect("release first sender");
        assert_eq!(drain_n(&pool, 1)[0].resource_id, ResourceId(3));
        assert!(pool.results.try_recv().is_err());
        pool.shutdown();
    }

    #[test]
    fn canceling_a_generation_cancels_every_resource() {
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
        pool.submit(7, 1, ResourceId::DOCUMENT, url("/first"));
        first_started_rx.recv().expect("first job did not start");
        pool.submit(7, 2, ResourceId(1), url("/one"));
        pool.submit(7, 2, ResourceId(2), url("/two"));
        pool.cancel(7, 2);
        pool.submit(7, 3, ResourceId::DOCUMENT, url("/sentinel"));
        release_tx.send(()).expect("release first sender");
        let _ = pool.results.recv().expect("first result");
        assert_eq!(observed_rx.recv().expect("sentinel fetch"), "/sentinel");
        let sentinel = pool.results.recv().expect("sentinel result");
        assert_eq!(sentinel.generation, 3);
        assert!(pool.results.try_recv().is_err());
        pool.shutdown();
    }

    #[test]
    fn shutdown_without_waiting_detaches_in_flight_workers() {
        let (first_started_tx, first_started_rx) = unbounded();
        let (release_tx, release_rx) = unbounded();
        let pool = FetchPool::spawn(
            Arc::new(GateFirst {
                first_started: first_started_tx,
                first_release: release_rx,
                observed: unbounded().0,
            }),
            1,
        );
        pool.submit(1, 1, ResourceId::DOCUMENT, url("/first"));
        first_started_rx.recv().expect("first job did not start");
        pool.shutdown_without_waiting();
        assert!(pool.shared.closed.load(Ordering::Acquire));
        assert!(pool.workers.lock().expect("pool workers lock").is_empty());
        release_tx.send(()).expect("release detached worker");
    }
}
