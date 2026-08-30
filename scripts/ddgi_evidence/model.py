from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import TypeAlias


class Suite(Enum):
    CORRECTNESS = "correctness"
    INFLIGHT_TERRAIN_EDITS = "inflight-terrain-edits"
    LIFECYCLE = "lifecycle"
    LOCAL_TERRAIN_CONVERGENCE = "local-terrain-convergence"
    RUNTIME_TERRAIN_EDITS = "runtime-terrain-edits"
    TERRAIN_EDIT_CYCLE = "terrain-edit-cycle"
    TRANSPORT = "transport"


@dataclass(frozen=True)
class CorrectnessOptions:
    auto_exit: str = "120"
    output_dir: Path | None = None
    terrain_hard_origin: str | None = None


@dataclass(frozen=True)
class InflightTerrainEditsOptions:
    auto_exit: str = "12"
    output_dir: Path | None = None


@dataclass(frozen=True)
class LifecycleOptions:
    auto_exit: str = "90"
    output_dir: Path | None = None


@dataclass(frozen=True)
class LocalTerrainConvergenceOptions:
    auto_exit: str = "30"
    output_dir: Path | None = None
    minimum_recovery_epoch: int = 4
    maximum_post_promotion_high_delta_epochs: int = 0


@dataclass(frozen=True)
class RuntimeTerrainEditsOptions:
    auto_exit: str = "60"
    output_dir: Path | None = None
    minimum_local_recovery_epoch: int = 4


@dataclass(frozen=True)
class TerrainEditCycleOptions:
    auto_exit: str = "60"
    output_dir: Path | None = None


@dataclass(frozen=True)
class TransportOptions:
    auto_exit: str = "120"
    output_dir: Path | None = None


SuiteOptions: TypeAlias = (
    CorrectnessOptions
    | InflightTerrainEditsOptions
    | LifecycleOptions
    | LocalTerrainConvergenceOptions
    | RuntimeTerrainEditsOptions
    | TerrainEditCycleOptions
    | TransportOptions
)


@dataclass(frozen=True)
class RunRequest:
    suite: Suite
    repo_root: Path
    dry_run: bool = False
    options: SuiteOptions | None = None
    run_id: str = "dry-run"


@dataclass(frozen=True)
class FactRef:
    stage_id: str
    name: str


Argument: TypeAlias = str | FactRef


@dataclass(frozen=True)
class FailureKey:
    suite: Suite
    case: str
    spacing: int | None = None
    repeat: int | None = None


@dataclass(frozen=True)
class ProductionAnalyzerOptions:
    arguments: tuple[Argument, ...] = ()


@dataclass(frozen=True)
class BuildRelease:
    quiet: bool = False

    def argv(self, repo_root: Path) -> tuple[str, ...]:
        quiet = ("--quiet",) if self.quiet else ()
        return (
            "/usr/bin/env",
            "cargo",
            "build",
            *quiet,
            "--release",
            "--manifest-path",
            str(repo_root / "Cargo.toml"),
        )


@dataclass(frozen=True)
class Capture:
    suite: Suite
    capture: Path
    console: Path
    scenario: str
    spacing_voxels: int
    auto_exit: str
    capture_target: str | None = None
    debug_view: str | None = None
    batch_order: str | None = None
    flora_enabled: bool = False
    rust_log: str = (
        "warn,re_flora::run_log_binding=info,re_flora::tracer=info,"
        "re_flora::app::core::environment_irradiance_capture=info,"
        "re_flora::app::core::environment_lighting_test_scene=info"
    )
    extra_arguments: tuple[str, ...] = ()
    require_test_scene_startup: bool = True

    def argv(self, repo_root: Path) -> tuple[str, ...]:
        arguments = [
            "/usr/bin/env",
            "cargo",
            "run",
            "--quiet",
            "--release",
            "--manifest-path",
            str(repo_root / "Cargo.toml"),
            "--",
            "--hidden",
            "--mute",
        ]
        if not self.flora_enabled:
            arguments.append("--no-flora")
        arguments.extend(("--no-particles", "--no-god-rays", "--no-lens-flare", "--no-clouds"))
        arguments.extend(
            (
                "--environment-lighting-test-scene",
                self.scenario,
                "--environment-probe-spacing-voxels",
                str(self.spacing_voxels),
            )
        )
        if self.capture_target:
            arguments.extend(("--environment-irradiance-capture-target", self.capture_target))
        if self.batch_order:
            arguments.extend(("--ddgi-batch-order", self.batch_order))
        if self.debug_view:
            arguments.extend(("--ddgi-debug-view", self.debug_view))
        arguments.extend(("--environment-irradiance-capture", str(self.capture)))
        arguments.extend(("--auto-exit", self.auto_exit))
        arguments.extend(self.extra_arguments)
        return tuple(arguments)


