#[cfg(test)]
use super::denoiser_bench::{
    CameraDenoiserCommand, CameraDenoiserPresentation, CameraFrameMotion, DenoiserCaptureStep,
    DenoiserFrame, DenoiserFrameRun, DenoiserReadbackStep, DenoiserUiStep, FixedVisualFrame,
    FoliageDenoiserCommand, FoliageDenoiserPresentation,
};
#[cfg(test)]
use super::CanopyAudioDiagnosticCounters;
use super::{
    authored_flora_bench::AuthoredFloraBench,
    canopy_audio_diagnostic::CanopyAudioDiagnosticRuntime,
    denoiser_bench::{
        DenoiserBench, DenoiserFrameCommand, DenoiserFrameCompletion, DenoiserReadbackOutcome,
    },
    environment_lighting_test_scene::EnvironmentLightingTestScene,
    hybrid_transparency_test_scene::HybridTransparencyTestScene,
    screenshot::ScreenshotRuntime,
    terrain_connectivity::bench::TerrainConnectivityBench,
    tree_bench::TreeBench,
    water::{self, WaterEditSoak},
    water_experience_scene::WaterExperienceScene,
};
pub(super) use super::{
    canopy_audio_diagnostic::{
        AudioTelemetryMarker, CanopyAudioAcousticBudget, CanopyAudioFrameCommand,
        CanopyAudioFrameEffect, CanopyAudioFrameReceipt, CanopyAudioStartObservation,
        CanopyAudioStartupPlan, CanopyAudioVegetationStartup, CanopyAudioWindPolicy,
    },
    water::{WaterEditFrameResult, WaterEditFrameTxn},
    water_experience_scene::{
        WaterExperienceFrameResult, WaterExperienceFrameTxn, WaterExperienceReadyReceipt,
    },
};
#[cfg(test)]
use crate::audio::CanopyAudioTelemetrySnapshot;
use crate::cli::{AutomationPlan, CameraAutomation, Scenario};

pub(super) enum CameraOwner {
    None,
    Snapshot {
        name: String,
    },
    DenoiserBenchmark {
        snapshot: String,
        runtime: DenoiserBench,
    },
}

impl CameraOwner {
    pub(super) fn snapshot_name(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Snapshot { name } => Some(name),
            Self::DenoiserBenchmark { snapshot, .. } => Some(snapshot),
        }
    }
}

pub(super) struct HouseSceneOwner;
pub(super) enum WorldScenarioOwner {
    Garden,
    House(HouseSceneOwner),
}

pub(super) enum WaterScenarioOwner {
    Experience(WaterExperienceScene),
    EditSoak(WaterEditSoak),
}

pub(super) enum TestSceneOwner {
    Hybrid(HybridTransparencyTestScene),
}

pub(super) enum StandardScenarioOwner {
    World(WorldScenarioOwner),
    Water(WaterScenarioOwner),
    TestScene(TestSceneOwner),
}

