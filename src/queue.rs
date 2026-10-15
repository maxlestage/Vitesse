//! A background job queue (Laravel's queues, BullMQ, Sidekiq, in memory):
//! answer the request now, send the email later, with a cap on the jobs
//! that run at once, a rate limit, and retries with an exponential backoff.
//!
//! ```
//! use std::time::Duration;
//! use vitesse::prelude::*;
//! use vitesse::queue::Queue;
//!
//! async fn send_welcome_email(to: String) -> Result<(), std::io::Error> {
//!     // … talk to the mail server
//!     Ok(())
//! }
//!
//! let queue = Queue::builder()
//!     .workers(4) // at most 4 jobs at once
//!     .rate_limit(10, Duration::from_secs(1)) // at most 10 starts per second
//!     .retries(3) // a failed job runs again up to 3 times: after 1 s, 2 s, 4 s
//!     .backoff(Duration::from_secs(1))
//!     .start();
//!
//! let mut app = App::new();
//! app.state(queue);
//! app.post("/signup", |req: Request| async move {
//!     let email = req.text().await?;
//!     req.state::<Queue>().dispatch(move || send_welcome_email(email.clone()));
//!     Ok::<_, Error>((202, "Welcome! Check your inbox."))
//! });
//! ```
//!
//! Jobs live in memory: those still waiting are lost if the process stops.
//! For jobs that must survive a restart, use a queue backed by a database
//! or a broker.

use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::{Notify, Semaphore, mpsc};
use tokio::time::Instant;

pub use crate::background::TaskOutcome;
use crate::background::{run_task, spawn_background};
use crate::handler::BoxFuture;

type JobFn = Arc<dyn Fn() -> BoxFuture<Option<String>> + Send + Sync>;

struct Job {
    run: JobFn,
    /// Runs already made.
    attempts: u32,
    not_before: Option<std::time::Instant>,
}

/// How a [`Queue`] runs its jobs (see [`Queue::builder`]).
#[derive(Debug, Clone)]
pub struct QueueBuilder {
    workers: usize,
    retries: u32,
    backoff: Duration,
    rate_limit: Option<(usize, Duration)>,
}

impl QueueBuilder {
    /// The number of jobs that run at the same time (4 by default).
    pub fn workers(mut self, workers: usize) -> Self {
        self.workers = workers.clamp(1, Semaphore::MAX_PERMITS);
        self
    }

    /// How many times a failed job (an `Err`, or a panic) runs again (none
    /// by default).
    pub fn retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    /// The wait before the first retry, doubled at each new one (1 s by
    /// default).
    pub fn backoff(mut self, backoff: Duration) -> Self {
        self.backoff = backoff;
        self
    }

    /// At most `jobs` jobs start per `period` (Laravel's `RateLimited`
    /// job middleware): to respect the limits of an external API.
    pub fn rate_limit(mut self, jobs: usize, period: Duration) -> Self {
        self.rate_limit = Some((jobs.max(1), period));
        self
    }

    /// Starts the queue: its jobs run on the current tokio runtime if there
    /// is one, else on a thread of their own (before
    /// [`App::run`](crate::App::run)).
    pub fn start(self) -> Queue {
        let (tx, rx) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            tx,
            pending: AtomicUsize::new(0),
            idle: Notify::new(),
        });
        spawn_background(
            "vitesse-queue",
            dispatcher(self.clone(), rx, Arc::downgrade(&shared)),
        );
        Queue {
            shared,
            config: Arc::new(self),
        }
    }
}

struct Shared {
    tx: mpsc::UnboundedSender<Job>,
    /// Jobs waiting or running.
    pending: AtomicUsize,
    idle: Notify,
}

impl Shared {
    fn done(&self) {
        if self.pending.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.idle.notify_waiters();
        }
    }
}

/// A queue of background jobs; clones share it. See the [module](self).
#[derive(Clone)]
pub struct Queue {
    shared: Arc<Shared>,
    config: Arc<QueueBuilder>,
}

impl fmt::Debug for Queue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Queue")
            .field("pending", &self.pending())
            .field("config", &self.config)
            .finish()
    }
}

impl Queue {
    /// A queue to configure, then [`start`](QueueBuilder::start).
    pub fn builder() -> QueueBuilder {
        QueueBuilder {
            workers: 4,
            retries: 0,
            backoff: Duration::from_secs(1),
            rate_limit: None,
        }
    }

    /// A queue with the default settings: 4 workers, no retry, no rate
    /// limit.
    pub fn new() -> Queue {
        Queue::builder().start()
    }

