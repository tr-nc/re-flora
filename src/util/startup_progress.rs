//! Startup-only progress, including synchronous calls that block the window thread.
//! A heartbeat means we are still waiting in a stage, not proof of driver progress.
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);

type Reporter = Arc<dyn Fn(Report) + Send + Sync>;

#[derive(Debug)]
struct Report {
    status: &'static str,
    stage: &'static str,
    stage_time: Duration,
    total_time: Duration,
}

struct State {
    started: Instant,
    stage_started: Instant,
    stage: &'static str,
    finished: bool,
}

impl State {
    fn report(&self, status: &'static str, reporter: &Reporter) {
        let now = Instant::now();
        reporter(Report {
            status,
            stage: self.stage,
            stage_time: now.duration_since(self.stage_started),
            total_time: now.duration_since(self.started),
        });
    }
}

/// Owned by App until the first game frame completes. Observes startup only;
/// the reporter never touches Vulkan, the window, or simulation state.
/// Dropping before `finish` reports an interrupted startup, never success.
pub(crate) struct StartupProgress {
    shared: Arc<(Mutex<State>, Condvar)>,
    reporter: Reporter,
    worker: Option<JoinHandle<()>>,
}

impl StartupProgress {
    pub(crate) fn new(stage: &'static str) -> Self {
        Self::with_reporter(
            stage,
            HEARTBEAT_INTERVAL,
            Arc::new(|report| {
                log::info!(
                    target: crate::RUN_SUMMARY_TARGET,
                    "[STARTUP] status={} stage={} stage_ms={:.1} total_ms={:.1}",
                    report.status,
                    report.stage,
                    report.stage_time.as_secs_f64() * 1000.0,
                    report.total_time.as_secs_f64() * 1000.0,
                );
            }),
        )
    }

    fn with_reporter(stage: &'static str, interval: Duration, reporter: Reporter) -> Self {
        let now = Instant::now();
        let state = State {
            started: now,
            stage_started: now,
            stage,
            finished: false,
        };
        state.report("begin", &reporter);
        let shared = Arc::new((Mutex::new(state), Condvar::new()));
        let worker_shared = shared.clone();
        let worker_reporter = reporter.clone();
        let worker = thread::Builder::new()
            .name("startup-progress".into())
            .spawn(move || {
                let (state, changed) = &*worker_shared;
                let mut state = state.lock().unwrap_or_else(|err| err.into_inner());
                while !state.finished {
                    let (next, timeout) = changed
                        .wait_timeout(state, interval)
                        .unwrap_or_else(|err| err.into_inner());
                    state = next;
                    if timeout.timed_out() && !state.finished {
                        state.report("waiting", &worker_reporter);
                    }
                }
            })
            .map_err(|err| log::warn!("Startup heartbeat unavailable: {err}"))
            .ok();
        Self {
            shared,
            reporter,
            worker,
        }
    }

    /// Repeated observations of the same stage do not reset its timer or spam logs.
    pub(crate) fn stage(&self, stage: &'static str) {
        let (state, changed) = &*self.shared;
        let mut state = state.lock().unwrap_or_else(|err| err.into_inner());
        if state.finished || state.stage == stage {
            return;
        }
        state.report("done", &self.reporter);
        state.stage = stage;
        state.stage_started = Instant::now();
        state.report("begin", &self.reporter);
        changed.notify_one();
    }

    pub(crate) fn finish(mut self) {
        self.stop(true);
    }

    fn stop(&mut self, ready: bool) {
        let (state, changed) = &*self.shared;
        {
            let mut state = state.lock().unwrap_or_else(|err| err.into_inner());
            if state.finished {
                return;
            }
            state.finished = true;
            if ready {
                state.report("done", &self.reporter);
            }
            // Reports are serialized with the worker so no heartbeat can appear
            // after ready/aborted. Wake it immediately instead of waiting 2s.
            state.report(if ready { "ready" } else { "aborted" }, &self.reporter);
            changed.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                log::warn!("Startup heartbeat thread panicked");
            }
        }
    }
}

impl Drop for StartupProgress {
    fn drop(&mut self) {
        self.stop(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn observed(interval: Duration) -> (StartupProgress, mpsc::Receiver<Report>) {
        let (tx, rx) = mpsc::channel();
        let progress = StartupProgress::with_reporter(
            "window_vulkan",
            interval,
            Arc::new(move |report| {
                let _ = tx.send(report);
            }),
        );
        (progress, rx)
    }

    #[test]
    fn stages_report_real_boundaries_and_finish_without_waiting_for_heartbeat() {
        let (progress, rx) = observed(Duration::from_secs(60));
        progress.stage("compute_pipelines");
        progress.stage("compute_pipelines");
        progress.stage("first_frame");
        progress.finish();
        let reports: Vec<_> = rx.into_iter().collect();
        let events: Vec<_> = reports.iter().map(|r| (r.status, r.stage)).collect();
        assert_eq!(
            events,
            [
                ("begin", "window_vulkan"),
                ("done", "window_vulkan"),
                ("begin", "compute_pipelines"),
                ("done", "compute_pipelines"),
                ("begin", "first_frame"),
                ("done", "first_frame"),
                ("ready", "first_frame"),
            ]
        );
        assert!(reports
            .windows(2)
            .all(|pair| pair[0].total_time <= pair[1].total_time));
        assert!(reports.iter().all(|r| r.stage_time <= r.total_time));
    }

    #[test]
    fn heartbeat_reports_while_startup_thread_is_blocked_and_retires_on_finish() {
        let (progress, rx) = observed(Duration::from_millis(1));
        assert_eq!(rx.recv().unwrap().status, "begin");
        let heartbeat = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            (heartbeat.status, heartbeat.stage),
            ("waiting", "window_vulkan")
        );
        progress.stage("first_frame");
        progress.finish();
        let remaining: Vec<_> = rx.into_iter().collect();
        assert_eq!(remaining.last().unwrap().status, "ready");
        let first_frame = remaining.iter().position(|r| r.status == "begin").unwrap();
        assert!(remaining[first_frame..]
            .iter()
            .all(|r| r.stage == "first_frame"));
    }

    #[test]
    fn early_error_reports_abort_not_success_and_retires_worker() {
        let (progress, rx) = observed(Duration::from_secs(60));
        let startup = |progress: StartupProgress| -> Result<(), &'static str> {
            progress.stage("shader_modules");
            Err("shader unavailable")?;
            progress.finish();
            Ok(())
        };
        assert_eq!(startup(progress), Err("shader unavailable"));
        let reports: Vec<_> = rx.into_iter().collect();
        assert_eq!(reports.last().unwrap().status, "aborted");
        assert_eq!(reports.last().unwrap().stage, "shader_modules");
        assert!(!reports.iter().any(|r| r.status == "ready"));
    }
}
