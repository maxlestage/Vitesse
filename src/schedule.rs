//! Scheduled tasks (Laravel's scheduler, cron inside the application):
//! run a function every few seconds, every hour or every day at a given
//! time, in the background of the server.
//!
//! ```no_run
//! use std::time::Duration;
//! use vitesse::prelude::*;
//! use vitesse::schedule::Scheduler;
//!
//! async fn purge_sessions() -> Result<(), std::io::Error> {
//!     // … delete the expired sessions
//!     Ok(())
//! }
//!
//! fn main() -> std::io::Result<()> {
//!     let scheduler = Scheduler::new()
//!         .every(Duration::from_secs(30), "heartbeat", || async { println!("alive") })
//!         .hourly_at(5, "purge sessions", purge_sessions)
//!         .daily_at(3, 30, "nightly report", || async {
//!             // … 03:30 UTC every day
//!         })
//!         .start();
//!
//!     let mut app = App::new();
//!     app.get("/", |_| async { "Hello!" });
//!     app.run(3000)?;
//!     scheduler.stop();
//!     Ok(())
//! }
//! ```
//!
//! A task returns nothing, or a `Result` whose error is logged. A task
//! that is still running when its next run comes is not started twice
//! (Laravel's `withoutOverlapping`); a panic is logged and the task runs
//! again next time. Times are in UTC. Each instance of the application
//! runs its own scheduler: with several instances, run it in one.

use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::watch;

pub use crate::background::TaskOutcome;
use crate::background::{run_task, spawn_background};
use crate::handler::BoxFuture;

type TaskFn = Arc<dyn Fn() -> BoxFuture<Option<String>> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    Every(Duration),
    /// At this second of every hour.
    Hourly(u64),
    /// At this second of every day (UTC).
    Daily(u64),
}

impl When {
    /// How long until the next run, `now` being seconds since the epoch.
    fn wait(self, now: Duration) -> Duration {
        let after = |period: u64, at: u64| {
            let since = now.as_secs() % period;
            let mut wait = Duration::from_secs((at + period - since) % period)
                .saturating_sub(Duration::from_nanos(u64::from(now.subsec_nanos())));
            if wait.is_zero() {
                wait = Duration::from_secs(period);
            }
            wait
        };
        match self {
            When::Every(interval) => interval,
            When::Hourly(at) => after(3600, at),
            When::Daily(at) => after(86_400, at),
        }
    }
}

struct Task {
    name: String,
    when: When,
    run: TaskFn,
}

/// The list of scheduled tasks, started with [`Scheduler::start`]. See the
/// [module](self).
#[derive(Default)]
pub struct Scheduler {
    tasks: Vec<Task>,
}

impl fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tasks: Vec<(&str, When)> = self.tasks.iter().map(|t| (&*t.name, t.when)).collect();
        f.debug_struct("Scheduler").field("tasks", &tasks).finish()
    }
}

fn boxed<F, Fut>(task: F) -> TaskFn
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future + Send + 'static,
    Fut::Output: TaskOutcome,
{
    Arc::new(move || {
        let future = task();
        Box::pin(async move { future.await.failure() })
    })
}

impl Scheduler {
    /// No task yet.
    pub fn new() -> Self {
        Scheduler::default()
    }

    fn add(mut self, when: When, name: &str, run: TaskFn) -> Self {
        self.tasks.push(Task {
            name: name.to_owned(),
            when,
            run,
        });
        self
    }

