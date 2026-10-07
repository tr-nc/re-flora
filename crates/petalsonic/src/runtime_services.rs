//! Typed ownership and ordered shutdown for runtime-level audio services.

use crate::error::{PetalSonicError, Result};
use crate::runtime_health::RuntimeFailurePublisher;
use crossbeam_channel::Sender;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuntimeChildKind {
    Acoustics,
    Output,
}

impl fmt::Display for RuntimeChildKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Acoustics => formatter.write_str("acoustics"),
            Self::Output => formatter.write_str("output"),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ChildCancellation {
    requested: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl ChildCancellation {
    pub(crate) fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            requested: Arc::new(AtomicBool::new(false)),
            wake: Arc::new(wake),
        }
    }

    pub(crate) fn passive() -> Self {
        Self::new(|| {})
    }

    fn request(&self) {
        self.requested.store(true, Ordering::Release);
        (self.wake)();
    }

    pub(crate) fn is_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeChildFailure {
    detail: String,
}

impl RuntimeChildFailure {
    pub(crate) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for RuntimeChildFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

pub(crate) type RuntimeChildResult = std::result::Result<(), RuntimeChildFailure>;

enum StartupStatus {
    Ready,
    Failed(PetalSonicError),
}

pub(crate) struct ChildStartup {
    sender: Option<Sender<StartupStatus>>,
}

impl ChildStartup {
    pub(crate) fn ready(mut self) -> RuntimeChildResult {
        self.sender
            .take()
            .expect("startup acknowledgement is consumed exactly once")
            .send(StartupStatus::Ready)
            .map_err(|_| RuntimeChildFailure::new("runtime owner dropped during startup"))
    }

    pub(crate) fn failed(mut self, error: PetalSonicError) -> RuntimeChildFailure {
        let detail = error.to_string();
        let _ = self
            .sender
            .take()
            .expect("startup acknowledgement is consumed exactly once")
            .send(StartupStatus::Failed(error));
        RuntimeChildFailure::new(detail)
    }
}

#[derive(Debug)]
enum ChildCompletion {
    Stopped,
    Failed(RuntimeChildFailure),
}

struct RuntimeChild {
    cancellation: ChildCancellation,
    handle: JoinHandle<ChildCompletion>,
}

struct AcousticsService(RuntimeChild);

struct OutputService(RuntimeChild);

#[derive(Default)]
struct RunningServices {
    acoustics: Option<AcousticsService>,
    output: Option<OutputService>,
}

/// Owns the producer-to-consumer topology of one audio runtime.
///
/// Startup acknowledgement, cancellation, abnormal-exit classification, joining, panic
/// conversion, and the aggregate close result stay behind this interface. Acoustics produces
/// responses consumed by the output-owned render quantum, so shutdown always quiesces and joins
/// acoustics before it allows output to release that consumer. Session-scoped render work shares
/// only [`RuntimeFailurePublisher`], never a task handle.
pub(crate) struct RuntimeServices {
    failure_publisher: RuntimeFailurePublisher,
    running: Mutex<RunningServices>,
}

impl RuntimeServices {
    pub(crate) fn new(failure_publisher: RuntimeFailurePublisher) -> Self {
        Self {
            failure_publisher,
            running: Mutex::new(RunningServices::default()),
        }
    }

    pub(crate) fn start_acoustics(
        &mut self,
        thread_name: &'static str,
        cancellation: ChildCancellation,
        run: impl FnOnce(ChildStartup, ChildCancellation) -> RuntimeChildResult + Send + 'static,
    ) -> Result<()> {
        let running = self
            .running
            .get_mut()
            .map_err(|_| PetalSonicError::Engine("Runtime services lock is poisoned".into()))?;
        if running.acoustics.is_some() {
            return Err(PetalSonicError::Engine(
                "Acoustics service is already running".into(),
            ));
        }
        let child = Self::start_service(
            self.failure_publisher.clone(),
            RuntimeChildKind::Acoustics,
            thread_name,
            cancellation,
            run,
        )?;
        running.acoustics = Some(AcousticsService(child));
        Ok(())
    }