    /// Adds a job: a function that returns a future, called again for each
    /// retry. Its output is nothing, or a `Result` whose error is logged
    /// (and retried).
    pub fn dispatch<F, Fut>(&self, job: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: TaskOutcome,
    {
        self.push(job, None);
    }

    /// Adds a job that starts after `delay` at the earliest.
    pub fn dispatch_after<F, Fut>(&self, delay: Duration, job: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: TaskOutcome,
    {
        self.push(job, Some(std::time::Instant::now() + delay));
    }

    fn push<F, Fut>(&self, job: F, not_before: Option<std::time::Instant>)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: TaskOutcome,
    {
        let run: JobFn = Arc::new(move || {
            let future = job();
            Box::pin(async move { future.await.failure() })
        });
        self.shared.pending.fetch_add(1, Ordering::AcqRel);
        let job = Job {
            run,
            attempts: 0,
            not_before,
        };
        if self.shared.tx.send(job).is_err() {
            self.shared.done();
            eprintln!("[vitesse] the job queue has stopped: job dropped");
        }
    }

    /// The jobs waiting or running.
    pub fn pending(&self) -> usize {
        self.shared.pending.load(Ordering::Acquire)
    }

    /// Waits until no job is waiting or running (in tests, before checking
    /// what the jobs did).
    pub async fn idle(&self) {
        loop {
            let notified = self.shared.idle.notified();
            if self.pending() == 0 {
                return;
            }
            notified.await;
        }
    }
}

impl Default for Queue {
    fn default() -> Self {
        Queue::new()
    }
}

async fn dispatcher(
    config: QueueBuilder,
    mut rx: mpsc::UnboundedReceiver<Job>,
    shared: std::sync::Weak<Shared>,
) {
    let workers = Arc::new(Semaphore::new(config.workers));
    let mut starts: VecDeque<Instant> = VecDeque::new();
    while let Some(job) = rx.recv().await {
        let Some(queue) = shared.upgrade() else {
            return;
        };
        if let Some(not_before) = job.not_before {
            let delay = not_before.saturating_duration_since(std::time::Instant::now());
            if !delay.is_zero() {
                tokio::spawn(async move {
                    tokio::time::sleep(delay).await;
                    let job = Job {
                        not_before: None,
                        ..job
                    };
                    if queue.tx.send(job).is_err() {
                        queue.done();
                    }
                });
                continue;
            }
        }
        if let Some((jobs, period)) = config.rate_limit {
            let now = Instant::now();
            while starts.front().is_some_and(|start| *start + period <= now) {
                starts.pop_front();
            }
            if starts.len() >= jobs {
                if let Some(first) = starts.pop_front() {
                    tokio::time::sleep_until(first + period).await;
                }
            }
            starts.push_back(Instant::now());
        }
        let Ok(permit) = workers.clone().acquire_owned().await else {
            return;
        };
        let backoff = config.backoff;
        let retries = config.retries;
        tokio::spawn(async move {
            let failure = run_task((job.run)()).await;
            drop(permit);
            let Some(error) = failure else {
                queue.done();
                return;
            };
            let attempts = job.attempts + 1;
            if attempts > retries {
                eprintln!("[vitesse] job failed after {attempts} attempts: {error}");
                queue.done();
                return;
            }
            let delay = backoff.saturating_mul(1 << (attempts - 1).min(16));
            let retry = Job {
                run: job.run,
                attempts,
                not_before: Some(std::time::Instant::now() + delay),
            };
            if queue.tx.send(retry).is_err() {
                queue.done();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[tokio::test]
    async fn runs_jobs() {
        let queue = Queue::new();
        let done = Arc::new(AtomicUsize::new(0));
        for _ in 0..10 {
            let done = done.clone();
            queue.dispatch(move || {
                let done = done.clone();
                async move {
                    done.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
        queue.idle().await;
        assert_eq!(done.load(Ordering::SeqCst), 10);
        assert_eq!(queue.pending(), 0);
    }

    #[tokio::test]
    async fn retries_with_backoff() {
        let queue = Queue::builder()
            .retries(2)
            .backoff(Duration::from_millis(5))
            .start();
        let attempts = Arc::new(AtomicUsize::new(0));
        let counter = attempts.clone();
        queue.dispatch(move || {
            let counter = counter.clone();
            async move {
                if counter.fetch_add(1, Ordering::SeqCst) < 2 {
                    return Err("not yet");
                }
                Ok(())
            }
        });
        // A job that always panics gives up after its retries.
        queue.dispatch(|| async {
            if true {
                panic!("boom");
            }
        });
        queue.idle().await;
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn limits() {
        let queue = Queue::builder()
            .workers(1)
            .rate_limit(2, Duration::from_millis(100))
            .start();
        let starts = Arc::new(Mutex::new(Vec::new()));
        for _ in 0..3 {
            let starts = starts.clone();
            queue.dispatch(move || {
                let starts = starts.clone();
                async move {
                    starts.lock().unwrap().push(std::time::Instant::now());
                }
            });
        }
        let begin = std::time::Instant::now();
        queue.dispatch_after(Duration::from_millis(30), || async {});
        queue.idle().await;
        let starts = starts.lock().unwrap();
        assert_eq!(starts.len(), 3);
        // The third start waited for the window of the first.
        assert!(starts[2].duration_since(starts[0]) >= Duration::from_millis(90));
        assert!(begin.elapsed() >= Duration::from_millis(30));
    }
}
