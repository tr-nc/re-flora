use super::{
    denoiser_bench::DenoiserBench,
    environment_lighting_test_scene::EnvironmentLightingTestScene,
    hybrid_transparency_test_scene::HybridTransparencyTestScene,
    screenshot::ScreenshotRuntime,
    terrain_connectivity::bench::TerrainConnectivityBench,
    water::{self, WaterEditSoak},
    water_experience_scene::WaterExperienceScene,
    CanopyAudioDiagnosticCounters, CanopyAudioDiagnosticRuntime,
};
use crate::audio::{
    CanopyAudioDiagnosticPose, CanopyAudioTelemetrySnapshot, CanopyAudioTrajectoryPhase,
};
use crate::cli::{AutomationPlan, CameraAutomation, Scenario};
use re_flora_vkn::GpuProfilerFrameResults;

pub(super) struct AutomationOwners {
    pub(super) capture: CaptureOwner,
    pub(super) benchmarks: crate::cli::BenchmarkPlan,
}

pub(super) struct CaptureOwner {
    pub(super) screenshot: ScreenshotRuntime,
    pub(super) mode: CameraCapture,
}

pub(super) enum CameraCapture {
    None,
    Snapshot {
        name: String,
    },
    DenoiserBenchmark {
        snapshot: String,
        runtime: DenoiserBench,
    },
}

impl CaptureOwner {
    pub(super) fn snapshot_name(&self) -> Option<&str> {
        match &self.mode {
            CameraCapture::None => None,
            CameraCapture::Snapshot { name } => Some(name),
            CameraCapture::DenoiserBenchmark { snapshot, .. } => Some(snapshot),
        }
    }

    pub(super) fn screenshot(&self) -> &ScreenshotRuntime {
        &self.screenshot
    }

    pub(super) fn screenshot_mut(&mut self) -> &mut ScreenshotRuntime {
        &mut self.screenshot
    }
}

pub(super) enum WaterExperienceOwner {
    Pending,
    Active(WaterExperienceScene),
}

impl WaterExperienceOwner {
    pub(super) fn activate(&mut self, expected_particle_count: usize) {
        *self = Self::Active(WaterExperienceScene::new(expected_particle_count));
    }

    fn progress(&self) -> WaterProgress {
        match self {
            Self::Pending => WaterProgress::ExperiencePending,
            Self::Active(scene) => scene.waiting_particle_count().map_or(
                WaterProgress::ExperienceReady,
                |expected_particle_count| WaterProgress::ExperienceWaiting {
                    expected_particle_count,
                },
            ),
        }
    }

    fn mark_ready(&mut self) {
        match self {
            Self::Pending => panic!("water experience is not active"),
            Self::Active(scene) => scene.mark_ready(),
        }
    }
}

pub(super) struct HouseSceneOwner;
pub(super) enum WorldScenarioOwner {
    Garden,
    House(HouseSceneOwner),
}

pub(super) enum WaterScenarioOwner {
    Experience(WaterExperienceOwner),
    EditSoak(WaterEditSoak),
}

pub(super) enum TestSceneOwner {
    Environment(EnvironmentLightingTestScene),
    Hybrid(HybridTransparencyTestScene),
}

pub(super) enum DiagnosticScenarioOwner {
    CanopyAudio(CanopyAudioDiagnosticRuntime),
    TerrainConnectivity(TerrainConnectivityBench),
    FoliageShadow(DenoiserBench),
}

