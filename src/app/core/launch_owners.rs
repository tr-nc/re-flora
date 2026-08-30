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

pub(super) struct AutomationOwners {
    pub(super) capture: CaptureOwner,
    pub(super) benchmarks: crate::cli::BenchmarkPlan,
}

pub(super) enum CaptureOwner {
    None {
        screenshot: ScreenshotRuntime,
    },
    Snapshot {
        snapshot: String,
        screenshot: ScreenshotRuntime,
    },
    Screenshot {
        snapshot: String,
        screenshot: ScreenshotRuntime,
    },
    DenoiserBenchmark {
        snapshot: String,
        screenshot: ScreenshotRuntime,
        runtime: DenoiserBench,
    },
}

impl CaptureOwner {
    pub(super) fn snapshot_name(&self) -> Option<&str> {
        match self {
            Self::None { .. } => None,
            Self::Snapshot { snapshot, .. }
            | Self::Screenshot { snapshot, .. }
            | Self::DenoiserBenchmark { snapshot, .. } => Some(snapshot),
        }
    }

    pub(super) fn screenshot(&self) -> &ScreenshotRuntime {
        match self {
            Self::None { screenshot }
            | Self::Snapshot { screenshot, .. }
            | Self::Screenshot { screenshot, .. }
            | Self::DenoiserBenchmark { screenshot, .. } => screenshot,
        }
    }