pub(super) enum ScenarioOwner {
    Standard(StandardScenarioOwner),
    Connectivity(TerrainConnectivityBench),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoadingDirective {
    Garden,
    WaterExperience,
    House,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TestSceneKind {
    None,
    Environment(crate::cli::EnvironmentLightingTestCase),
    Hybrid,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TestSceneFramePlan {
    kind: TestSceneKind,
    capture_ready: bool,
    hides_terrain_edit_preview: bool,
}

impl TestSceneFramePlan {
    fn none() -> Self {
        Self {
            kind: TestSceneKind::None,
            capture_ready: true,
            hides_terrain_edit_preview: false,
        }
    }

    pub(super) fn kind(self) -> TestSceneKind {
        self.kind
    }

    pub(super) fn owns_capture_scene(self) -> bool {
        self.kind != TestSceneKind::None
    }

    pub(super) fn capture_is_ready(self) -> bool {
        self.capture_ready
    }

    pub(super) fn hides_terrain_edit_preview(self) -> bool {
        self.hides_terrain_edit_preview
    }
}

impl ScenarioOwner {
    pub(super) fn loading_directive(&self) -> LoadingDirective {
        match self {
            Self::Standard(StandardScenarioOwner::World(WorldScenarioOwner::Garden)) => {
                LoadingDirective::Garden
            }
            Self::Standard(StandardScenarioOwner::World(WorldScenarioOwner::House(_))) => {
                LoadingDirective::House
            }
            Self::Standard(StandardScenarioOwner::Water(WaterScenarioOwner::Experience(_))) => {
                LoadingDirective::WaterExperience
            }
            Self::Standard(
                StandardScenarioOwner::Water(WaterScenarioOwner::EditSoak(_))
                | StandardScenarioOwner::TestScene(_),
            )
            | Self::Connectivity(_) => LoadingDirective::Garden,
        }
    }

    pub(super) fn test_scene_frame_plan(&self) -> TestSceneFramePlan {
        match self {
            Self::Standard(StandardScenarioOwner::TestScene(TestSceneOwner::Hybrid(scene))) => {
                TestSceneFramePlan {
                    kind: TestSceneKind::Hybrid,
                    capture_ready: scene.is_ready(),
                    hides_terrain_edit_preview: false,
                }
            }
            Self::Standard(StandardScenarioOwner::World(_) | StandardScenarioOwner::Water(_))
            | Self::Connectivity(_) => TestSceneFramePlan::none(),
        }
    }
}

pub(in crate::app) struct LaunchOwners {
    screenshot: ScreenshotRuntime,
    pub(super) tree_bench: Option<TreeBench>,
    pub(super) authored_flora_bench: Option<AuthoredFloraBench>,
    pub(super) mode: LaunchMode,
}

pub(super) enum LaunchMode {
    General {
        camera: CameraOwner,
        scenario: ScenarioOwner,
    },
    Environment {
        camera: CameraOwner,
        owner: EnvironmentLightingTestScene,
    },
    CanopyAudio {
        camera: CameraOwner,
        startup: Option<CanopyAudioStartupPlan>,
        owner: CanopyAudioDiagnosticRuntime,
    },
    FoliageShadow {
        runtime: DenoiserBench,
    },
}

impl LaunchOwners {
    pub(super) fn snapshot_name(&self) -> Option<&str> {
        match &self.mode {
            LaunchMode::General { camera, .. }
            | LaunchMode::Environment { camera, .. }
            | LaunchMode::CanopyAudio { camera, .. } => camera.snapshot_name(),
            LaunchMode::FoliageShadow { .. } => None,
        }
    }

    pub(super) fn screenshot(&self) -> &ScreenshotRuntime {
        &self.screenshot
    }

    pub(super) fn screenshot_mut(&mut self) -> &mut ScreenshotRuntime {
        &mut self.screenshot
    }

    pub(super) fn loading_directive(&self) -> LoadingDirective {
        match &self.mode {
            LaunchMode::General { scenario, .. } => scenario.loading_directive(),
            LaunchMode::Environment { .. }
            | LaunchMode::CanopyAudio { .. }
            | LaunchMode::FoliageShadow { .. } => LoadingDirective::Garden,
        }
    }

    pub(super) fn test_scene_frame_plan(&self) -> TestSceneFramePlan {
        match &self.mode {
            LaunchMode::General { scenario, .. } => scenario.test_scene_frame_plan(),
            LaunchMode::Environment { owner, .. } => TestSceneFramePlan {
                kind: TestSceneKind::Environment(owner.case()),
                capture_ready: owner.is_ready(),
                hides_terrain_edit_preview: owner.hides_terrain_edit_preview(),
            },
            LaunchMode::CanopyAudio { .. } | LaunchMode::FoliageShadow { .. } => {
                TestSceneFramePlan::none()
            }
        }
    }

    pub(super) fn activate_water_experience(&mut self, expected_particle_count: usize) {
        match &mut self.mode {
            LaunchMode::General {
                scenario:
                    ScenarioOwner::Standard(StandardScenarioOwner::Water(
                        WaterScenarioOwner::Experience(scene),
                    )),
                ..
            } => scene.activate(expected_particle_count),
            LaunchMode::General { .. }
            | LaunchMode::Environment { .. }
            | LaunchMode::CanopyAudio { .. }
            | LaunchMode::FoliageShadow { .. } => {
                panic!("only a water-experience launch can be activated")
            }
        }
    }

    pub(super) fn begin_water_experience_frame(&self) -> WaterExperienceFrameTxn {
        match &self.mode {
            LaunchMode::General {
                scenario:
                    ScenarioOwner::Standard(StandardScenarioOwner::Water(
                        WaterScenarioOwner::Experience(scene),
                    )),
                ..
            } => scene.begin_frame(),
            LaunchMode::General { .. }
            | LaunchMode::Environment { .. }
            | LaunchMode::CanopyAudio { .. }
            | LaunchMode::FoliageShadow { .. } => WaterExperienceFrameTxn::Inactive,
        }
    }

    pub(super) fn finish_water_experience_frame(
        &mut self,
        transaction: WaterExperienceFrameTxn,
        result: WaterExperienceFrameResult,
    ) -> anyhow::Result<Option<WaterExperienceReadyReceipt>> {
        match (&mut self.mode, transaction) {
            (
                LaunchMode::General {
                    scenario:
                        ScenarioOwner::Standard(StandardScenarioOwner::Water(
                            WaterScenarioOwner::Experience(scene),
                        )),
                    ..
                },
                transaction @ (WaterExperienceFrameTxn::PendingActivation
                | WaterExperienceFrameTxn::Waiting { .. }
                | WaterExperienceFrameTxn::Ready),
            ) => scene.finish_frame(transaction, result),
            (
                LaunchMode::General { .. }
                | LaunchMode::Environment { .. }
                | LaunchMode::CanopyAudio { .. }
                | LaunchMode::FoliageShadow { .. },
                WaterExperienceFrameTxn::Inactive,
            ) => {
                anyhow::ensure!(
                    matches!(result, WaterExperienceFrameResult::NotReady),
                    "inactive water-experience owner received a ready result"
                );
                Ok(None)
            }
            _ => anyhow::bail!("water-experience transaction changed launch owner"),
        }
    }

    pub(super) fn begin_water_edit_frame(&self) -> WaterEditFrameTxn {
        match &self.mode {
            LaunchMode::General {
                scenario:
                    ScenarioOwner::Standard(StandardScenarioOwner::Water(WaterScenarioOwner::EditSoak(
                        owner,
                    ))),
                ..
            } => owner.begin_frame(),
            LaunchMode::General { .. }
            | LaunchMode::Environment { .. }
            | LaunchMode::CanopyAudio { .. }
            | LaunchMode::FoliageShadow { .. } => WaterEditFrameTxn::Inactive,
        }
    }

    pub(super) fn finish_water_edit_frame(
        &mut self,
        transaction: WaterEditFrameTxn,
        result: WaterEditFrameResult,
    ) -> anyhow::Result<bool> {
        match (&mut self.mode, transaction) {
            (
                LaunchMode::General {
                    scenario:
                        ScenarioOwner::Standard(StandardScenarioOwner::Water(
                            WaterScenarioOwner::EditSoak(owner),
                        )),
                    ..
                },
                transaction @ (WaterEditFrameTxn::Step { .. } | WaterEditFrameTxn::Complete),
            ) => owner.finish_frame(transaction, result),
            (
                LaunchMode::General { .. }
                | LaunchMode::Environment { .. }
                | LaunchMode::CanopyAudio { .. }
                | LaunchMode::FoliageShadow { .. },
                WaterEditFrameTxn::Inactive,
            ) => {
                anyhow::ensure!(
                    result == WaterEditFrameResult::Failed,
                    "inactive water-edit owner received an applied result"
                );
                Ok(false)
            }
            _ => anyhow::bail!("water-edit transaction changed launch owner"),
        }
    }

    pub(super) fn take_canopy_audio_startup_plan(
        &mut self,
    ) -> anyhow::Result<Option<CanopyAudioStartupPlan>> {
        match &mut self.mode {
            LaunchMode::CanopyAudio { startup, .. } => startup
                .take()
                .map(Some)
                .ok_or_else(|| anyhow::anyhow!("canopy audio startup plan was already consumed")),
            LaunchMode::General { .. }
            | LaunchMode::Environment { .. }
            | LaunchMode::FoliageShadow { .. } => Ok(None),
        }
    }

    pub(super) fn begin_canopy_audio_frame(
        &self,
        tree_origin_world: glam::Vec3,
        time_seconds: f32,
    ) -> CanopyAudioFrameCommand {
        match &self.mode {
            LaunchMode::CanopyAudio { owner, .. } => {
                owner.begin_frame(tree_origin_world, time_seconds)
            }
            LaunchMode::General { .. }
            | LaunchMode::Environment { .. }
            | LaunchMode::FoliageShadow { .. } => CanopyAudioFrameCommand::Standard,
        }
    }

    pub(super) fn finish_canopy_audio_frame(
        &mut self,
        command: CanopyAudioFrameCommand,
        effect: CanopyAudioFrameEffect,
    ) -> anyhow::Result<CanopyAudioFrameReceipt> {
        match (&mut self.mode, command) {
            (
                LaunchMode::CanopyAudio { owner, .. },
                command @ CanopyAudioFrameCommand::Diagnostic { .. },
            ) => owner.finish_frame(command, effect),
            (
                LaunchMode::General { .. }
                | LaunchMode::Environment { .. }
                | LaunchMode::FoliageShadow { .. },
                CanopyAudioFrameCommand::Standard,
            ) => Ok(CanopyAudioFrameReceipt::standard(effect)),
            _ => anyhow::bail!("canopy audio frame command changed launch owner"),
        }
    }

    pub(super) fn is_foliage_shadow(&self) -> bool {
        matches!(self.mode, LaunchMode::FoliageShadow { .. })
    }

    pub(super) fn begin_denoiser_frame(&self) -> DenoiserFrameCommand {
        match &self.mode {
            LaunchMode::General {
                camera: CameraOwner::DenoiserBenchmark { runtime, .. },
                ..
            }
            | LaunchMode::Environment {
                camera: CameraOwner::DenoiserBenchmark { runtime, .. },
                ..
            }
            | LaunchMode::CanopyAudio {
                camera: CameraOwner::DenoiserBenchmark { runtime, .. },
                ..
            } => DenoiserFrameCommand::Camera(runtime.begin_camera_frame()),
            LaunchMode::FoliageShadow { runtime } => {
                DenoiserFrameCommand::Foliage(runtime.begin_foliage_frame())
            }
            LaunchMode::General {
                camera: CameraOwner::None | CameraOwner::Snapshot { .. },
                ..
            }
            | LaunchMode::Environment {
                camera: CameraOwner::None | CameraOwner::Snapshot { .. },
                ..
            }
            | LaunchMode::CanopyAudio {
                camera: CameraOwner::None | CameraOwner::Snapshot { .. },
                ..
            } => DenoiserFrameCommand::Inactive,
        }
    }

    pub(super) fn finish_denoiser_frame(
        &mut self,
        completion: DenoiserFrameCompletion,
    ) -> anyhow::Result<bool> {
        match (&mut self.mode, completion) {
            (
                LaunchMode::General {
                    camera: CameraOwner::DenoiserBenchmark { runtime, .. },
                    ..
                },
                completion @ DenoiserFrameCompletion::Camera { .. },
            )
            | (
                LaunchMode::Environment {
                    camera: CameraOwner::DenoiserBenchmark { runtime, .. },
                    ..
                },
                completion @ DenoiserFrameCompletion::Camera { .. },
            )
            | (
                LaunchMode::CanopyAudio {
                    camera: CameraOwner::DenoiserBenchmark { runtime, .. },
                    ..
                },
                completion @ DenoiserFrameCompletion::Camera { .. },
            ) => runtime.finish_camera_frame(completion),
            (
                LaunchMode::FoliageShadow { runtime },
                completion @ DenoiserFrameCompletion::Foliage { .. },
            ) => runtime.finish_foliage_frame(completion),
            (
                LaunchMode::General {
                    camera: CameraOwner::None | CameraOwner::Snapshot { .. },
                    ..
                },
                DenoiserFrameCompletion::Inactive(readback),
            ) => match readback {
                DenoiserReadbackOutcome::NotRequested => Ok(false),
                DenoiserReadbackOutcome::Failed(error) => Err(error),
                DenoiserReadbackOutcome::Frame(_) => {
                    anyhow::bail!("inactive denoiser owner received a frame")
                }
            },
            (
                LaunchMode::Environment {
                    camera: CameraOwner::None | CameraOwner::Snapshot { .. },
                    ..
                }
                | LaunchMode::CanopyAudio {
                    camera: CameraOwner::None | CameraOwner::Snapshot { .. },
                    ..
                },
                DenoiserFrameCompletion::Inactive(readback),
            ) => match readback {
                DenoiserReadbackOutcome::NotRequested => Ok(false),
                DenoiserReadbackOutcome::Failed(error) => Err(error),
                DenoiserReadbackOutcome::Frame(_) => {
                    anyhow::bail!("inactive denoiser owner received a frame")
                }
            },
            _ => anyhow::bail!("denoiser frame transaction changed launch owner"),
        }
    }
}

pub(in crate::app) fn prepare_startup_owners(
    automation: AutomationPlan,
    scenario: Scenario,
) -> Result<LaunchOwners, String> {
    let AutomationPlan { camera, benchmarks } = automation;
    let tree_bench = benchmarks.tree_samples.map(TreeBench::new);
    let authored_flora_bench = benchmarks
        .authored_flora_samples
        .map(AuthoredFloraBench::new);
    if let Scenario::FoliageShadowBenchmark(options) = scenario {
        let CameraAutomation::None = camera else {
            return Err(
                "foliage-shadow scenario cannot carry a second camera automation".to_owned(),
            );
        };
        return Ok(LaunchOwners {
            screenshot: ScreenshotRuntime::new(None),
            tree_bench,
            authored_flora_bench,
            mode: LaunchMode::FoliageShadow {
                runtime: DenoiserBench::new_foliage(options),
            },
        });
    }
    let (camera, screenshot) = match camera {
        CameraAutomation::None => (CameraOwner::None, ScreenshotRuntime::new(None)),
        CameraAutomation::Snapshot(name) => {
            (CameraOwner::Snapshot { name }, ScreenshotRuntime::new(None))
        }
        CameraAutomation::Screenshot { snapshot, capture } => (
            CameraOwner::Snapshot { name: snapshot },
            ScreenshotRuntime::new(Some(capture)),
        ),
        CameraAutomation::DenoiserBenchmark {
            snapshot,
            benchmark,
        } => (
            CameraOwner::DenoiserBenchmark {
                snapshot,
                runtime: DenoiserBench::new_camera(benchmark),
            },
            ScreenshotRuntime::new(None),
        ),
    };
    let mode = match scenario {
        Scenario::Garden => LaunchMode::General {
            camera,
            scenario: ScenarioOwner::Standard(StandardScenarioOwner::World(
                WorldScenarioOwner::Garden,
            )),
        },
        Scenario::CanopyAudioDiagnostic { constrained_budget } => LaunchMode::CanopyAudio {
            camera,
            startup: Some(CanopyAudioStartupPlan::diagnostic(constrained_budget)),
            owner: CanopyAudioDiagnosticRuntime::new(),
        },
        Scenario::WaterExperience => LaunchMode::General {
            camera,
            scenario: ScenarioOwner::Standard(StandardScenarioOwner::Water(
                WaterScenarioOwner::Experience(WaterExperienceScene::pending()),
            )),
        },
        Scenario::WaterEditSoak => LaunchMode::General {
            camera,
            scenario: ScenarioOwner::Standard(StandardScenarioOwner::Water(
                WaterScenarioOwner::EditSoak(water::WaterEditSoak::default()),
            )),
        },
        Scenario::EnvironmentLighting(case) => LaunchMode::Environment {
            camera,
            owner: EnvironmentLightingTestScene::new(case),
        },
        Scenario::HybridTransparency => LaunchMode::General {
            camera,
            scenario: ScenarioOwner::Standard(StandardScenarioOwner::TestScene(
                TestSceneOwner::Hybrid(HybridTransparencyTestScene::new()),
            )),
        },
        Scenario::House => LaunchMode::General {
            camera,
            scenario: ScenarioOwner::Standard(StandardScenarioOwner::World(
                WorldScenarioOwner::House(HouseSceneOwner),
            )),
        },
        Scenario::TerrainConnectivityBenchmark(options) => LaunchMode::General {
            camera,
            scenario: ScenarioOwner::Connectivity(TerrainConnectivityBench::new(options)),
        },
        Scenario::FoliageShadowBenchmark(_) => unreachable!("foliage handled before camera setup"),
    };
    Ok(LaunchOwners {
        screenshot,
        tree_bench,
        authored_flora_bench,
        mode,
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
    }

    fn owner_kind(launch: LaunchOwners) -> OwnerKind {
        match launch.mode {
            LaunchMode::General { scenario, .. } => match scenario {
                ScenarioOwner::Standard(StandardScenarioOwner::World(world)) => match world {
                    WorldScenarioOwner::Garden => OwnerKind::Garden,
                    WorldScenarioOwner::House(_) => OwnerKind::House,
                },
                ScenarioOwner::Standard(StandardScenarioOwner::Water(water)) => match water {
                    WaterScenarioOwner::Experience(_) => OwnerKind::WaterExperience,
                    WaterScenarioOwner::EditSoak(_) => OwnerKind::WaterEditSoak,
                },
                ScenarioOwner::Standard(StandardScenarioOwner::TestScene(
                    TestSceneOwner::Hybrid(_),
                )) => OwnerKind::Hybrid,
                ScenarioOwner::Connectivity(_) => OwnerKind::TerrainConnectivity,
            },
            LaunchMode::Environment { .. } => OwnerKind::Environment,
            LaunchMode::CanopyAudio { .. } => OwnerKind::CanopyAudio,
            LaunchMode::FoliageShadow { .. } => {
                panic!("foliage benchmark is not a fixed scene owner")
            }
        }
    }

    fn launch_for(scenario: Scenario) -> LaunchOwners {
        prepare_startup_owners(AutomationPlan::default(), scenario).unwrap()
    }

    #[test]
    fn protocol_heavy_scenarios_are_promoted_to_top_level_launch_modes() {
        let environment = launch_for(Scenario::EnvironmentLighting(
            crate::cli::EnvironmentLightingTestCase::Sealed,
        ));
        assert!(matches!(
            environment.mode,
            LaunchMode::Environment {
                camera: CameraOwner::None,
                ..
            }
        ));

        let canopy = launch_for(Scenario::CanopyAudioDiagnostic {
            constrained_budget: true,
        });
        assert!(matches!(
            canopy.mode,
            LaunchMode::CanopyAudio {
                camera: CameraOwner::None,
                ..
            }
        ));

        let garden = launch_for(Scenario::Garden);
        assert!(matches!(garden.mode, LaunchMode::General { .. }));
    }

    fn capture_for(camera: CameraAutomation) -> LaunchOwners {
        prepare_startup_owners(
            AutomationPlan {
                camera,
                benchmarks: BenchmarkPlan::default(),
            },
            Scenario::Garden,
        )
        .unwrap()
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
            let launch = capture_for(camera);
            assert_eq!(launch.screenshot.is_scheduled(), expected_scheduled);
            let LaunchMode::General { camera, .. } = launch.mode else {
                panic!("garden must use standard launch ownership")
            };
            match camera {
                CameraOwner::None => {
                    assert_eq!(expected_snapshot, None);
                    assert!(!expected_denoiser);
                }
                CameraOwner::Snapshot { name } => {
                    assert_eq!(Some(name.as_str()), expected_snapshot);
                    assert!(!expected_denoiser);
                }
                CameraOwner::DenoiserBenchmark { snapshot, .. } => {
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
            LaunchMode::General { .. }
            | LaunchMode::Environment { .. }
            | LaunchMode::CanopyAudio { .. } => {
                panic!("foliage benchmark must not be a standard scenario/camera pair")
            }
            LaunchMode::FoliageShadow { runtime } => {
                assert!(!launch.screenshot.is_scheduled());
                assert!(runtime.fixed_frame_delta_seconds().is_some());
            }
        }
    }

    #[test]
    fn foliage_denoiser_exposes_an_owned_plan_without_borrowing_the_runtime() {
        let launch = prepare_startup_owners(
            AutomationPlan::default(),
            Scenario::FoliageShadowBenchmark(FoliageDenoiserOptions {
                capture: capture_options(),
            }),
        )
        .unwrap();

        assert!(matches!(
            launch.begin_denoiser_frame(),
            DenoiserFrameCommand::Foliage(_)
        ));
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
        ];

        for (scenario, expected) in cases {
            assert_eq!(owner_kind(launch_for(scenario)), expected);
        }
        assert_eq!(owner_kind(launch_for(Scenario::Garden)), OwnerKind::Garden);
    }

    #[test]
    fn non_diagnostic_scenarios_never_request_the_canopy_camera_pose() {
        let mut garden = launch_for(Scenario::Garden);
        assert!(garden.take_canopy_audio_startup_plan().unwrap().is_none());
        let command = garden.begin_canopy_audio_frame(glam::Vec3::ZERO, 1.0);
        assert!(matches!(command, CanopyAudioFrameCommand::Standard));
        assert_eq!(command.wind_policy(), CanopyAudioWindPolicy::Configured);
        let mut snapshot = CanopyAudioTelemetrySnapshot::default();
        snapshot.petal_direct_ray_count = 9;
        let receipt = garden
            .finish_canopy_audio_frame(
                command,
                CanopyAudioFrameEffect::Applied {
                    start_observation: None,
                    telemetry_counters: Some(CanopyAudioDiagnosticCounters::from_snapshot(
                        &snapshot,
                    )),
                },
            )
            .unwrap();
        assert!(matches!(
            receipt.telemetry().unwrap().marker,
            AudioTelemetryMarker::NotDiagnostic
        ));
        assert_eq!(receipt.telemetry().unwrap().counters.direct_rays, 9);

        let canopy = launch_for(Scenario::CanopyAudioDiagnostic {
            constrained_budget: false,
        });
        assert!(matches!(
            canopy.begin_canopy_audio_frame(glam::Vec3::ZERO, 1.0),
            CanopyAudioFrameCommand::Diagnostic { .. }
        ));
    }

    #[test]
    fn water_phase_is_an_owned_closed_transaction() {
        let experience = launch_for(Scenario::WaterExperience);
        assert!(matches!(
            experience.begin_water_experience_frame(),
            WaterExperienceFrameTxn::PendingActivation
        ));

        let edit_soak = launch_for(Scenario::WaterEditSoak);
        assert!(matches!(
            edit_soak.begin_water_edit_frame(),
            WaterEditFrameTxn::Step { step: 0 }
        ));

        let garden = launch_for(Scenario::Garden);
        assert!(matches!(
            garden.begin_water_experience_frame(),
            WaterExperienceFrameTxn::Inactive
        ));
        assert!(matches!(
            garden.begin_water_edit_frame(),
            WaterEditFrameTxn::Inactive
        ));
    }

    #[test]
    fn test_scene_frame_plan_contains_no_borrowed_runtime() {
        let environment = launch_for(Scenario::EnvironmentLighting(
            crate::cli::EnvironmentLightingTestCase::Sealed,
        ));
        assert_eq!(
            environment.test_scene_frame_plan().kind(),
            TestSceneKind::Environment(crate::cli::EnvironmentLightingTestCase::Sealed)
        );

        let hybrid = launch_for(Scenario::HybridTransparency);
        assert_eq!(hybrid.test_scene_frame_plan().kind(), TestSceneKind::Hybrid);

        let garden = launch_for(Scenario::Garden);
        assert_eq!(garden.test_scene_frame_plan().kind(), TestSceneKind::None);
    }

    #[test]
    fn terrain_connectivity_scenario_uses_the_exclusive_connectivity_family() {
        let owner = launch_for(Scenario::TerrainConnectivityBenchmark(
            crate::cli::TerrainConnectivityBenchOptions {
                mode: crate::cli::TerrainConnectivityBenchMode::Correct,
                available_particles: 8,
                warmup_frames: 1,
                observe_frames: 1,
                voxel_budget: 8,
            },
        ));

        assert!(matches!(
            owner.mode,
            LaunchMode::General {
                scenario: ScenarioOwner::Connectivity(_),
                ..
            }
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

    fn denoiser_capture_options() -> DenoiserCaptureOptions {
        DenoiserCaptureOptions {
            report_path: "unused-denoiser-report.toml".to_owned(),
            warmup_frames: 0,
            capture_frames: 2,
        }
    }

    fn camera_denoiser_owners() -> LaunchOwners {
        prepare_startup_owners(
            AutomationPlan {
                camera: CameraAutomation::DenoiserBenchmark {
                    snapshot: "tree".to_owned(),
                    benchmark: CameraDenoiserOptions {
                        capture: denoiser_capture_options(),
                        camera_motion: CameraMotion::Scripted,
                    },
                },
                benchmarks: BenchmarkPlan::default(),
            },
            Scenario::Garden,
        )
        .unwrap()
    }

    fn foliage_denoiser_owners() -> LaunchOwners {
        prepare_startup_owners(
            AutomationPlan::default(),
            Scenario::FoliageShadowBenchmark(FoliageDenoiserOptions {
                capture: denoiser_capture_options(),
            }),
        )
        .unwrap()
    }

    #[test]
    fn denoiser_frame_transaction_keeps_camera_and_foliage_ownership_exclusive() {
        let camera = camera_denoiser_owners();
        let DenoiserFrameCommand::Camera(camera_frame) = camera.begin_denoiser_frame() else {
            panic!("camera owner must mint its leaf command")
        };
        assert!(matches!(
            camera_frame.presentation(),
            CameraDenoiserPresentation::Scripted {
                capture: DenoiserCaptureStep::Record { frame: 0 },
                capture_frame: 0,
                is_last: false,
            }
        ));

        let foliage = foliage_denoiser_owners();
        let DenoiserFrameCommand::Foliage(foliage_frame) = foliage.begin_denoiser_frame() else {
            panic!("foliage owner must mint its leaf command")
        };
        assert!(matches!(
            foliage_frame.presentation(),
            FoliageDenoiserPresentation {
                capture: DenoiserCaptureStep::Record { frame: 0 },
                timeline: FixedVisualFrame {
                    frame_delta_seconds,
                    visual_time_seconds: 0.0,
                },
            } if frame_delta_seconds > 0.0
        ));
    }

    #[test]
    fn denoiser_leaf_commands_mint_one_affine_run_through_readback_and_commit() {
        static_assertions::assert_not_impl_any!(CameraDenoiserCommand: Clone, Copy);
        static_assertions::assert_not_impl_any!(FoliageDenoiserCommand: Clone, Copy);
        static_assertions::assert_not_impl_any!(DenoiserFrameRun: Clone, Copy);

        let mut camera = camera_denoiser_owners();
        let DenoiserFrameCommand::Camera(command) = camera.begin_denoiser_frame() else {
            panic!("camera launch must mint a camera-owned command")
        };
        assert!(matches!(
            command.presentation(),
            CameraDenoiserPresentation::Scripted {
                capture: DenoiserCaptureStep::Record { frame: 0 },
                capture_frame: 0,
                is_last: false,
            }
        ));
        let completion =
            command
                .into_run()
                .complete(DenoiserReadbackOutcome::Failed(anyhow::anyhow!(
                    "injected readback failure"
                )));
        camera
            .finish_denoiser_frame(completion)
            .expect_err("failed readback must not commit presentation");

        let DenoiserFrameCommand::Camera(command) = camera.begin_denoiser_frame() else {
            unreachable!()
        };
        assert!(matches!(
            command.presentation(),
            CameraDenoiserPresentation::Scripted {
                capture: DenoiserCaptureStep::Record { frame: 0 },
                capture_frame: 0,
                ..
            }
        ));

        let foliage = foliage_denoiser_owners();
        let DenoiserFrameCommand::Foliage(command) = foliage.begin_denoiser_frame() else {
            panic!("foliage launch must mint a foliage-owned command")
        };
        assert!(matches!(
            command.presentation(),
            FoliageDenoiserPresentation {
                capture: DenoiserCaptureStep::Record { frame: 0 },
                timeline: FixedVisualFrame {
                    visual_time_seconds: 0.0,
                    ..
                },
            }
        ));
    }

    #[test]
    fn opaque_denoiser_run_retains_each_leafs_frame_steps() {
        let camera = camera_denoiser_owners().begin_denoiser_frame().into_run();
        assert_eq!(
            camera.timeline(0.25, 2.0),
            FixedVisualFrame {
                frame_delta_seconds: 0.25,
                visual_time_seconds: 2.0,
            }
        );
        assert!(matches!(
            camera.camera_step(),
            CameraFrameMotion::Scripted {
                capture_frame: 0,
                is_last: false,
            }
        ));
        assert_eq!(camera.ui_step(), DenoiserUiStep::CameraCapture);
        assert_eq!(camera.readback_step(), DenoiserReadbackStep::Record);

        let foliage = foliage_denoiser_owners().begin_denoiser_frame().into_run();
        assert_eq!(
            foliage.timeline(0.25, 2.0),
            FixedVisualFrame {
                frame_delta_seconds: 1.0 / 60.0,
                visual_time_seconds: 0.0,
            }
        );
        assert_eq!(foliage.camera_step(), CameraFrameMotion::Fixed);
        assert_eq!(foliage.ui_step(), DenoiserUiStep::FoliageStability);
        assert_eq!(foliage.readback_step(), DenoiserReadbackStep::Record);

        let inactive = launch_for(Scenario::Garden)
            .begin_denoiser_frame()
            .into_run();
        assert_eq!(
            inactive.timeline(0.25, 2.0),
            FixedVisualFrame {
                frame_delta_seconds: 0.25,
                visual_time_seconds: 2.0,
            }
        );
        assert_eq!(inactive.camera_step(), CameraFrameMotion::Fixed);
        assert_eq!(inactive.ui_step(), DenoiserUiStep::Inactive);
        assert_eq!(inactive.readback_step(), DenoiserReadbackStep::Skip);
    }

    #[test]
    fn failed_denoiser_record_does_not_present_or_advance_the_owned_transaction() {
        for mut owners in [camera_denoiser_owners(), foliage_denoiser_owners()] {
            let readback_transaction = owners.begin_denoiser_frame();
            owners
                .finish_denoiser_frame(readback_transaction.into_run().complete(
                    DenoiserReadbackOutcome::Failed(anyhow::anyhow!("synthetic readback failure")),
                ))
                .expect_err("readback failure must reject the transaction");

            let transaction = owners.begin_denoiser_frame();
            let error = owners
                .finish_denoiser_frame(transaction.into_run().complete(
                    DenoiserReadbackOutcome::Frame(DenoiserFrame::new(2, 2, vec![0; 3])),
                ))
                .expect_err("invalid frame bytes must reject the transaction");
            assert!(error.to_string().contains("expected"));

            match owners.begin_denoiser_frame() {
                DenoiserFrameCommand::Camera(command) => assert!(matches!(
                    command.presentation(),
                    CameraDenoiserPresentation::Fixed {
                        capture: DenoiserCaptureStep::Record { frame: 0 },
                    } | CameraDenoiserPresentation::Scripted {
                        capture: DenoiserCaptureStep::Record { frame: 0 },
                        ..
                    }
                )),
                DenoiserFrameCommand::Foliage(command) => assert!(matches!(
                    command.presentation(),
                    FoliageDenoiserPresentation {
                        capture: DenoiserCaptureStep::Record { frame: 0 },
                        timeline: FixedVisualFrame {
                            visual_time_seconds: 0.0,
                            ..
                        },
                    }
                )),
                DenoiserFrameCommand::Inactive => panic!("benchmark owner became inactive"),
            }
        }
    }

    #[test]
    fn failed_denoiser_report_write_does_not_commit_capture_or_present_counters() {
        let report_directory = tempfile::tempdir().unwrap();
        let mut owners = prepare_startup_owners(
            AutomationPlan {
                camera: CameraAutomation::DenoiserBenchmark {
                    snapshot: "tree".to_owned(),
                    benchmark: CameraDenoiserOptions {
                        capture: DenoiserCaptureOptions {
                            report_path: report_directory.path().display().to_string(),
                            warmup_frames: 0,
                            capture_frames: 1,
                        },
                        camera_motion: CameraMotion::Fixed,
                    },
                },
                benchmarks: BenchmarkPlan::default(),
            },
            Scenario::Garden,
        )
        .unwrap();

        let transaction = owners.begin_denoiser_frame();
        let error =
            owners
                .finish_denoiser_frame(transaction.into_run().complete(
                    DenoiserReadbackOutcome::Frame(DenoiserFrame::new(1, 1, vec![0; 4])),
                ))
                .expect_err("writing a report over an existing directory must fail");
        assert!(error.to_string().contains("publish denoiser report"));
        let DenoiserFrameCommand::Camera(command) = owners.begin_denoiser_frame() else {
            panic!("camera benchmark owner became inactive")
        };
        assert!(matches!(
            command.presentation(),
            CameraDenoiserPresentation::Fixed {
                capture: DenoiserCaptureStep::Record { frame: 0 },
            }
        ));
    }

    #[test]
    fn water_phase_transactions_commit_only_successful_runtime_receipts() {
        let mut experience = launch_for(Scenario::WaterExperience);
        experience.activate_water_experience(10_000);
        let ready = experience.begin_water_experience_frame();
        assert!(matches!(
            ready,
            WaterExperienceFrameTxn::Waiting {
                expected_particle_count: 10_000,
                ..
            }
        ));
        experience
            .finish_water_experience_frame(
                ready,
                WaterExperienceFrameResult::Ready {
                    particle_count: 10_000,
                    sim_time_seconds: 1.0 / 60.0,
                    revision: 7,
                },
            )
            .unwrap();
        assert!(matches!(
            experience.begin_water_experience_frame(),
            WaterExperienceFrameTxn::Ready
        ));

        let mut edit = launch_for(Scenario::WaterEditSoak);
        let first = edit.begin_water_edit_frame();
        assert!(matches!(first, WaterEditFrameTxn::Step { step: 0, .. }));
        edit.finish_water_edit_frame(first, WaterEditFrameResult::Failed)
            .unwrap();
        assert!(matches!(
            edit.begin_water_edit_frame(),
            WaterEditFrameTxn::Step { step: 0, .. }
        ));
        let retry = edit.begin_water_edit_frame();
        edit.finish_water_edit_frame(retry, WaterEditFrameResult::Applied)
            .unwrap();
        assert!(matches!(
            edit.begin_water_edit_frame(),
            WaterEditFrameTxn::Step { step: 1, .. }
        ));
    }

    #[test]
    fn canopy_startup_policy_and_frame_effect_are_each_consumed_once() {
        static_assertions::assert_not_impl_any!(CanopyAudioStartupPlan: Clone, Copy);
        static_assertions::assert_not_impl_any!(CanopyAudioVegetationStartup: Clone, Copy);
        static_assertions::assert_not_impl_any!(CanopyAudioFrameCommand: Clone, Copy);

        let mut owners = launch_for(Scenario::CanopyAudioDiagnostic {
            constrained_budget: true,
        });
        let startup = owners
            .take_canopy_audio_startup_plan()
            .unwrap()
            .expect("diagnostic launch must own its startup plan");
        let (budget, vegetation) = startup.into_effects();
        assert!(matches!(budget, CanopyAudioAcousticBudget::Constrained));
        assert!(vegetation.plants_budget_stress_trees());
        assert!(owners.take_canopy_audio_startup_plan().is_err());

        let frame = owners.begin_canopy_audio_frame(glam::Vec3::ZERO, 0.0);
        assert_eq!(frame.wind_policy(), CanopyAudioWindPolicy::Diagnostic);
        owners
            .finish_canopy_audio_frame(frame, CanopyAudioFrameEffect::Rejected)
            .unwrap();

        let mut snapshot = CanopyAudioTelemetrySnapshot::default();
        for time_seconds in [1.0, 1.05, 1.11] {
            let command = owners.begin_canopy_audio_frame(glam::Vec3::ZERO, time_seconds);
            assert!(command.pose().is_some());
            let receipt = owners
                .finish_canopy_audio_frame(
                    command,
                    CanopyAudioFrameEffect::Applied {
                        start_observation: Some(CanopyAudioStartObservation::new(
                            time_seconds,
                            true,
                            &snapshot,
                        )),
                        telemetry_counters: Some(CanopyAudioDiagnosticCounters::from_snapshot(
                            &snapshot,
                        )),
                    },
                )
                .unwrap();
            assert_eq!(receipt.started(), time_seconds == 1.11);
        }

        snapshot.petal_direct_ray_count = 7;
        let rejected = owners.begin_canopy_audio_frame(glam::Vec3::ZERO, 1.25);
        assert!(rejected.phase_log().is_some());
        owners
            .finish_canopy_audio_frame(rejected, CanopyAudioFrameEffect::Rejected)
            .unwrap();
        assert!(owners
            .begin_canopy_audio_frame(glam::Vec3::ZERO, 1.25)
            .phase_log()
            .is_some());

        let applied = owners.begin_canopy_audio_frame(glam::Vec3::ZERO, 1.25);
        let receipt = owners
            .finish_canopy_audio_frame(
                applied,
                CanopyAudioFrameEffect::Applied {
                    start_observation: None,
                    telemetry_counters: Some(CanopyAudioDiagnosticCounters::from_snapshot(
                        &snapshot,
                    )),
                },
            )
            .unwrap();
        assert!(matches!(
            receipt.telemetry().unwrap().marker,
            AudioTelemetryMarker::Active(_, _)
        ));
        assert_eq!(receipt.telemetry().unwrap().counters.direct_rays, 7);
        assert!(owners
            .begin_canopy_audio_frame(glam::Vec3::ZERO, 1.25)
            .phase_log()
            .is_none());
    }
}