pub(super) enum ScenarioOwner {
    World(WorldScenarioOwner),
    Water(WaterScenarioOwner),
    TestScene(TestSceneOwner),
    Diagnostic(DiagnosticScenarioOwner),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoadingDirective {
    Garden,
    WaterExperience,
    House,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WaterProgress {
    Inactive,
    ExperiencePending,
    ExperienceWaiting { expected_particle_count: usize },
    ExperienceReady,
    EditSoak { step: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CanopyAudioMode {
    Disabled,
    Diagnostic { budget_stress: bool },
}

pub(super) enum TestSceneEvent<'a> {
    None,
    Environment(&'a EnvironmentLightingTestScene),
    Hybrid(&'a HybridTransparencyTestScene),
}

impl<'a> TestSceneEvent<'a> {
    pub(super) fn environment(self) -> &'a EnvironmentLightingTestScene {
        match self {
            Self::Environment(scene) => scene,
            Self::None | Self::Hybrid(_) => {
                panic!("environment-test behavior requires the environment test scene")
            }
        }
    }

    pub(super) fn owns_capture_scene(&self) -> bool {
        match self {
            Self::None => false,
            Self::Environment(_) | Self::Hybrid(_) => true,
        }
    }

    pub(super) fn capture_is_ready(&self) -> bool {
        match self {
            Self::None => true,
            Self::Environment(scene) => scene.is_ready(),
            Self::Hybrid(scene) => scene.is_ready(),
        }
    }

    pub(super) fn hides_terrain_edit_preview(&self) -> bool {
        match self {
            Self::Environment(scene) => scene.hides_terrain_edit_preview(),
            Self::None | Self::Hybrid(_) => false,
        }
    }
}

pub(super) enum TestSceneEventMut<'a> {
    None,
    Environment(&'a mut EnvironmentLightingTestScene),
    Hybrid(&'a mut HybridTransparencyTestScene),
}

impl<'a> TestSceneEventMut<'a> {
    pub(super) fn environment(self) -> &'a mut EnvironmentLightingTestScene {
        match self {
            Self::Environment(scene) => scene,
            Self::None | Self::Hybrid(_) => {
                panic!("environment-test behavior requires the environment test scene")
            }
        }
    }
}

pub(super) enum FoliageCaptureEvent<'a> {
    None,
    Active(&'a DenoiserBench),
}

pub(super) enum FoliageCaptureEventMut<'a> {
    None,
    Active(&'a mut DenoiserBench),
}

pub(super) enum AudioTelemetryMarker {
    NotDiagnostic,
    WaitingForStart,
    Active(f32, CanopyAudioTrajectoryPhase),
}

pub(super) enum AudioTrajectorySample {
    NotDiagnostic,
    WaitingForStart,
    Active(CanopyAudioDiagnosticPose, f32, bool),
}

pub(super) enum ConnectivityEvent<'a> {
    None,
    Active(&'a mut TerrainConnectivityBench),
}

impl ConnectivityEvent<'_> {
    pub(super) fn source_frame(&self, frame_slot: usize) -> Option<u64> {
        match self {
            Self::None => None,
            Self::Active(bench) => bench.gpu_source_frame(frame_slot),
        }
    }

    pub(super) fn note_gpu_frame_started(&mut self, frame_slot: usize, frame: u64) {
        match self {
            Self::None => {}
            Self::Active(bench) => bench.note_gpu_frame_started(frame_slot, frame),
        }
    }

    pub(super) fn observe_gpu_results(
        &mut self,
        source_frame: Option<u64>,
        results: &GpuProfilerFrameResults,
    ) {
        match self {
            Self::None => {}
            Self::Active(bench) => bench.observe_gpu_results(source_frame, results),
        }
    }

    pub(super) fn isolates_particle_capacity(&self) -> bool {
        match self {
            Self::None => false,
            Self::Active(bench) => bench.active(),
        }
    }
}

impl DiagnosticScenarioOwner {
    fn canopy_audio_mode(&self) -> CanopyAudioMode {
        match self {
            Self::CanopyAudio(runtime) => CanopyAudioMode::Diagnostic {
                budget_stress: runtime.budget_stress(),
            },
            Self::TerrainConnectivity(_) | Self::FoliageShadow(_) => CanopyAudioMode::Disabled,
        }
    }

    fn sample_canopy_audio_trajectory(
        &mut self,
        tree_origin_world: glam::Vec3,
        time_seconds: f32,
    ) -> AudioTrajectorySample {
        match self {
            Self::CanopyAudio(runtime) => runtime.sample(tree_origin_world, time_seconds).map_or(
                AudioTrajectorySample::WaitingForStart,
                |(pose, elapsed, changed)| AudioTrajectorySample::Active(pose, elapsed, changed),
            ),
            Self::TerrainConnectivity(_) | Self::FoliageShadow(_) => {
                AudioTrajectorySample::NotDiagnostic
            }
        }
    }

    fn start_canopy_audio_when_ready(
        &mut self,
        time_seconds: f32,
        response_matches_published_scene: bool,
        snapshot: &CanopyAudioTelemetrySnapshot,
    ) -> bool {
        let runtime = match self {
            Self::CanopyAudio(runtime) => runtime,
            Self::TerrainConnectivity(_) | Self::FoliageShadow(_) => return false,
        };
        if runtime.started()
            || !runtime.observe_acoustic_readiness(
                time_seconds,
                response_matches_published_scene,
                snapshot.petal_render_rejected_response_count,
            )
        {
            return false;
        }
        runtime.start(time_seconds, snapshot);
        true
    }

    fn canopy_audio_telemetry_marker(
        &self,
        tree_origin_world: glam::Vec3,
        time_seconds: f32,
    ) -> AudioTelemetryMarker {
        match self {
            Self::CanopyAudio(runtime) => runtime
                .telemetry_marker(tree_origin_world, time_seconds)
                .map_or(AudioTelemetryMarker::WaitingForStart, |(elapsed, phase)| {
                    AudioTelemetryMarker::Active(elapsed, phase)
                }),
            Self::TerrainConnectivity(_) | Self::FoliageShadow(_) => {
                AudioTelemetryMarker::NotDiagnostic
            }
        }
    }

    fn canopy_audio_counters(
        &self,
        snapshot: &CanopyAudioTelemetrySnapshot,
    ) -> CanopyAudioDiagnosticCounters {
        match self {
            Self::CanopyAudio(runtime) => runtime
                .counters(snapshot)
                .unwrap_or_else(|| CanopyAudioDiagnosticCounters::from_snapshot(snapshot)),
            Self::TerrainConnectivity(_) | Self::FoliageShadow(_) => {
                CanopyAudioDiagnosticCounters::from_snapshot(snapshot)
            }
        }
    }
}

impl WaterScenarioOwner {
    fn progress(&self) -> WaterProgress {
        match self {
            Self::Experience(owner) => owner.progress(),
            Self::EditSoak(owner) => WaterProgress::EditSoak {
                step: owner.current_step(),
            },
        }
    }

    fn activate_experience(&mut self, expected_particle_count: usize) {
        match self {
            Self::Experience(owner) => owner.activate(expected_particle_count),
            Self::EditSoak(_) => {
                panic!("only a water-experience scenario can be activated")
            }
        }
    }

    fn mark_experience_ready(&mut self) {
        match self {
            Self::Experience(owner) => owner.mark_ready(),
            Self::EditSoak(_) => {
                panic!("only a water-experience scenario can become ready")
            }
        }
    }

    fn advance_edit_soak(&mut self) -> bool {
        match self {
            Self::EditSoak(owner) => owner.advance(),
            Self::Experience(_) => false,
        }
    }
}

impl ScenarioOwner {
    pub(super) fn loading_directive(&self) -> LoadingDirective {
        match self {
            Self::World(WorldScenarioOwner::Garden) => LoadingDirective::Garden,
            Self::World(WorldScenarioOwner::House(_)) => LoadingDirective::House,
            Self::Water(WaterScenarioOwner::Experience(_)) => LoadingDirective::WaterExperience,
            Self::Water(WaterScenarioOwner::EditSoak(_))
            | Self::TestScene(_)
            | Self::Diagnostic(_) => LoadingDirective::Garden,
        }
    }

    pub(super) fn test_scene_event(&self) -> TestSceneEvent<'_> {
        match self {
            Self::TestScene(TestSceneOwner::Environment(scene)) => {
                TestSceneEvent::Environment(scene)
            }
            Self::TestScene(TestSceneOwner::Hybrid(scene)) => TestSceneEvent::Hybrid(scene),
            Self::World(_) | Self::Water(_) | Self::Diagnostic(_) => TestSceneEvent::None,
        }
    }

    pub(super) fn test_scene_event_mut(&mut self) -> TestSceneEventMut<'_> {
        match self {
            Self::TestScene(TestSceneOwner::Environment(scene)) => {
                TestSceneEventMut::Environment(scene)
            }
            Self::TestScene(TestSceneOwner::Hybrid(scene)) => TestSceneEventMut::Hybrid(scene),
            Self::World(_) | Self::Water(_) | Self::Diagnostic(_) => TestSceneEventMut::None,
        }
    }

    pub(super) fn foliage_capture_event(&self) -> FoliageCaptureEvent<'_> {
        match self {
            Self::Diagnostic(DiagnosticScenarioOwner::FoliageShadow(bench)) => {
                FoliageCaptureEvent::Active(bench)
            }
            Self::Diagnostic(
                DiagnosticScenarioOwner::CanopyAudio(_)
                | DiagnosticScenarioOwner::TerrainConnectivity(_),
            )
            | Self::World(_)
            | Self::Water(_)
            | Self::TestScene(_) => FoliageCaptureEvent::None,
        }
    }

