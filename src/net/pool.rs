use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use super::fetch::{Fetch, FetchPayload, FetchRequest};

struct Job {
    generation: u64,
    request: FetchRequest,
}

struct State {
    jobs: VecDeque<Job>,
    closed: bool,
}

struct Shared {
    state: Mutex<State>,
    cond: Condvar,
}

pub struct FetchPool {
    shared: Arc<Shared>,
    results: Mutex<Receiver<FetchPayload>>,
}

impl FetchPool {
    pub fn spawn(fetch: Arc<dyn Fetch>, worker_count: usize) -> Self {
        assert!(worker_count > 0, "worker count must be positive");
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                jobs: VecDeque::new(),
                closed: false,
            }),
            cond: Condvar::new(),
        });
        let (results_tx, results_rx) = std::sync::mpsc::channel();
        for _ in 0..worker_count {
            let shared = Arc::clone(&shared);
            let fetch = Arc::clone(&fetch);
            let results_tx = results_tx.clone();
            thread::spawn(move || worker_loop(shared, fetch, results_tx));
        }
        Self {
            shared,
            results: Mutex::new(results_rx),
        }
    }

    pub fn submit(&self, generation: u64, request: FetchRequest) {
        let mut state = self.shared.state.lock().expect("pool state lock");
        state.jobs.push_back(Job {
            generation,
            request,
        });
        drop(state);
        self.shared.cond.notify_one();
    }

    pub fn try_recv(&self) -> Option<FetchPayload> {
        match self.results.lock().expect("pool results lock").try_recv() {
            Ok(payload) => Some(payload),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => None,
        }
    }

    pub fn shutdown(&self) {
        let mut state = self.shared.state.lock().expect("pool state lock");
        state.closed = true;
        drop(state);
        self.shared.cond.notify_all();
    }
}

fn worker_loop(shared: Arc<Shared>, fetch: Arc<dyn Fetch>, results: Sender<FetchPayload>) {
    loop {
        let mut state = shared.state.lock().expect("pool state lock");
        let job = loop {
            if let Some(job) = state.jobs.pop_front() {
                break Some(job);
            }
            if state.closed {
                break None;
            }
            state = shared.cond.wait(state).expect("pool condvar wait");
        };
        drop(state);
        let Some(job) = job else { break };
        let result = fetch.fetch(&job.request);
        let _ = results.send(FetchPayload {
            generation: job.generation,
            result,
        });
        shared.cond.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fetch::{FetchError, FetchResponse};
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};
    use url::Url;

    const WORKERS: usize = 4;

    struct Recorder {
        current: AtomicUsize,
        max: AtomicUsize,
    }

    impl Recorder {
        fn new() -> Self {
            Self {
                current: AtomicUsize::new(0),
                max: AtomicUsize::new(0),
            }
        }
    }

    impl Default for Recorder {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Fetch for Recorder {
        fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
            let current = self.current.fetch_add(1, Ordering::SeqCst) + 1;
            self.max.fetch_max(current, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(10));
            self.current.fetch_sub(1, Ordering::SeqCst);
            Ok(FetchResponse {
                final_url: request.url.clone(),
                body: vec![],
                content_type: None,
            })
        }
    }

    struct Mixed {
        speed: Duration,
    }

    impl Fetch for Mixed {
        fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
            if request.url.path() == "/hung" {
                thread::sleep(Duration::from_millis(600));
            } else {
                thread::sleep(self.speed);
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
        let deadline = Instant::now() + Duration::from_secs(5);
        while out.len() < n {
            if let Some(payload) = pool.try_recv() {
                out.push(payload);
                continue;
            }
            assert!(Instant::now() < deadline, "timed out waiting for results");
            thread::sleep(Duration::from_millis(2));
        }
        out
    }

    #[test]
    fn concurrency_never_exceeds_the_pool_size() {
        let fetch = Arc::new(Recorder::default());
        let probe = Arc::clone(&fetch);
        let pool = FetchPool::spawn(fetch, WORKERS);
        for i in 0..24 {
            pool.submit(i, url(&format!("/job{i}")));
        }
        assert_eq!(drain_n(&pool, 24).len(), 24);
        assert!(probe.max.load(Ordering::SeqCst) <= WORKERS);
        assert_eq!(probe.current.load(Ordering::SeqCst), 0);
        pool.shutdown();
    }

    #[test]
    fn a_hung_job_does_not_block_others() {
        let pool = FetchPool::spawn(
            Arc::new(Mixed {
                speed: Duration::from_millis(5),
            }),
            WORKERS,
        );
        let start = Instant::now();
        pool.submit(0, url("/hung"));
        thread::sleep(Duration::from_millis(20));
        for i in 1..=12 {
            pool.submit(i, url(&format!("/job{i}")));
        }
        let mut fast = Vec::new();
        while fast.len() < 12 {
            if let Some(payload) = pool.try_recv()
                && payload.generation != 0
            {
                fast.push(payload.generation);
            }
            assert!(
                start.elapsed() < Duration::from_millis(400),
                "fast jobs did not finish before the hung one"
            );
            thread::sleep(Duration::from_millis(1));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let hung = loop {
            if let Some(payload) = pool.try_recv() {
                break payload;
            }
            assert!(Instant::now() < deadline, "hung job was never delivered");
            thread::sleep(Duration::from_millis(2));
        };
        assert_eq!(hung.generation, 0);
        pool.shutdown();
    }

    #[test]
    fn results_carry_their_generation_regardless_of_completion_order() {
        let pool = FetchPool::spawn(
            Arc::new(Mixed {
                speed: Duration::from_millis(1),
            }),
            WORKERS,
        );
        for i in 0..32 {
            pool.submit(i, url(&format!("/job{i}")));
        }
        let results = drain_n(&pool, 32);
        let gens: HashSet<u64> = results.iter().map(|p| p.generation).collect();
        assert_eq!(gens.len(), 32);
        for i in 0..32 {
            assert!(gens.contains(&i));
        }
        pool.shutdown();
    }
}
