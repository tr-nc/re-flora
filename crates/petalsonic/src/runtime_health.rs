use crate::events::RuntimeState;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// The single publication seam for terminal failures from independently owned runtime work.
///
/// Long-lived services and session-scoped work share this publisher without sharing task handles.
/// A failure can replace Running or Recovering, but cannot resurrect or rewrite a runtime whose
/// owner has already started closing it.
#[derive(Clone)]
pub(crate) struct RuntimeFailurePublisher {
    state: Arc<AtomicU8>,
}

impl RuntimeFailurePublisher {
    pub(crate) fn new(state: Arc<AtomicU8>) -> Self {
        Self { state }
    }

    pub(crate) fn publish(&self) -> bool {
        let mut observed = self.state.load(Ordering::Acquire);
        loop {
            match RuntimeState::from_storage(observed) {
                RuntimeState::Running | RuntimeState::Recovering => {}
                RuntimeState::Failed | RuntimeState::Closing | RuntimeState::Closed => {
                    return false;
                }
            }
            match self.state.compare_exchange_weak(
                observed,
                RuntimeState::Failed as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(current) => observed = current,
            }
        }
    }
}