    /// Runs `task` every `interval`, the first time one interval after the
    /// start.
    pub fn every<F, Fut>(self, interval: Duration, name: &str, task: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: TaskOutcome,
    {
        let interval = interval.max(Duration::from_millis(1));
        self.add(When::Every(interval), name, boxed(task))
    }

    /// Runs `task` every minute.
    pub fn every_minute<F, Fut>(self, name: &str, task: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: TaskOutcome,
    {
        self.every(Duration::from_secs(60), name, task)
    }

    /// Runs `task` every hour, at `minute` past the hour.
    ///
    /// # Panics
    /// If `minute` is 60 or more.
    #[track_caller]
    pub fn hourly_at<F, Fut>(self, minute: u32, name: &str, task: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: TaskOutcome,
    {
        assert!(minute < 60, "the minute must be below 60, not {minute}");
        self.add(When::Hourly(u64::from(minute) * 60), name, boxed(task))
    }

    /// Runs `task` every day at `hour:minute`, UTC.
    ///
    /// # Panics
    /// If the hour is 24 or more, or the minute 60 or more.
    #[track_caller]
    pub fn daily_at<F, Fut>(self, hour: u32, minute: u32, name: &str, task: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: TaskOutcome,
    {
        assert!(hour < 24 && minute < 60, "invalid time {hour}:{minute}");
        let at = u64::from(hour) * 3600 + u64::from(minute) * 60;
        self.add(When::Daily(at), name, boxed(task))
    }

    /// Starts the tasks in the background: on the current tokio runtime if
    /// there is one, else on a thread of their own (before
    /// [`App::run`](crate::App::run)). They run until
    /// [`SchedulerHandle::stop`], or until the program ends.
    pub fn start(self) -> SchedulerHandle {
        let (stop, stopped) = watch::channel(false);
        let tasks = self.tasks;
        spawn_background("vitesse-scheduler", async move {
            let mut handles = Vec::new();
            for task in tasks {
                handles.push(tokio::spawn(run_schedule(task, stopped.clone())));
            }
            for handle in handles {
                let _ = handle.await;
            }
        });
        SchedulerHandle { stop }
    }
}

async fn run_schedule(task: Task, mut stopped: watch::Receiver<bool>) {
    let running = Arc::new(AtomicBool::new(false));
    loop {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        tokio::select! {
            _ = tokio::time::sleep(task.when.wait(now)) => {}
            _ = stopped.changed() => return,
        }
        if *stopped.borrow() {
            return;
        }
        if running.swap(true, Ordering::AcqRel) {
            continue; // The previous run is not over: skip this one.
        }
        let run = (task.run)();
        let running = running.clone();
        let name = task.name.clone();
        tokio::spawn(async move {
            if let Some(error) = run_task(run).await {
                eprintln!("[vitesse] scheduled task '{name}' failed: {error}");
            }
            running.store(false, Ordering::Release);
        });
    }
}

/// Stops the tasks started by [`Scheduler::start`]. Dropping it leaves
/// them running.
#[derive(Debug, Clone)]
pub struct SchedulerHandle {
    stop: watch::Sender<bool>,
}

impl SchedulerHandle {
    /// Stops the tasks: no new run starts (a run in progress finishes).
    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn next_runs() {
        let at = |h: u64, m: u64, s: u64| Duration::from_secs(10 * 86_400 + h * 3600 + m * 60 + s);
        assert_eq!(
            When::Daily(3 * 3600).wait(at(2, 0, 0)),
            Duration::from_secs(3600)
        );
        assert_eq!(
            When::Daily(3 * 3600).wait(at(4, 0, 0)),
            Duration::from_secs(23 * 3600)
        );
        assert_eq!(
            When::Daily(3 * 3600).wait(at(3, 0, 0)),
            Duration::from_secs(86_400)
        );
        assert_eq!(
            When::Hourly(300).wait(at(7, 4, 30)),
            Duration::from_secs(30)
        );
        assert_eq!(
            When::Hourly(300).wait(at(7, 6, 0)),
            Duration::from_secs(59 * 60)
        );
        assert_eq!(
            When::Every(Duration::from_secs(5)).wait(at(0, 0, 0)),
            Duration::from_secs(5)
        );
    }

    #[tokio::test]
    async fn runs_and_stops() {
        let runs = Arc::new(AtomicUsize::new(0));
        let counter = runs.clone();
        let handle = Scheduler::new()
            .every(Duration::from_millis(10), "count", move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            })
            .start();
        tokio::time::sleep(Duration::from_millis(200)).await;
        handle.stop();
        tokio::time::sleep(Duration::from_millis(30)).await;
        let seen = runs.load(Ordering::SeqCst);
        assert!(seen >= 3, "only {seen} runs");
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(runs.load(Ordering::SeqCst), seen);
    }

    #[tokio::test]
    async fn no_overlap() {
        let started = Arc::new(AtomicUsize::new(0));
        let counter = started.clone();
        let handle = Scheduler::new()
            .every(Duration::from_millis(5), "slow", move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                    Err::<(), _>("boom")
                }
            })
            .start();
        tokio::time::sleep(Duration::from_millis(100)).await;
        handle.stop();
        assert_eq!(started.load(Ordering::SeqCst), 1);
    }
}
