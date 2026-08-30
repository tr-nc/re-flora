use super::{
    denoiser_bench::DenoiserBench,
    environment_lighting_test_scene::EnvironmentLightingTestScene,
    hybrid_transparency_test_scene::HybridTransparencyTestScene,
    screenshot::ScreenshotRuntime,
    terrain_connectivity::bench::TerrainConnectivityBench,
    water::{self, WaterEditSoak},
    water_experience_scene::WaterExperienceScene,
    CanopyAudioDiagnosticRuntime,
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

    pub(super) fn denoiser(&self) -> Option<&DenoiserBench> {
        match self {
            Self::DenoiserBenchmark { runtime, .. } => Some(runtime),
            Self::None { .. } | Self::Snapshot { .. } | Self::Screenshot { .. } => None,
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

    pub(super) fn scene(&self) -> Option<&WaterExperienceScene> {
        match self {
            Self::Pending => None,
            Self::Active(scene) => Some(scene),
        }
    }

    pub(super) fn scene_mut(&mut self) -> Option<&mut WaterExperienceScene> {
        match self {
            Self::Pending => None,
            Self::Active(scene) => Some(scene),
        }
    }
}

pub(super) struct HouseSceneOwner;
pub(super) enum ScenarioOwner {
    Garden,
    CanopyAudioDiagnostic(CanopyAudioDiagnosticRuntime),
    WaterExperience(WaterExperienceOwner),
    WaterEditSoak(WaterEditSoak),
    EnvironmentLighting(EnvironmentLightingTestScene),
    HybridTransparency(HybridTransparencyTestScene),
    House(HouseSceneOwner),
    TerrainConnectivityBenchmark(TerrainConnectivityBench),
    FoliageShadowBenchmark(DenoiserBench),
}

impl ScenarioOwner {
    pub(super) fn is_water_experience(&self) -> bool {
        matches!(self, Self::WaterExperience(_))
    }

    pub(super) fn is_house(&self) -> bool {
        matches!(self, Self::House(_))
    }

    pub(super) fn canopy_audio(&self) -> Option<&CanopyAudioDiagnosticRuntime> {
        match self {
            Self::CanopyAudioDiagnostic(runtime) => Some(runtime),
            _ => None,
        }
    }

    pub(super) fn canopy_audio_mut(&mut self) -> Option<&mut CanopyAudioDiagnosticRuntime> {
        match self {
            Self::CanopyAudioDiagnostic(runtime) => Some(runtime),
            _ => None,
        }
    }

    pub(super) fn water_edit_soak(&self) -> Option<&WaterEditSoak> {
        match self {
            Self::WaterEditSoak(runtime) => Some(runtime),
            _ => None,
        }
    }

    pub(super) fn water_edit_soak_mut(&mut self) -> Option<&mut WaterEditSoak> {
        match self {
            Self::WaterEditSoak(runtime) => Some(runtime),
            _ => None,
        }
    }

    pub(super) fn water_experience(&self) -> Option<&WaterExperienceScene> {
        match self {
            Self::WaterExperience(owner) => owner.scene(),
            _ => None,
        }
    }

    pub(super) fn water_experience_mut(&mut self) -> Option<&mut WaterExperienceScene> {
        match self {
            Self::WaterExperience(owner) => owner.scene_mut(),
            _ => None,
        }
    }

    pub(super) fn activate_water_experience(&mut self, expected_particle_count: usize) {
        match self {
            Self::WaterExperience(owner) => owner.activate(expected_particle_count),
            _ => panic!("only a water-experience scenario can be activated"),
        }
    }

    pub(super) fn environment_lighting(&self) -> Option<&EnvironmentLightingTestScene> {
        match self {
            Self::EnvironmentLighting(scene) => Some(scene),
            _ => None,
        }
    }

    pub(super) fn environment_lighting_mut(&mut self) -> Option<&mut EnvironmentLightingTestScene> {
        match self {
            Self::EnvironmentLighting(scene) => Some(scene),
            _ => None,
        }
    }

    pub(super) fn hybrid_transparency(&self) -> Option<&HybridTransparencyTestScene> {
        match self {
            Self::HybridTransparency(scene) => Some(scene),
            _ => None,
        }
    }

    pub(super) fn hybrid_transparency_mut(&mut self) -> Option<&mut HybridTransparencyTestScene> {
        match self {
            Self::HybridTransparency(scene) => Some(scene),
            _ => None,
        }
    }

    pub(super) fn terrain_connectivity(&self) -> Option<&TerrainConnectivityBench> {
        match self {
            Self::TerrainConnectivityBenchmark(bench) => Some(bench),
            _ => None,
        }
    }

    pub(super) fn terrain_connectivity_mut(&mut self) -> Option<&mut TerrainConnectivityBench> {
        match self {
            Self::TerrainConnectivityBenchmark(bench) => Some(bench),
            _ => None,
        }
    }

    pub(super) fn foliage_shadow_benchmark(&self) -> Option<&DenoiserBench> {
        match self {
            Self::FoliageShadowBenchmark(bench) => Some(bench),
            _ => None,
        }
    }

    pub(super) fn foliage_shadow_benchmark_mut(&mut self) -> Option<&mut DenoiserBench> {
        match self {
            Self::FoliageShadowBenchmark(bench) => Some(bench),
            _ => None,
        }
    }

    pub(super) fn take_terrain_connectivity(&mut self) -> Option<TerrainConnectivityBench> {
        if matches!(self, Self::TerrainConnectivityBenchmark(_)) {
            match std::mem::replace(self, Self::Garden) {
                Self::TerrainConnectivityBenchmark(bench) => Some(bench),
                _ => unreachable!("matched terrain-connectivity scenario"),
            }
        } else {
            None
        }
    }

    pub(super) fn restore_terrain_connectivity(&mut self, bench: TerrainConnectivityBench) {
        debug_assert!(matches!(self, Self::Garden));
        *self = Self::TerrainConnectivityBenchmark(bench);
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
        Scenario::Garden => ScenarioOwner::Garden,
        Scenario::CanopyAudioDiagnostic { constrained_budget } => {
            ScenarioOwner::CanopyAudioDiagnostic(CanopyAudioDiagnosticRuntime::new(
                constrained_budget,
            ))
        }
        Scenario::WaterExperience => ScenarioOwner::WaterExperience(WaterExperienceOwner::Pending),
        Scenario::WaterEditSoak => ScenarioOwner::WaterEditSoak(water::WaterEditSoak::default()),
        Scenario::EnvironmentLighting(case) => {
            ScenarioOwner::EnvironmentLighting(EnvironmentLightingTestScene::new(case))
        }
        Scenario::HybridTransparency => {
            ScenarioOwner::HybridTransparency(HybridTransparencyTestScene::new())
        }
        Scenario::House => ScenarioOwner::House(HouseSceneOwner),
        Scenario::TerrainConnectivityBenchmark(options) => {
            ScenarioOwner::TerrainConnectivityBenchmark(TerrainConnectivityBench::new(options))
        }
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
                scenario: ScenarioOwner::FoliageShadowBenchmark(DenoiserBench::new(options)),
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
            runtime: DenoiserBench::new(benchmark),
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
    use crate::cli::{BenchmarkPlan, CameraMotion, DenoiserBenchMode, DenoiserBenchOptions};

    fn benchmark(mode: DenoiserBenchMode) -> DenoiserBenchOptions {
        DenoiserBenchOptions {
            report_path: "report.toml".to_owned(),
            warmup_frames: 12,
            capture_frames: 8,
            mode,
        }
    }

    #[test]
    fn fixed_scenarios_construct_exactly_one_runtime_owner() {
        let automation = AutomationPlan::default();
        let owner = prepare_startup_owners(
            automation,
            Scenario::TerrainConnectivityBenchmark(crate::cli::TerrainConnectivityBenchOptions {
                mode: crate::cli::TerrainConnectivityBenchMode::Correct,
                available_particles: 8,
                warmup_frames: 1,
                observe_frames: 1,
                voxel_budget: 8,
            }),
        )
        .unwrap()
        .scenario;

        assert!(matches!(
            owner,
            ScenarioOwner::TerrainConnectivityBenchmark(_)
        ));
    }

    #[test]
    fn contradictory_foliage_and_camera_benchmarks_fail_before_owner_construction() {
        let automation = AutomationPlan {
            camera: CameraAutomation::DenoiserBenchmark {
                snapshot: "tree".to_owned(),
                benchmark: benchmark(DenoiserBenchMode::CameraSnapshot(CameraMotion::Fixed)),
            },
            benchmarks: BenchmarkPlan::default(),
        };

        let error = prepare_startup_owners(
            automation,
            Scenario::FoliageShadowBenchmark(benchmark(DenoiserBenchMode::FoliageShadow)),
        )
        .err()
        .expect("contradictory owners must be rejected");

        assert!(error.contains("second camera automation"));
    }
}