@dataclass(frozen=True)
class ValidateProcessEvidence:
    capture: Path
    console: Path
    require_test_scene_startup: bool = True


class ScenarioValidation(Enum):
    INFLIGHT_FINAL = "inflight-final"
    RADIANCE_STREAM = "radiance-stream"
    DENSITY_STREAM = "density-stream"
    LOCAL_RECOVERY = "local-recovery"
    RUNTIME_FINAL = "runtime-final"
    RUNTIME_TRANSIENT = "runtime-transient"
    FLORA_CONSUMER = "flora-consumer"
    TERRAIN_EDIT = "terrain-edit"


@dataclass(frozen=True)
class ValidateScenarioLog:
    validation: ScenarioValidation
    console: Path
    spacing_voxels: int
    state: str = ""
    minimum_epoch: int = 0
    maximum_high_delta_epochs: int = 0
    fact_namespace: str | None = None


@dataclass(frozen=True)
class AnalyzeCurrentCapture:
    capture: Path
    options: ProductionAnalyzerOptions = field(default_factory=ProductionAnalyzerOptions)
    output: Path | None = None

    def argv(self, repo_root: Path, facts: dict[tuple[str, str], str] | None = None) -> tuple[str, ...]:
        resolved: list[str] = []
        for argument in self.options.arguments:
            if isinstance(argument, FactRef):
                if facts is None:
                    resolved.append(f"{{{argument.stage_id}.{argument.name}}}")
                else:
                    resolved.append(facts[(argument.stage_id, argument.name)])
            else:
                resolved.append(argument)
        return (
            str(repo_root / "scripts/analyze_current_environment_irradiance_capture.py"),
            str(self.capture),
            *resolved,
        )


@dataclass(frozen=True)
class ValidateRadianceLifecycle:
    capture: Path
    console: Path
    spacing_voxels: int
    sunlit_roi: tuple[float, float, float, float, float, float]
    minimum_direct_light_delta: float
    output: Path


@dataclass(frozen=True)
class SummarizeConvergence:
    run_dir: Path
    output: Path

    def argv(self, repo_root: Path) -> tuple[str, ...]:
        return (
            str(repo_root / "scripts/summarize_ddgi_convergence.py"),
            "--run-dir",
            str(self.run_dir),
            "--output",
            str(self.output),
        )


@dataclass(frozen=True)
class CheckSkyNormalization:
    def argv(self, repo_root: Path) -> tuple[str, ...]:
        return (
            "/usr/bin/env",
            "python3",
            str(repo_root / "scripts/check_ddgi_sky_normalization_evidence.py"),
        )


@dataclass(frozen=True)
class RelocateArtifact:
    source: Path
    destination: Path


Action: TypeAlias = (
    BuildRelease
    | Capture
    | ValidateProcessEvidence
    | ValidateScenarioLog
    | AnalyzeCurrentCapture
    | ValidateRadianceLifecycle
    | SummarizeConvergence
    | CheckSkyNormalization
    | RelocateArtifact
)


@dataclass(frozen=True)
class Setup:
    id: str
    actions: tuple[Action, ...]


@dataclass(frozen=True)
class Evidence:
    id: str
    failure_key: FailureKey
    actions: tuple[Action, ...]


@dataclass(frozen=True)
class Aggregate:
    id: str
    actions: tuple[Action, ...]
    failure_key: FailureKey


@dataclass(frozen=True)
class IncludeSuite:
    id: str
    execution_plan: ExecutionPlan


@dataclass(frozen=True)
class Claim:
    id: str
    message: str
    requires: tuple[str, ...]


Stage: TypeAlias = Setup | Evidence | Aggregate | IncludeSuite | Claim


@dataclass(frozen=True)
class ExecutionPlan:
    request: RunRequest
    run_dir: Path
    stages: tuple[Stage, ...]


@dataclass(frozen=True)
class ActionFailure:
    key: FailureKey
    message: str


@dataclass(frozen=True)
class RunReport:
    suite: Suite
    run_dir: Path
    failures: tuple[ActionFailure, ...]
    claims: tuple[str, ...]
    facts: dict[tuple[str, str], str]

    @property
    def succeeded(self) -> bool:
        return not self.failures

    @property
    def exit_code(self) -> int:
        return 0 if self.succeeded else 1


def iter_actions(execution_plan: ExecutionPlan):
    for stage in execution_plan.stages:
        if isinstance(stage, IncludeSuite):
            yield from iter_actions(stage.execution_plan)
        elif isinstance(stage, (Setup, Evidence, Aggregate)):
            yield from stage.actions
