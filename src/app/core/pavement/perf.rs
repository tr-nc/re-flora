//! Opt-in Release measurements through the production drag/release path.
use super::ActivePavement;
use std::{sync::OnceLock, time::Instant};

pub(super) struct Step {
    started: Instant,
    pieces_before: usize,
    phase: &'static str,
}

impl Step {
    pub(super) fn begin(active: &ActivePavement, phase: &'static str) -> Option<Self> {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        ENABLED
            .get_or_init(|| std::env::var_os("RE_FLORA_PAVEMENT_PERF_REPORT").is_some())
            .then(|| Self {
                started: Instant::now(),
                pieces_before: active.pieces_placed,
                phase,
            })
    }

    pub(super) fn finish(self, active: &ActivePavement) {
        // Count retained receipt allocation after stopping the work timer. This is
        // voxel receipt capacity, not process RSS, flora payload or HashSet storage.
        let elapsed_us = self.started.elapsed().as_secs_f64() * 1_000_000.;
        log::info!(
            "[PERF][PAVEMENT_STEP] phase={} kind={} pieces={} elapsed_us={elapsed_us:.3} courses={} owned_voxels={} voxel_receipt_bytes={}",
            self.phase,
            active.recipe.name(),
            active.pieces_placed.saturating_sub(self.pieces_before),
            active.surfaces_cleared,
            active.edits.owned_voxels().len(),
            active.edits.voxel_receipt_bytes(),
        );
    }
}
