//! Bounded shared-cache correctness scene. Called only by the explicit apple
//! review plus cache/preview diagnostic flags. No settings or saves are written.
use super::App;

impl App {
    pub(super) fn prepare_shared_model_cache_review(&mut self, frame: u32) {
        let phase = (frame / 24).min(12);
        // Exercise each bank at the production 512-view count without
        // allocating multi-GiB 64px banks in the correctness fixture.
        let resolutions = match phase {
            0 | 1 | 11 | 12 => [8; 4],
            2 => [16, 8, 8, 8],
            3 => [16, 16, 8, 8],
            4 => [16, 16, 16, 8],
            5 => [16; 4],
            6..=10 => [8; 4],
            _ => unreachable!(),
        };
        let views = crate::tracer::MODEL_PIXEL_VIEW_COUNT;
        let s = &mut self.debug_settings.adjustables;
        s.model_flower_head_scale.value = 1.;
        s.model_flower_height_scale.value = 1.;
        s.falling_leaf_pixel_resolution.value = resolutions[0];
        s.apple_pixel_resolution.value = resolutions[1];
        s.butterfly_pixel_resolution.value = resolutions[2];
        s.model_flower_pixel_resolution.value = resolutions[3];
        s.butterfly_mesh_preview.value = true;
        s.falling_leaf_mesh.value = true;
        s.falling_leaf_size_scale.value = 1.;
        s.fruit_cycle.value = if phase >= 6 { 1. } else { 0.7 };
        s.model_flower_size_scale.value = if phase == 1 { 2. } else { 1. };
        s.flora_growth_override_enabled.value = true;
        s.flora_growth_override.value = if phase == 1 { 0.55 } else { 1. };
        s.auto_daynight_cycle.value = false;
        if frame.is_multiple_of(24) && frame <= 12 * 24 {
            self.set_manual_time_of_day(if phase == 1 { 0.2 } else { 0.45 });
            log::info!("[MODEL_CACHE_PHASE] phase={phase} views={views} resolutions={}/{}/{}/{} heads_only={} dropped={} saved=false",resolutions[0],resolutions[1],resolutions[2],resolutions[3],true,phase>=6);
            if phase == 12 {
                if let Some(resize) = &mut self.resize_lifecycle_test {
                    resize.requested = 0;
                    resize.next_request_frame = 0;
                    resize.complete = false;
                    log::info!("[MODEL_CACHE_RESIZE] ready_banks=true replay_after_consumers=true");
                }
            }
        }
    }
}