    pub(super) fn foliage_capture_event_mut(&mut self) -> FoliageCaptureEventMut<'_> {
        match self {
            Self::Diagnostic(DiagnosticScenarioOwner::FoliageShadow(bench)) => {
                FoliageCaptureEventMut::Active(bench)
            }
            Self::Diagnostic(
                DiagnosticScenarioOwner::CanopyAudio(_)
                | DiagnosticScenarioOwner::TerrainConnectivity(_),
            )
            | Self::World(_)
            | Self::Water(_)
            | Self::TestScene(_) => FoliageCaptureEventMut::None,
        }
    }

    pub(super) fn water_progress(&self) -> WaterProgress {
        match self {
            Self::Water(owner) => owner.progress(),
            Self::World(_) | Self::TestScene(_) | Self::Diagnostic(_) => WaterProgress::Inactive,
        }
    }

    pub(super) fn activate_water_experience(&mut self, expected_particle_count: usize) {
        match self {
            Self::Water(owner) => owner.activate_experience(expected_particle_count),
            Self::World(_) | Self::TestScene(_) | Self::Diagnostic(_) => {
                panic!("only a water-experience scenario can be activated")
            }
        }
    }

    pub(super) fn mark_water_experience_ready(&mut self) {
        match self {
            Self::Water(owner) => owner.mark_experience_ready(),
            Self::World(_) | Self::TestScene(_) | Self::Diagnostic(_) => {
                panic!("only a water-experience scenario can become ready")
            }
        }
    }

    pub(super) fn advance_water_edit_soak(&mut self) -> bool {
        match self {
            Self::Water(owner) => owner.advance_edit_soak(),
            Self::World(_) | Self::TestScene(_) | Self::Diagnostic(_) => false,
        }
    }

    pub(super) fn canopy_audio_mode(&self) -> CanopyAudioMode {
        match self {
            Self::Diagnostic(owner) => owner.canopy_audio_mode(),
            Self::World(_) | Self::Water(_) | Self::TestScene(_) => CanopyAudioMode::Disabled,
        }
    }

    pub(super) fn sample_canopy_audio_trajectory(
        &mut self,
        tree_origin_world: glam::Vec3,
        time_seconds: f32,
    ) -> AudioTrajectorySample {
        match self {
            Self::Diagnostic(owner) => {
                owner.sample_canopy_audio_trajectory(tree_origin_world, time_seconds)
            }
            Self::World(_) | Self::Water(_) | Self::TestScene(_) => {
                AudioTrajectorySample::NotDiagnostic
            }
        }
    }

    pub(super) fn start_canopy_audio_when_ready(
        &mut self,
        time_seconds: f32,
        response_matches_published_scene: bool,
        snapshot: &CanopyAudioTelemetrySnapshot,
    ) -> bool {
        match self {
            Self::Diagnostic(owner) => owner.start_canopy_audio_when_ready(
                time_seconds,
                response_matches_published_scene,
                snapshot,
            ),
            Self::World(_) | Self::Water(_) | Self::TestScene(_) => false,
        }
    }

    pub(super) fn canopy_audio_telemetry_marker(
        &self,
        tree_origin_world: glam::Vec3,
        time_seconds: f32,
    ) -> AudioTelemetryMarker {
        match self {
            Self::Diagnostic(owner) => {
                owner.canopy_audio_telemetry_marker(tree_origin_world, time_seconds)
            }
            Self::World(_) | Self::Water(_) | Self::TestScene(_) => {
                AudioTelemetryMarker::NotDiagnostic
            }
        }
    }

    pub(super) fn canopy_audio_counters(
        &self,
        snapshot: &CanopyAudioTelemetrySnapshot,
    ) -> CanopyAudioDiagnosticCounters {
        match self {
            Self::Diagnostic(owner) => owner.canopy_audio_counters(snapshot),
            Self::World(_) | Self::Water(_) | Self::TestScene(_) => {
                CanopyAudioDiagnosticCounters::from_snapshot(snapshot)
            }
        }
    }

    pub(super) fn connectivity_event(&mut self) -> ConnectivityEvent<'_> {
        match self {
            Self::Diagnostic(DiagnosticScenarioOwner::TerrainConnectivity(bench)) => {
                ConnectivityEvent::Active(bench)
            }
            Self::Diagnostic(
                DiagnosticScenarioOwner::CanopyAudio(_) | DiagnosticScenarioOwner::FoliageShadow(_),
            )
            | Self::World(_)
            | Self::Water(_)
            | Self::TestScene(_) => ConnectivityEvent::None,
        }
    }
}

