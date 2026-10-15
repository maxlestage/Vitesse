# Background jobs and scheduled tasks

Some work doesn't belong in the request: sending an email, resizing an image, calling a slow API, purging old sessions every night. Vitesse has a job queue, `vitesse::queue` (Laravel's queues, BullMQ, Sidekiq), and a scheduler, `vitesse::schedule` (Laravel's scheduler, cron), both in memory and both opt-in.

## The job queue

A `Queue` runs jobs in the background, with a cap on the jobs that run at once, an optional rate limit, and retries:

```rust
use std::time::Duration;
use vitesse::queue::Queue;

let queue = Queue::builder()
    .workers(4) // at most 4 jobs at once (the default)
    .rate_limit(10, Duration::from_secs(1)) // at most 10 starts per second
    .retries(3) // a failed job runs again up to 3 times…
    .backoff(Duration::from_secs(1)) // …after 1 s, then 2 s, then 4 s
    .start();

let mut app = App::new();
app.state(queue);
```

`Queue::new()` starts a queue with the default settings: 4 workers, no retry, no rate limit.

## Dispatching jobs

A job is a function that returns a future, called again for each retry. It returns nothing, or a `Result` whose error is logged and retried:

```rust
async fn send_welcome_email(to: String) -> Result<(), std::io::Error> {
    // … talk to the mail server
    Ok(())
}

app.post("/signup", |req: Request| async move {
    let email = req.text().await?;
    req.state::<Queue>().dispatch(move || send_welcome_email(email.clone()));
    Ok::<_, Error>((202, "Welcome! Check your inbox."))
});
```

- `dispatch_after(delay, job)` starts the job after a delay at the earliest.
- A job that panics counts as failed: it is retried like an error.
- `queue.pending()` counts the jobs waiting or running, and `queue.idle().await` waits until there are none, handy in tests before checking what the jobs did.

## Scheduled tasks

A `Scheduler` runs functions at regular times, in the background of the server:

```rust
use std::time::Duration;
use vitesse::schedule::Scheduler;

fn main() -> std::io::Result<()> {
    let scheduler = Scheduler::new()
        .every(Duration::from_secs(30), "heartbeat", || async { println!("alive") })
        .every_minute("refresh rates", refresh_exchange_rates)
        .hourly_at(5, "purge sessions", purge_sessions) // at :05 every hour
        .daily_at(3, 30, "nightly report", send_report) // at 03:30 UTC every day
        .start();

    let mut app = App::new();
    app.get("/", |_| async { "Hello!" });
    app.run(3000)?;
    scheduler.stop();
    Ok(())
}
```

- Like jobs, a task returns nothing or a `Result`; errors and panics are logged, and the task runs again next time.
- A task still running when its next run comes is not started twice (Laravel's `withoutOverlapping`).
- Times are in UTC. `every(interval, …)` runs a first time one interval after the start.
- `scheduler.stop()` stops the tasks; dropping the handle leaves them running.

## Where they run

The queue and the scheduler run on the current tokio runtime when there is one (`#[tokio::main]`, `app.listen`, tests), and otherwise on a thread of their own with its own runtime: create them before `app.run(…)` and they keep running next to the server.

## Limits

Jobs and schedules live in the memory of the process: the jobs still waiting are lost if it stops, and each instance of the application runs its own scheduler. For jobs that must survive a restart, use a queue backed by a database or a broker; with several instances, run the scheduler in one of them only.