    pub(super) fn screenshot_mut(&mut self) -> &mut ScreenshotRuntime {
        match self {
            Self::None { screenshot }
            | Self::Snapshot { screenshot, .. }
            | Self::Screenshot { screenshot, .. }
            | Self::DenoiserBenchmark { screenshot, .. } => screenshot,
        }
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

    fn waiting_particle_count(&self) -> Option<usize> {
        match self {
            Self::Pending => None,
            Self::Active(scene) => scene.waiting_particle_count(),
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

pub(super) enum WaterEvent<'a> {
    None,
    Experience(&'a WaterExperienceOwner),
    EditSoak(&'a WaterEditSoak),
}

impl WaterEvent<'_> {
    pub(super) fn waiting_particle_count(&self) -> Option<usize> {
        match self {
            Self::None | Self::EditSoak(_) => None,
            Self::Experience(owner) => owner.waiting_particle_count(),
        }
    }

    pub(super) fn edit_soak_step(&self) -> Option<usize> {
        match self {
            Self::None | Self::Experience(_) => None,
            Self::EditSoak(owner) => Some(owner.current_step()),
        }
    }
}

pub(super) enum WaterEventMut<'a> {
    None,
    Experience(&'a mut WaterExperienceOwner),
    EditSoak(&'a mut WaterEditSoak),
}

pub(super) enum AudioEvent<'a> {
    None,
    Canopy(&'a mut CanopyAudioDiagnosticRuntime),
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

impl AudioEvent<'_> {
    pub(super) fn is_canopy_diagnostic(&self) -> bool {
        match self {
            Self::None => false,
            Self::Canopy(_) => true,
        }
    }

    pub(super) fn budget_stress(&self) -> bool {
        match self {
            Self::None => false,
            Self::Canopy(runtime) => runtime.budget_stress(),
        }
    }

    pub(super) fn trajectory_sample(
        &mut self,
        tree_origin_world: glam::Vec3,
        time_seconds: f32,
    ) -> AudioTrajectorySample {
        match self {
            Self::None => AudioTrajectorySample::NotDiagnostic,
            Self::Canopy(runtime) => runtime.sample(tree_origin_world, time_seconds).map_or(
                AudioTrajectorySample::WaitingForStart,
                |(pose, elapsed, changed)| AudioTrajectorySample::Active(pose, elapsed, changed),
            ),
        }
    }

    pub(super) fn start_when_acoustics_are_ready(
        &mut self,
        time_seconds: f32,
        response_matches_published_scene: bool,
        snapshot: &CanopyAudioTelemetrySnapshot,
    ) -> bool {
        let runtime = match self {
            Self::None => return false,
            Self::Canopy(runtime) => runtime,
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

    pub(super) fn telemetry_marker(
        &self,
        tree_origin_world: glam::Vec3,
        time_seconds: f32,
    ) -> AudioTelemetryMarker {
        match self {
            Self::None => AudioTelemetryMarker::NotDiagnostic,
            Self::Canopy(runtime) => runtime
                .telemetry_marker(tree_origin_world, time_seconds)
                .map_or(AudioTelemetryMarker::WaitingForStart, |(elapsed, phase)| {
                    AudioTelemetryMarker::Active(elapsed, phase)
                }),
        }
    }

    pub(super) fn counters(
        &self,
        snapshot: &CanopyAudioTelemetrySnapshot,
    ) -> CanopyAudioDiagnosticCounters {
        match self {
            Self::None => CanopyAudioDiagnosticCounters::from_snapshot(snapshot),
            Self::Canopy(runtime) => runtime
                .counters(snapshot)
                .unwrap_or_else(|| CanopyAudioDiagnosticCounters::from_snapshot(snapshot)),
        }
    }
}

impl WaterEventMut<'_> {
    pub(super) fn activate_experience(&mut self, expected_particle_count: usize) {
        match self {
            Self::Experience(owner) => owner.activate(expected_particle_count),
            Self::None | Self::EditSoak(_) => {
                panic!("only a water-experience scenario can be activated")
            }
        }
    }

    pub(super) fn mark_experience_ready(&mut self) {
        match self {
            Self::Experience(owner) => owner.mark_ready(),
            Self::None | Self::EditSoak(_) => {
                panic!("only a water-experience scenario can become ready")
            }
        }
    }

    pub(super) fn advance_edit_soak(&mut self) -> bool {
        match self {
            Self::EditSoak(owner) => owner.advance(),
            Self::None | Self::Experience(_) => false,
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

    pub(super) fn water_event(&self) -> WaterEvent<'_> {
        match self {
            Self::Water(WaterScenarioOwner::Experience(owner)) => WaterEvent::Experience(owner),
            Self::Water(WaterScenarioOwner::EditSoak(owner)) => WaterEvent::EditSoak(owner),
            Self::World(_) | Self::TestScene(_) | Self::Diagnostic(_) => WaterEvent::None,
        }
    }

    pub(super) fn water_event_mut(&mut self) -> WaterEventMut<'_> {
        match self {
            Self::Water(WaterScenarioOwner::Experience(owner)) => WaterEventMut::Experience(owner),
            Self::Water(WaterScenarioOwner::EditSoak(owner)) => WaterEventMut::EditSoak(owner),
            Self::World(_) | Self::TestScene(_) | Self::Diagnostic(_) => WaterEventMut::None,
        }
    }

    pub(super) fn audio_event(&mut self) -> AudioEvent<'_> {
        match self {
            Self::Diagnostic(DiagnosticScenarioOwner::CanopyAudio(runtime)) => {
                AudioEvent::Canopy(runtime)
            }
            Self::Diagnostic(
                DiagnosticScenarioOwner::TerrainConnectivity(_)
                | DiagnosticScenarioOwner::FoliageShadow(_),
            )
            | Self::World(_)
            | Self::Water(_)
            | Self::TestScene(_) => AudioEvent::None,
        }
    }
}

pub(in crate::app) struct StartupOwners {
    automation: AutomationOwners,
    scenario: ScenarioOwner,
}

impl StartupOwners {
    pub(super) fn into_parts(self) -> (AutomationOwners, ScenarioOwner) {
        (self.automation, self.scenario)
    }
}

pub(in crate::app) fn prepare_startup_owners(
    automation: AutomationPlan,
    scenario: Scenario,
) -> Result<StartupOwners, String> {
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
            return Ok(StartupOwners {
                automation: AutomationOwners {
                    capture: CaptureOwner::None {
                        screenshot: ScreenshotRuntime::new(None),
                    },
                    benchmarks,
                },
                scenario: ScenarioOwner::Diagnostic(DiagnosticScenarioOwner::FoliageShadow(
                    DenoiserBench::new_foliage(options),
                )),
            });
        }
    };
    let capture = match camera {
        CameraAutomation::None => CaptureOwner::None {
            screenshot: ScreenshotRuntime::new(None),
        },
        CameraAutomation::Snapshot(snapshot) => CaptureOwner::Snapshot {
            snapshot,
            screenshot: ScreenshotRuntime::new(None),
        },
        CameraAutomation::Screenshot { snapshot, capture } => CaptureOwner::Screenshot {
            snapshot,
            screenshot: ScreenshotRuntime::new(Some(capture)),
        },
        CameraAutomation::DenoiserBenchmark {
            snapshot,
            benchmark,
        } => CaptureOwner::DenoiserBenchmark {
            snapshot,
            screenshot: ScreenshotRuntime::new(None),
            runtime: DenoiserBench::new_camera(benchmark),
        },
    };
    Ok(StartupOwners {
        automation: AutomationOwners {
            capture,
            benchmarks,
        },
        scenario: scenario_owner,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{
        BenchmarkPlan, CameraDenoiserOptions, CameraMotion, DenoiserCaptureOptions,
        FoliageDenoiserOptions,
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
            .scenario
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
    fn scenario_owner_does_not_regrow_parallel_optional_getters() {
        let source = include_str!("launch_owners.rs");
        for old_method in [
            "denoiser",
            "is_water_experience",
            "is_house",
            "canopy_audio",
            "canopy_audio_mut",
            "water_edit_soak",
            "water_edit_soak_mut",
            "water_experience",
            "water_experience_mut",
            "environment_lighting",
            "environment_lighting_mut",
            "hybrid_transparency",
            "hybrid_transparency_mut",
            "terrain_connectivity",
            "terrain_connectivity_mut",
            "foliage_shadow_benchmark",
            "foliage_shadow_benchmark_mut",
        ] {
            assert!(
                !source.contains(&format!("fn {old_method}(")),
                "old optional getter {old_method} must remain deleted",
            );
        }
        assert!(!source.contains(concat!("fn take_", "terrain_connectivity(")));
        assert!(!source.contains(concat!("fn restore_", "terrain_connectivity(")));
    }

    #[test]
    fn non_diagnostic_scenarios_never_request_the_canopy_camera_pose() {
        let mut garden = owner_for(Scenario::Garden);
        match garden
            .audio_event()
            .trajectory_sample(glam::Vec3::ZERO, 1.0)
        {
            AudioTrajectorySample::NotDiagnostic => {}
            AudioTrajectorySample::WaitingForStart | AudioTrajectorySample::Active(_, _, _) => {
                panic!("garden must not participate in canopy camera automation")
            }
        }

        let mut canopy = owner_for(Scenario::CanopyAudioDiagnostic {
            constrained_budget: false,
        });
        match canopy
            .audio_event()
            .trajectory_sample(glam::Vec3::ZERO, 1.0)
        {
            AudioTrajectorySample::WaitingForStart => {}
            AudioTrajectorySample::NotDiagnostic | AudioTrajectorySample::Active(_, _, _) => {
                panic!("unstarted canopy diagnostic must hold its initial camera pose")
            }
        }
    }

    #[test]
    fn terrain_connectivity_scenario_owns_its_protocol_outside_standard_scenarios() {
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
            ScenarioOwner::Connectivity(_)
        ));
        assert!(matches!(
            owner_for(Scenario::Garden),
            ScenarioOwner::Standard(_)
        ));

        let source = include_str!("launch_owners.rs");
        assert!(
            !source.contains("dispatch_connectivity"),
            "standard scenarios must not pass through a borrowed connectivity dispatcher",
        );
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
