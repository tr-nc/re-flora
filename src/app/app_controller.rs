use super::core::launch_owners::prepare_startup_owners;
use super::core::App;
use crate::RunPlan;
use winit::{
    application::ApplicationHandler, event::WindowEvent, event_loop::ActiveEventLoop,
    window::WindowId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResumeOutcome {
    Started,
    AlreadyRunning,
}

enum LifecycleSlot<P, R> {
    Pending(P),
    Running(R),
}

impl<P, R> LifecycleSlot<P, R> {
    fn resume<E>(&mut self, factory: impl FnOnce(&P) -> Result<R, E>) -> Result<ResumeOutcome, E> {
        let candidate = match self {
            Self::Pending(plan) => factory(plan)?,
            Self::Running(_) => return Ok(ResumeOutcome::AlreadyRunning),
        };
        *self = Self::Running(candidate);
        Ok(ResumeOutcome::Started)
    }

    fn running_mut(&mut self) -> &mut R {
        match self {
            Self::Pending(_) => panic!("App is not initialized"),
            Self::Running(runtime) => runtime,
        }
    }
}

pub struct AppController(LifecycleSlot<RunPlan, App>);

impl AppController {
    pub fn new(plan: RunPlan) -> Self {
        Self(LifecycleSlot::Pending(plan))
    }
}

impl ApplicationHandler for AppController {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.0
            .resume(|plan| {
                let RunPlan {
                    platform,
                    audio,
                    world,
                    automation,
                    scenario,
                } = plan;
                let owners = prepare_startup_owners(automation.clone(), scenario.clone())
                    .map_err(|error| anyhow::anyhow!("invalid launch ownership: {error}"))?;
                App::new(
                    event_loop,
                    platform.clone(),
                    world.clone(),
                    audio.clone(),
                    owners,
                )
            })
            .unwrap_or_else(|error| panic!("failed to initialize App: {error:#}"));
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Keep the running App intact; a later resume is intentionally idempotent.
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.0.running_mut().on_window_event(event_loop, id, event);
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        self.0
            .running_mut()
            .on_device_event(event_loop, device_id, event);
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.0.running_mut().on_about_to_wait(_event_loop);
    }
}

#[cfg(test)]
mod tests {
    use super::{LifecycleSlot, ResumeOutcome};
    use std::{
        cell::Cell,
        panic::{catch_unwind, AssertUnwindSafe},
        rc::Rc,
    };

    struct NonClonePlan {
        token: String,
    }

    #[test]
    fn failed_resume_preserves_the_non_clone_plan_for_retry() {
        let attempts = Cell::new(0);
        let mut slot = LifecycleSlot::<NonClonePlan, String>::Pending(NonClonePlan {
            token: "launch-once".to_owned(),
        });

        let failure = slot.resume(|plan| {
            attempts.set(attempts.get() + 1);
            assert_eq!(plan.token, "launch-once");
            Err("not ready")
        });
        assert_eq!(failure, Err("not ready"));
        match &slot {
            LifecycleSlot::Pending(plan) => assert_eq!(plan.token, "launch-once"),
            LifecycleSlot::Running(_) => panic!("failed construction must not commit a runtime"),
        }

        assert!(catch_unwind(AssertUnwindSafe(|| slot.resume(
            |_| -> Result<String, &str> {
                attempts.set(attempts.get() + 1);
                panic!("factory panic")
            }
        )))
        .is_err());
        match &slot {
            LifecycleSlot::Pending(plan) => assert_eq!(plan.token, "launch-once"),
            LifecycleSlot::Running(_) => panic!("panicked construction must not commit a runtime"),
        }

        let outcome = slot
            .resume(|plan| {
                attempts.set(attempts.get() + 1);
                Ok::<_, &str>(plan.token.clone())
            })
            .unwrap();
        assert_eq!(outcome, ResumeOutcome::Started);
        assert_eq!(attempts.get(), 3);
        match &slot {
            LifecycleSlot::Pending(_) => panic!("successful retry must commit the runtime"),
            LifecycleSlot::Running(runtime) => assert_eq!(runtime, "launch-once"),
        }
    }

    #[derive(Default)]
    struct LiveCounts {
        live: Cell<usize>,
        max_live: Cell<usize>,
        drops: Cell<usize>,
    }

    struct DropProbe {
        id: usize,
        counts: Rc<LiveCounts>,
    }

    impl DropProbe {
        fn new(id: usize, counts: Rc<LiveCounts>) -> Self {
            let live = counts.live.get() + 1;
            counts.live.set(live);
            counts.max_live.set(counts.max_live.get().max(live));
            Self { id, counts }
        }
    }

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.counts.live.set(self.counts.live.get() - 1);
            self.counts.drops.set(self.counts.drops.get() + 1);
        }
    }

    #[test]
    fn redundant_resume_neither_calls_factory_nor_replaces_the_running_value() {
        let calls = Cell::new(0);
        let counts = Rc::new(LiveCounts::default());
        let mut slot = LifecycleSlot::Pending(());

        let first = slot
            .resume(|_| {
                calls.set(calls.get() + 1);
                Ok::<_, ()>(DropProbe::new(1, Rc::clone(&counts)))
            })
            .unwrap();
        let second = slot
            .resume(|_| {
                calls.set(calls.get() + 1);
                Ok::<_, ()>(DropProbe::new(2, Rc::clone(&counts)))
            })
            .unwrap();
        assert_eq!(first, ResumeOutcome::Started);
        assert_eq!(second, ResumeOutcome::AlreadyRunning);
        assert_eq!(calls.get(), 1);
        assert_eq!(counts.live.get(), 1);
        assert_eq!(counts.max_live.get(), 1);
        assert_eq!(counts.drops.get(), 0);
        assert_eq!(slot.running_mut().id, 1);
        drop(slot);
        assert_eq!(counts.live.get(), 0);
        assert_eq!(counts.drops.get(), 1);
    }

    struct EventProbe {
        dispatched: usize,
    }

    fn dispatch_one_event(slot: &mut LifecycleSlot<(), EventProbe>) {
        slot.running_mut().dispatched += 1;
    }

    #[test]
    fn event_dispatch_is_rejected_before_success_and_delivered_once_afterward() {
        let mut slot = LifecycleSlot::Pending(());
        assert!(catch_unwind(AssertUnwindSafe(|| dispatch_one_event(&mut slot))).is_err());

        assert_eq!(
            slot.resume(|_| Ok::<_, ()>(EventProbe { dispatched: 0 }))
                .unwrap(),
            ResumeOutcome::Started,
        );
        dispatch_one_event(&mut slot);
        assert_eq!(slot.running_mut().dispatched, 1);
    }
}