pub(in crate::app) struct LaunchOwners {
    mode: LaunchMode,
}

enum LaunchMode {
    Standard {
        capture: CaptureOwner,
        benchmarks: crate::cli::BenchmarkPlan,
        scenario: ScenarioOwner,
    },
    FoliageShadow {
        screenshot: ScreenshotRuntime,
        benchmarks: crate::cli::BenchmarkPlan,
        runtime: DenoiserBench,
    },
}

impl LaunchOwners {
    pub(super) fn into_parts(self) -> (AutomationOwners, ScenarioOwner) {
        match self.mode {
            LaunchMode::Standard {
                capture,
                benchmarks,
                scenario,
            } => (
                AutomationOwners {
                    capture,
                    benchmarks,
                },
                scenario,
            ),
            LaunchMode::FoliageShadow {
                screenshot,
                benchmarks,
                runtime,
            } => (
                AutomationOwners {
                    capture: CaptureOwner {
                        screenshot,
                        mode: CameraCapture::None,
                    },
                    benchmarks,
                },
                ScenarioOwner::Diagnostic(DiagnosticScenarioOwner::FoliageShadow(runtime)),
            ),
        }
    }
}

pub(in crate::app) fn prepare_startup_owners(
    automation: AutomationPlan,
    scenario: Scenario,
) -> Result<LaunchOwners, String> {
    let AutomationPlan { camera, benchmarks } = automation;
    let scenario_owner = match scenario {
        Scenario::Garden => ScenarioOwner::World(WorldScenarioOwner::Garden),
        Scenario::CanopyAudioDiagnostic { constrained_budget } => {
            ScenarioOwner::Diagnostic(DiagnosticScenarioOwner::CanopyAudio(
                CanopyAudioDiagnosticRuntime::new(constrained_budget),
            ))
        }
        Scenario::WaterExperience => ScenarioOwner::Water(WaterScenarioOwner::Experience(
            WaterExperienceOwner::Pending,
        )),
        Scenario::WaterEditSoak => {
            ScenarioOwner::Water(WaterScenarioOwner::EditSoak(water::WaterEditSoak::default()))
        }
        Scenario::EnvironmentLighting(case) => ScenarioOwner::TestScene(
            TestSceneOwner::Environment(EnvironmentLightingTestScene::new(case)),
        ),
        Scenario::HybridTransparency => {
            ScenarioOwner::TestScene(TestSceneOwner::Hybrid(HybridTransparencyTestScene::new()))
        }
        Scenario::House => ScenarioOwner::World(WorldScenarioOwner::House(HouseSceneOwner)),
        Scenario::TerrainConnectivityBenchmark(options) => ScenarioOwner::Diagnostic(
            DiagnosticScenarioOwner::TerrainConnectivity(TerrainConnectivityBench::new(options)),
        ),
        Scenario::FoliageShadowBenchmark(options) => {
            let CameraAutomation::None = camera else {
                return Err(
                    "foliage-shadow scenario cannot carry a second camera automation".to_owned(),
                );
            };
            return Ok(LaunchOwners {
                mode: LaunchMode::FoliageShadow {
                    screenshot: ScreenshotRuntime::new(None),
                    benchmarks,
                    runtime: DenoiserBench::new_foliage(options),
                },
            });
        }
    };
    let (mode, screenshot) = match camera {
        CameraAutomation::None => (CameraCapture::None, ScreenshotRuntime::new(None)),
        CameraAutomation::Snapshot(name) => (
            CameraCapture::Snapshot { name },
            ScreenshotRuntime::new(None),
        ),
        CameraAutomation::Screenshot { snapshot, capture } => (
            CameraCapture::Snapshot { name: snapshot },
            ScreenshotRuntime::new(Some(capture)),
        ),
        CameraAutomation::DenoiserBenchmark {
            snapshot,
            benchmark,
        } => (
            CameraCapture::DenoiserBenchmark {
                snapshot,
                runtime: DenoiserBench::new_camera(benchmark),
            },
            ScreenshotRuntime::new(None),
        ),
    };
    Ok(LaunchOwners {
        mode: LaunchMode::Standard {
            capture: CaptureOwner { screenshot, mode },
            benchmarks,
            scenario: scenario_owner,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{
        BenchmarkPlan, CameraDenoiserOptions, CameraMotion, DenoiserCaptureOptions,
        FoliageDenoiserOptions, ScreenshotOptions,
    };

    fn capture_options() -> DenoiserCaptureOptions {
        DenoiserCaptureOptions {
            report_path: "report.toml".to_owned(),
            warmup_frames: 12,
            capture_frames: 8,
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum OwnerKind {
        Garden,
        House,
        WaterExperience,
        WaterEditSoak,
        Environment,
        Hybrid,
        CanopyAudio,
        TerrainConnectivity,
        FoliageShadow,
    }

    fn owner_kind(owner: ScenarioOwner) -> OwnerKind {
        match owner {
            ScenarioOwner::World(world) => match world {
                WorldScenarioOwner::Garden => OwnerKind::Garden,
                WorldScenarioOwner::House(_) => OwnerKind::House,
            },
            ScenarioOwner::Water(water) => match water {
                WaterScenarioOwner::Experience(_) => OwnerKind::WaterExperience,
                WaterScenarioOwner::EditSoak(_) => OwnerKind::WaterEditSoak,
            },
            ScenarioOwner::TestScene(test_scene) => match test_scene {
                TestSceneOwner::Environment(_) => OwnerKind::Environment,
                TestSceneOwner::Hybrid(_) => OwnerKind::Hybrid,
            },
            ScenarioOwner::Diagnostic(diagnostic) => match diagnostic {
                DiagnosticScenarioOwner::CanopyAudio(_) => OwnerKind::CanopyAudio,
                DiagnosticScenarioOwner::TerrainConnectivity(_) => OwnerKind::TerrainConnectivity,
                DiagnosticScenarioOwner::FoliageShadow(_) => OwnerKind::FoliageShadow,
            },
        }
    }

    fn owner_for(scenario: Scenario) -> ScenarioOwner {
        prepare_startup_owners(AutomationPlan::default(), scenario)
            .unwrap()
            .into_parts()
            .1
    }

    fn capture_for(camera: CameraAutomation) -> CaptureOwner {
        match prepare_startup_owners(
            AutomationPlan {
                camera,
                benchmarks: BenchmarkPlan::default(),
            },
            Scenario::Garden,
        )
        .unwrap()
        .mode
        {
            LaunchMode::Standard { capture, .. } => capture,
            LaunchMode::FoliageShadow { .. } => {
                panic!("garden must use standard launch ownership")
            }
        }
    }

    #[test]
    fn capture_modes_share_exactly_one_screenshot_runtime_owner() {
        let cases = [
            (CameraAutomation::None, None, false, false),
            (
                CameraAutomation::Snapshot("tree".to_owned()),
                Some("tree"),
                false,
                false,
            ),
            (
                CameraAutomation::Screenshot {
                    snapshot: "tree".to_owned(),
                    capture: ScreenshotOptions {
                        path: "frame.png".to_owned(),
                        delay: 0.5,
                    },
                },
                Some("tree"),
                true,
                false,
            ),
            (
                CameraAutomation::DenoiserBenchmark {
                    snapshot: "tree".to_owned(),
                    benchmark: CameraDenoiserOptions {
                        capture: capture_options(),
                        camera_motion: CameraMotion::Fixed,
                    },
                },
                Some("tree"),
                false,
                true,
            ),
        ];

        for (camera, expected_snapshot, expected_scheduled, expected_denoiser) in cases {
            let CaptureOwner { screenshot, mode } = capture_for(camera);
            assert_eq!(screenshot.is_scheduled(), expected_scheduled);
            match mode {
                CameraCapture::None => {
                    assert_eq!(expected_snapshot, None);
                    assert!(!expected_denoiser);
                }
                CameraCapture::Snapshot { name } => {
                    assert_eq!(Some(name.as_str()), expected_snapshot);
                    assert!(!expected_denoiser);
                }
                CameraCapture::DenoiserBenchmark { snapshot, .. } => {
                    assert_eq!(Some(snapshot.as_str()), expected_snapshot);
                    assert!(expected_denoiser);
                }
            }
        }
    }

    #[test]
    fn foliage_shadow_is_a_structurally_exclusive_launch_owner() {
        let launch = prepare_startup_owners(
            AutomationPlan::default(),
            Scenario::FoliageShadowBenchmark(FoliageDenoiserOptions {
                capture: capture_options(),
            }),
        )
        .unwrap();

        match launch.mode {
            LaunchMode::Standard { .. } => {
                panic!("foliage benchmark must not be a standard scenario/camera pair")
            }
            LaunchMode::FoliageShadow {
                screenshot,
                runtime,
                benchmarks: _,
            } => {
                assert!(!screenshot.is_scheduled());
                assert!(runtime.is_foliage_shadow());
            }
        }
    }

    #[test]
    fn all_fixed_scenarios_construct_their_exhaustive_family_owner() {
        let cases = [
            (
                Scenario::CanopyAudioDiagnostic {
                    constrained_budget: false,
                },
                OwnerKind::CanopyAudio,
            ),
            (Scenario::WaterExperience, OwnerKind::WaterExperience),
            (Scenario::WaterEditSoak, OwnerKind::WaterEditSoak),
            (
                Scenario::EnvironmentLighting(crate::cli::EnvironmentLightingTestCase::Sealed),
                OwnerKind::Environment,
            ),
            (Scenario::HybridTransparency, OwnerKind::Hybrid),
            (Scenario::House, OwnerKind::House),
            (
                Scenario::TerrainConnectivityBenchmark(
                    crate::cli::TerrainConnectivityBenchOptions {
                        mode: crate::cli::TerrainConnectivityBenchMode::Correct,
                        available_particles: 8,
                        warmup_frames: 1,
                        observe_frames: 1,
                        voxel_budget: 8,
                    },
                ),
                OwnerKind::TerrainConnectivity,
            ),
            (
                Scenario::FoliageShadowBenchmark(FoliageDenoiserOptions {
                    capture: capture_options(),
                }),
                OwnerKind::FoliageShadow,
            ),
        ];

        for (scenario, expected) in cases {
            assert_eq!(owner_kind(owner_for(scenario)), expected);
        }
        assert_eq!(owner_kind(owner_for(Scenario::Garden)), OwnerKind::Garden);
    }

    #[test]
    fn non_diagnostic_scenarios_never_request_the_canopy_camera_pose() {
        let mut garden = owner_for(Scenario::Garden);
        match garden.sample_canopy_audio_trajectory(glam::Vec3::ZERO, 1.0) {
            AudioTrajectorySample::NotDiagnostic => {}
            AudioTrajectorySample::WaitingForStart | AudioTrajectorySample::Active(_, _, _) => {
                panic!("garden must not participate in canopy camera automation")
            }
        }

        let mut canopy = owner_for(Scenario::CanopyAudioDiagnostic {
            constrained_budget: false,
        });
        match canopy.sample_canopy_audio_trajectory(glam::Vec3::ZERO, 1.0) {
            AudioTrajectorySample::WaitingForStart => {}
            AudioTrajectorySample::NotDiagnostic | AudioTrajectorySample::Active(_, _, _) => {
                panic!("unstarted canopy diagnostic must hold its initial camera pose")
            }
        }
    }

    #[test]
    fn water_progress_is_an_owned_closed_port() {
        let experience = owner_for(Scenario::WaterExperience);
        assert_eq!(
            experience.water_progress(),
            WaterProgress::ExperiencePending
        );

        let edit_soak = owner_for(Scenario::WaterEditSoak);
        assert_eq!(
            edit_soak.water_progress(),
            WaterProgress::EditSoak { step: 0 }
        );

        let garden = owner_for(Scenario::Garden);
        assert_eq!(garden.water_progress(), WaterProgress::Inactive);
    }

    #[test]
    fn terrain_connectivity_scenario_uses_the_diagnostic_family() {
        let owner = owner_for(Scenario::TerrainConnectivityBenchmark(
            crate::cli::TerrainConnectivityBenchOptions {
                mode: crate::cli::TerrainConnectivityBenchMode::Correct,
                available_particles: 8,
                warmup_frames: 1,
                observe_frames: 1,
                voxel_budget: 8,
            },
        ));

        assert!(matches!(
            owner,
            ScenarioOwner::Diagnostic(DiagnosticScenarioOwner::TerrainConnectivity(_))
        ));
    }

    #[test]
    fn contradictory_foliage_and_camera_benchmarks_fail_before_owner_construction() {
        let automation = AutomationPlan {
            camera: CameraAutomation::DenoiserBenchmark {
                snapshot: "tree".to_owned(),
                benchmark: CameraDenoiserOptions {
                    capture: capture_options(),
                    camera_motion: CameraMotion::Fixed,
                },
            },
            benchmarks: BenchmarkPlan::default(),
        };

        let error = prepare_startup_owners(
            automation,
            Scenario::FoliageShadowBenchmark(FoliageDenoiserOptions {
                capture: capture_options(),
            }),
        )
        .err()
        .expect("contradictory owners must be rejected");

        assert!(error.contains("second camera automation"));
    }
}
