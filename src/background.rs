//! Background tasks: the runtime they run on and what they return.

/// Runs `future` in the background: on the current tokio runtime if there
/// is one, else on a thread of its own with its own runtime (before
/// `app.run(…)`, which starts its runtimes later).
pub(crate) fn spawn_background<F>(name: &str, future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(future);
        }
        Err(_) => {
            let thread = name.to_owned();
            let spawned = std::thread::Builder::new()
                .name(name.to_owned())
                .spawn(move || {
                    let name = thread;
                    match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(runtime) => runtime.block_on(future),
                        Err(e) => eprintln!("[vitesse] cannot start a runtime for {name}: {e}"),
                    }
                });
            if let Err(e) = spawned {
                eprintln!("[vitesse] cannot start a thread for {name}: {e}");
            }
        }
    }
}

/// What a background task returns: nothing, or a `Result` whose error is
/// logged (and retried, in a [`Queue`](crate::queue::Queue)).
pub trait TaskOutcome: Send + 'static {
    /// The error message, if the task failed.
    fn failure(self) -> Option<String>;
}

impl TaskOutcome for () {
    fn failure(self) -> Option<String> {
        None
    }
}

impl<E: std::fmt::Display + Send + 'static> TaskOutcome for Result<(), E> {
    fn failure(self) -> Option<String> {
        self.err().map(|e| e.to_string())
    }
}

/// Runs a task, turning a panic into a failure.
pub(crate) async fn run_task(task: crate::handler::BoxFuture<Option<String>>) -> Option<String> {
    match tokio::spawn(task).await {
        Ok(outcome) => outcome,
        Err(e) if e.is_panic() => Some("the task panicked".to_owned()),
        Err(e) => Some(e.to_string()),
    }
}
