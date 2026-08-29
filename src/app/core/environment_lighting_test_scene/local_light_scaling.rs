use super::*;

// Measure zero last so it is a real 8 -> 0 transport update. A 0 -> 0 metadata-only provider
// publication intentionally does not perturb the immutable DDGI radiance payload.
const LOCAL_LIGHT_SCALING_COUNTS: [usize; 5] = [1, 2, 4, 8, 0];
const LOCAL_LIGHT_SCALING_WARMUP_FRAMES: u16 = 12;
const LOCAL_LIGHT_SCALING_SAMPLE_FRAMES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalLightScalingStage {
    AwaitBaseline,
    AwaitLive,
    AwaitDdgiBuild,
    Warmup,
    Sampling,
    AwaitFinalLive,
    AwaitFinalPublication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LocalLightScalingState {
    pub(super) terrain_revision: u32,
    stage: LocalLightScalingStage,
    count_index: usize,
    expected_source_revision: u64,
    expected_registry_revision: u64,
    mutation_frame: u64,
    warmup_frames: u16,
}

impl LocalLightScalingState {
    pub(super) fn new(terrain_revision: u32) -> Self {
        Self {
            terrain_revision,
            stage: LocalLightScalingStage::AwaitBaseline,
            count_index: 0,
            expected_source_revision: 0,
            expected_registry_revision: 0,
            mutation_frame: 0,
            warmup_frames: 0,
        }
    }

    pub(super) const fn phase_label(self) -> &'static str {
        match self.stage {
            LocalLightScalingStage::AwaitBaseline => "waiting-for-local-light-scale-baseline",
            LocalLightScalingStage::AwaitLive => "waiting-for-local-light-scale-live",
            LocalLightScalingStage::AwaitDdgiBuild => "waiting-for-local-light-scale-ddgi-build",
            LocalLightScalingStage::Warmup => "warming-local-light-scale-sample",
            LocalLightScalingStage::Sampling => "sampling-local-light-scale",
            LocalLightScalingStage::AwaitFinalLive => "waiting-for-local-light-scale-final-live",
            LocalLightScalingStage::AwaitFinalPublication => {
                "waiting-for-local-light-scale-final-publication"
            }
        }
    }

    fn requested_count(self) -> usize {
        LOCAL_LIGHT_SCALING_COUNTS[self.count_index]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LocalLightScalingSample {
    frame_cpu_us: f64,
    frame_gpu_us: f64,
    terrain_path_gpu_us: f64,
    raster_flora_cache_gpu_us: f64,
    ddgi_trace_gpu_us: f64,
    ddgi_filter_gpu_us: f64,
    render_trace_record_cpu_us: f64,
}

fn scope_duration_us(results: &re_flora_vkn::GpuProfilerFrameResults, name: &str) -> Option<f64> {
    results
        .scopes
        .iter()
        .find(|scope| scope.name == name)
        .map(|scope| scope.duration_us())
}

fn sample_from_app(app: &App) -> Option<LocalLightScalingSample> {
    let results = app.gpu_profiler_latest_results.as_ref()?;
    if results.dropped_scope_count != 0 {
        return None;
    }
    Some(LocalLightScalingSample {
        frame_cpu_us: f64::from(app.frame_timing_snapshot.total_ms) * 1_000.0,
        frame_gpu_us: scope_duration_us(results, "frame.render")?,
        terrain_path_gpu_us: scope_duration_us(results, "tracer.pass")?,
        raster_flora_cache_gpu_us: scope_duration_us(results, "graphics.flora_lighting_cache")?,
        ddgi_trace_gpu_us: scope_duration_us(results, "ddgi.probe_trace")?,
        ddgi_filter_gpu_us: scope_duration_us(results, "ddgi.irradiance_filter")?,
        render_trace_record_cpu_us: f64::from(app.frame_timing_snapshot.render_trace_record_ms)
            * 1_000.0,
    })
}

fn percentile(mut values: Vec<f64>, quantile: f64) -> f64 {
    assert!(!values.is_empty());
    assert!((0.0..=1.0).contains(&quantile));
    values.sort_by(f64::total_cmp);
    let index = ((values.len() - 1) as f64 * quantile).round() as usize;
    values[index]
}

fn sample_percentile(
    samples: &[LocalLightScalingSample],
    quantile: f64,
    project: impl Fn(LocalLightScalingSample) -> f64,
) -> f64 {
    percentile(samples.iter().copied().map(project).collect(), quantile)
}

fn add_scaling_light(app: &mut App, index: usize) -> LightId {
    let ring = [
        Vec3::ZERO,
        Vec3::new(0.018, 0.0, 0.0),
        Vec3::new(-0.018, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.018),
        Vec3::new(0.0, 0.0, -0.018),
        Vec3::new(0.012, 0.0, 0.012),
        Vec3::new(-0.012, 0.0, 0.012),
        Vec3::new(0.012, 0.0, -0.012),
    ];
    app.local_lights.add(LocalLight::Point(
        PointLight::new(
            POINT_LIGHT_ADD_POSITION + ring[index],
            Vec3::new(1.0, 0.45, 0.2),
            0.035,
            POINT_LIGHT_SOURCE_RADIUS_WORLD,
            POINT_LIGHT_RANGE_WORLD,
        )
        .expect("local-light scaling point must be valid"),
    ))
}

fn publish_scaling_count(app: &mut App, requested_count: usize) -> (u64, u64) {
    assert!(requested_count <= LOCAL_LIGHT_GPU_CAPACITY);
    if requested_count == 0 {
        let ids = std::mem::take(
            &mut app
                .environment_lighting_test_scene
                .as_mut()
                .unwrap()
                .local_light_scaling_ids,
        );
        for id in ids {
            app.local_lights
                .remove(id)
                .expect("zero-count scaling transition must remove every selected light");
        }
    } else {
        let first_new = app
            .environment_lighting_test_scene
            .as_ref()
            .unwrap()
            .local_light_scaling_ids
            .len();
        assert!(first_new <= requested_count);
        let new_ids = (first_new..requested_count)
            .map(|index| add_scaling_light(app, index))
            .collect::<Vec<_>>();
        app.environment_lighting_test_scene
            .as_mut()
            .unwrap()
            .local_light_scaling_ids
            .extend(new_ids);
    }
    let snapshot = app.local_lights.snapshot();
    assert_eq!(snapshot.lights().len(), requested_count);
    (snapshot.source_revision(), snapshot.registry_revision())
}

fn builder_matches(app: &App, state: LocalLightScalingState) -> bool {
    let Some(snapshot) = app.tracer.ddgi_builder_radiance_snapshot() else {
        return false;
    };
    let status = app.tracer.ddgi_runtime_status();
    snapshot.local_lights.source_revision() == state.expected_source_revision
        && snapshot.local_lights.count() == state.requested_count() as u32
        && status.active().building_field.is_some()
        && !app
            .tracer
            .ddgi_lighting_diagnostics()
            .has_mixed_in_flight_revision
}

impl App {
    pub(super) fn advance_local_light_scaling(
        &mut self,
        mut state: LocalLightScalingState,
    ) -> Option<TestScenePhase> {
        let next = match state.stage {
            LocalLightScalingStage::AwaitBaseline => {
                assert!(
                    self.perf_logging && self.gpu_profiler.is_some(),
                    "local-light-scaling requires --perf release GPU timestamps"
                );
                let status = self.tracer.ddgi_runtime_status();
                let active = status.active();
                let baseline = active.published_field?;
                if !is_converged_field(baseline)
                    || active.stage != DdgiVolumeStage::Ready
                    || active.building_field.is_some()
                    || status.staging().is_some()
                {
                    return None;
                }
                assert_eq!(baseline.field().geometry_revision(), state.terrain_revision);
                assert!(self.local_lights.snapshot().lights().is_empty());
                let (source_revision, registry_revision) =
                    publish_scaling_count(self, state.requested_count());
                state.expected_source_revision = source_revision;
                state.expected_registry_revision = registry_revision;
                state.mutation_frame = self.time_info.total_frame_count();
                state.stage = LocalLightScalingStage::AwaitLive;
                log::info!(
                    "[LOCAL_LIGHT_SCALE] begin counts=0,1,2,4,8 measurement_order=1,2,4,8,0 zero_is_real_8_to_0_transport=true warmup_frames={} sample_frames={} fixed_camera=true release_required=true gpu_capacity={} live_resource_bytes={} ddgi_transport_resource_bytes={}",
                    LOCAL_LIGHT_SCALING_WARMUP_FRAMES,
                    LOCAL_LIGHT_SCALING_SAMPLE_FRAMES,
                    LOCAL_LIGHT_GPU_CAPACITY,
                    std::mem::size_of::<crate::generated::gpu_structs::LocalLightInfo>()
                        + LOCAL_LIGHT_GPU_CAPACITY
                            * std::mem::size_of::<crate::generated::gpu_structs::LightGpu>(),
                    std::mem::size_of::<crate::generated::gpu_structs::LocalLightInfo>()
                        + LOCAL_LIGHT_GPU_CAPACITY
                            * std::mem::size_of::<crate::generated::gpu_structs::LightGpu>(),
                );
                state
            }
            LocalLightScalingStage::AwaitLive => {
                if self.tracer.local_light_live_observation().state()
                    != (
                        Some(state.expected_source_revision),
                        state.requested_count() as u32,
                    )
                    || self.time_info.total_frame_count() <= state.mutation_frame
                {
                    return None;
                }
                assert_eq!(
                    self.tracer.local_light_live_observation().registry_revision,
                    Some(state.expected_registry_revision)
                );
                assert!(self
                    .tracer
                    .local_light_live_observation()
                    .overflow
                    .is_empty());
                state.stage = LocalLightScalingStage::AwaitDdgiBuild;
                state
            }
            LocalLightScalingStage::AwaitDdgiBuild => {
                if !builder_matches(self, state) {
                    return None;
                }
                self.environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .local_light_scaling_samples
                    .clear();
                state.warmup_frames = 0;
                state.stage = LocalLightScalingStage::Warmup;
                state
            }
            LocalLightScalingStage::Warmup => {
                if !builder_matches(self, state) || sample_from_app(self).is_none() {
                    return None;
                }
                state.warmup_frames += 1;
                if state.warmup_frames >= LOCAL_LIGHT_SCALING_WARMUP_FRAMES {
                    state.stage = LocalLightScalingStage::Sampling;
                }
                state
            }
            LocalLightScalingStage::Sampling => {
                if !builder_matches(self, state) {
                    return None;
                }
                let sample = sample_from_app(self)?;
                let samples = &mut self
                    .environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .local_light_scaling_samples;
                samples.push(sample);
                if samples.len() < LOCAL_LIGHT_SCALING_SAMPLE_FRAMES {
                    return Some(TestScenePhase::LocalLightScaling(state));
                }
                let count = state.requested_count();
                let ddgi = self.tracer.ddgi_local_light_gpu_evidence();
                let (ddgi_candidates, ddgi_visible, ddgi_occluded) = ddgi
                    .filter(|evidence| {
                        evidence.local_source_revision == state.expected_source_revision
                            && evidence.local_light_count == count as u32
                    })
                    .map_or((0, 0, 0), |evidence| {
                        (
                            evidence.totals.candidates,
                            evidence.totals.visible,
                            evidence.totals.occluded,
                        )
                    });
                log::info!(
                    "[LOCAL_LIGHT_SCALE] count={} samples={} source_revision={} registry_revision={} accepted={} overflow=0 frame_cpu_p50_us={:.1} frame_cpu_p95_us={:.1} frame_gpu_p50_us={:.1} frame_gpu_p95_us={:.1} terrain_path_gpu_p50_us={:.1} terrain_path_gpu_p95_us={:.1} raster_flora_cache_gpu_p50_us={:.1} raster_flora_cache_gpu_p95_us={:.1} ddgi_trace_gpu_p50_us={:.1} ddgi_trace_gpu_p95_us={:.1} ddgi_filter_gpu_p50_us={:.1} ddgi_filter_gpu_p95_us={:.1} render_trace_record_cpu_p50_us={:.1} render_trace_record_cpu_p95_us={:.1} ddgi_candidates={} ddgi_visible={} ddgi_occluded={} mixed_in_flight=false",
                    count,
                    samples.len(),
                    state.expected_source_revision,
                    state.expected_registry_revision,
                    count,
                    sample_percentile(samples, 0.50, |sample| sample.frame_cpu_us),
                    sample_percentile(samples, 0.95, |sample| sample.frame_cpu_us),
                    sample_percentile(samples, 0.50, |sample| sample.frame_gpu_us),
                    sample_percentile(samples, 0.95, |sample| sample.frame_gpu_us),
                    sample_percentile(samples, 0.50, |sample| sample.terrain_path_gpu_us),
                    sample_percentile(samples, 0.95, |sample| sample.terrain_path_gpu_us),
                    sample_percentile(samples, 0.50, |sample| sample.raster_flora_cache_gpu_us),
                    sample_percentile(samples, 0.95, |sample| sample.raster_flora_cache_gpu_us),
                    sample_percentile(samples, 0.50, |sample| sample.ddgi_trace_gpu_us),
                    sample_percentile(samples, 0.95, |sample| sample.ddgi_trace_gpu_us),
                    sample_percentile(samples, 0.50, |sample| sample.ddgi_filter_gpu_us),
                    sample_percentile(samples, 0.95, |sample| sample.ddgi_filter_gpu_us),
                    sample_percentile(samples, 0.50, |sample| sample.render_trace_record_cpu_us),
                    sample_percentile(samples, 0.95, |sample| sample.render_trace_record_cpu_us),
                    ddgi_candidates,
                    ddgi_visible,
                    ddgi_occluded,
                );

                if state.count_index + 1 < LOCAL_LIGHT_SCALING_COUNTS.len() {
                    state.count_index += 1;
                    let (source_revision, registry_revision) =
                        publish_scaling_count(self, state.requested_count());
                    state.expected_source_revision = source_revision;
                    state.expected_registry_revision = registry_revision;
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = LocalLightScalingStage::AwaitLive;
                    state
                } else {
                    let ids = std::mem::take(
                        &mut self
                            .environment_lighting_test_scene
                            .as_mut()
                            .unwrap()
                            .local_light_scaling_ids,
                    );
                    for id in ids {
                        self.local_lights
                            .remove(id)
                            .expect("scaling cleanup must remove each live authored light");
                    }
                    let snapshot = self.local_lights.snapshot();
                    state.expected_source_revision = snapshot.source_revision();
                    state.expected_registry_revision = snapshot.registry_revision();
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = LocalLightScalingStage::AwaitFinalLive;
                    state
                }
            }
            LocalLightScalingStage::AwaitFinalLive => {
                if self.tracer.local_light_live_observation().state()
                    != (Some(state.expected_source_revision), 0)
                    || self.time_info.total_frame_count() <= state.mutation_frame
                {
                    return None;
                }
                state.stage = LocalLightScalingStage::AwaitFinalPublication;
                state
            }
            LocalLightScalingStage::AwaitFinalPublication => {
                let transport = self.tracer.ddgi_live_radiance_snapshot()?;
                if transport.local_lights.source_revision() != state.expected_source_revision
                    || transport.local_lights.count() != 0
                {
                    return None;
                }
                let status = self.tracer.ddgi_runtime_status();
                let active = status.active();
                let field = active.published_field?;
                if field.field().radiance_revision()
                    != transport.local_lights.info.transport_revision
                    || field.field().geometry_revision() != state.terrain_revision
                    || !is_converged_field(field)
                    || active.stage != DdgiVolumeStage::Ready
                    || active.building_field.is_some()
                    || status.staging().is_some()
                {
                    return None;
                }
                let evidence = self.tracer.ddgi_local_light_gpu_evidence()?;
                if !evidence.matches_classified_field(field)
                    || evidence.local_source_revision != state.expected_source_revision
                    || evidence.local_light_count != 0
                    || !evidence.is_complete()
                {
                    return None;
                }
                assert_eq!(evidence.totals.candidates, 0);
                assert_eq!(evidence.totals.irradiance_luma_q8, 0);
                assert!(
                    !self
                        .tracer
                        .ddgi_lighting_diagnostics()
                        .has_mixed_in_flight_revision
                );
                let (lag, coalesced) = self.tracer.local_light_transport_observability();
                assert_eq!(lag, 0);
                log::info!(
                    "[LOCAL_LIGHT_SCALE] complete counts=0,1,2,4,8 samples_per_count={} final_source_revision={} final_registry_revision={} final_transport_revision={} revision_lag=0 coalesced_live_revisions={} final_zero=true mixed_in_flight=false",
                    LOCAL_LIGHT_SCALING_SAMPLE_FRAMES,
                    state.expected_source_revision,
                    state.expected_registry_revision,
                    field.field().radiance_revision(),
                    coalesced,
                );
                return Some(TestScenePhase::Ready);
            }
        };
        Some(TestScenePhase::LocalLightScaling(next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_counts_cover_zero_through_the_full_small_n_capacity() {
        assert_eq!(LOCAL_LIGHT_SCALING_COUNTS, [1, 2, 4, 8, 0]);
        assert_eq!(LOCAL_LIGHT_SCALING_COUNTS[3], LOCAL_LIGHT_GPU_CAPACITY);
        assert_eq!(LOCAL_LIGHT_SCALING_COUNTS.last().copied(), Some(0));
        const { assert!(LOCAL_LIGHT_SCALING_SAMPLE_FRAMES >= 32) };
    }

    #[test]
    fn percentile_is_deterministic_for_even_sample_counts() {
        assert_eq!(percentile(vec![4.0, 1.0, 3.0, 2.0], 0.5), 3.0);
        assert_eq!(percentile(vec![4.0, 1.0, 3.0, 2.0], 0.95), 4.0);
    }
}