    pub(crate) fn start_output(
        &mut self,
        thread_name: &'static str,
        cancellation: ChildCancellation,
        run: impl FnOnce(ChildStartup, ChildCancellation) -> RuntimeChildResult + Send + 'static,
    ) -> Result<()> {
        let running = self
            .running
            .get_mut()
            .map_err(|_| PetalSonicError::Engine("Runtime services lock is poisoned".into()))?;
        if running.acoustics.is_none() {
            return Err(PetalSonicError::Engine(
                "Acoustics service must be running before output starts".into(),
            ));
        }
        if running.output.is_some() {
            return Err(PetalSonicError::Engine(
                "Output service is already running".into(),
            ));
        }
        let child = Self::start_service(
            self.failure_publisher.clone(),
            RuntimeChildKind::Output,
            thread_name,
            cancellation,
            run,
        )?;
        running.output = Some(OutputService(child));
        Ok(())
    }

    fn start_service(
        failure_publisher: RuntimeFailurePublisher,
        kind: RuntimeChildKind,
        thread_name: &'static str,
        cancellation: ChildCancellation,
        run: impl FnOnce(ChildStartup, ChildCancellation) -> RuntimeChildResult + Send + 'static,
    ) -> Result<RuntimeChild> {
        let (startup_sender, startup_receiver) = crossbeam_channel::bounded(1);
        let task_cancellation = cancellation.clone();
        let handle = std::thread::Builder::new()
            .name(thread_name.into())
            .spawn(move || {
                let run_result = catch_unwind(AssertUnwindSafe(|| {
                    run(
                        ChildStartup {
                            sender: Some(startup_sender),
                        },
                        task_cancellation.clone(),
                    )
                }));
                let completion = match run_result {
                    Ok(Ok(())) if task_cancellation.is_requested() => ChildCompletion::Stopped,
                    Ok(Ok(())) => ChildCompletion::Failed(RuntimeChildFailure::new(
                        "exited without runtime cancellation",
                    )),
                    Ok(Err(failure)) => ChildCompletion::Failed(failure),
                    Err(_) => ChildCompletion::Failed(RuntimeChildFailure::new("panicked")),
                };
                if matches!(completion, ChildCompletion::Failed(_)) {
                    failure_publisher.publish();
                }
                completion
            })
            .map_err(|error| {
                PetalSonicError::Engine(format!("Failed to start {kind} child: {error}"))
            })?;

        match startup_receiver.recv() {
            Ok(StartupStatus::Ready) => Ok(RuntimeChild {
                cancellation,
                handle,
            }),
            Ok(StartupStatus::Failed(error)) => {
                let _ = handle.join();
                Err(error)
            }
            Err(_) => {
                let detail = match handle.join() {
                    Ok(ChildCompletion::Failed(failure)) => failure.to_string(),
                    Ok(ChildCompletion::Stopped) => "stopped during startup".into(),
                    Err(_) => "panicked during startup".into(),
                };
                Err(PetalSonicError::Engine(format!(
                    "{kind} child exited during startup: {detail}"
                )))
            }
        }
    }

    pub(crate) fn close(&self) -> Result<()> {
        let mut running = self
            .running
            .lock()
            .map_err(|_| PetalSonicError::Engine("Runtime services lock is poisoned".into()))?;

        let mut failures = Vec::new();
        if let Some(AcousticsService(acoustics)) = running.acoustics.take() {
            Self::stop_and_join(RuntimeChildKind::Acoustics, acoustics, &mut failures);
        }
        if let Some(OutputService(output)) = running.output.take() {
            Self::stop_and_join(RuntimeChildKind::Output, output, &mut failures);
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(PetalSonicError::Engine(format!(
                "Runtime child shutdown failed: {}",
                failures.join("; ")
            )))
        }
    }

    fn stop_and_join(kind: RuntimeChildKind, child: RuntimeChild, failures: &mut Vec<String>) {
        child.cancellation.request();
        child.handle.thread().unpark();
        match child.handle.join() {
            Ok(ChildCompletion::Stopped) => {}
            Ok(ChildCompletion::Failed(failure)) => {
                failures.push(format!("{kind}: {failure}"));
            }
            Err(_) => failures.push(format!("{kind}: join observed a panic")),
        }
    }
}

impl Drop for RuntimeServices {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
