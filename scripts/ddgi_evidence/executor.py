from __future__ import annotations

import os
import shlex
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

from .model import (
    Action,
    ActionFailure,
    Aggregate,
    AnalyzeCurrentCapture,
    BuildRelease,
    Capture,
    CheckSkyNormalization,
    Claim,
    Evidence,
    ExecutionPlan,
    FailureKey,
    IncludeSuite,
    IncludedRunReport,
    RelocateArtifact,
    RunReport,
    Setup,
    Suite,
    SummarizeConvergence,
    ValidateProcessEvidence,
    ValidateRadianceLifecycle,
    ValidateScenarioLog,
)


@dataclass(frozen=True)
class CommandRecord:
    kind: str
    argv: tuple[str, ...]


@dataclass(frozen=True)
class ActionResult:
    succeeded: bool
    message: str = ""
    facts: dict[str, str] = field(default_factory=dict)


class _ProductionActionResult(ActionResult):
    """Successful evidence produced only by the concrete subprocess adapter."""


@dataclass(frozen=True)
class _ExecutionOutcome:
    report: RunReport
    production_complete: bool


class RecordingHost:
    """Zero-side-effect adapter that records the production plan's exact argv."""

    def __init__(self, *, stdout=None, stderr=None) -> None:
        self.commands: list[CommandRecord] = []
        self.stdout = stdout or sys.stdout
        self.stderr = stderr or sys.stderr

    def prepare(self, run_dirs: tuple[Path, ...]) -> ActionResult:
        return ActionResult(True)

    def build(self, action: BuildRelease, repo_root: Path) -> ActionResult:
        self._record("build", action.argv(repo_root))
        return ActionResult(True)

    def capture(self, action: Capture, repo_root: Path) -> ActionResult:
        self._announce_capture(action)
        self._record("capture", action.argv(repo_root))
        return ActionResult(True)

    def validate_process(self, action: ValidateProcessEvidence) -> ActionResult:
        self._record("process-validation", ())
        return ActionResult(True)

    def validate_scenario(self, action: ValidateScenarioLog) -> ActionResult:
        self._record("scenario-validation", ())
        fact_names = (
            "field_serial",
            "source_field_serial",
            "geometry_revision",
            "build_token_serial",
            "obsolete_density_token_serial",
            "active_revision",
        )
        return ActionResult(
            True,
            facts={name: f"{{dry-run:{name}}}" for name in fact_names},
        )

    def analyze(
        self,
        action: AnalyzeCurrentCapture,
        repo_root: Path,
        facts: dict[tuple[str, str], str],
    ) -> ActionResult:
        argv = action.argv(repo_root, facts)
        self._record("analysis", argv)
        print("analyze_current_capture " + shlex.join(argv[1:]), file=self.stderr)
        return ActionResult(True)

    def validate_radiance(self, action: ValidateRadianceLifecycle) -> ActionResult:
        self._record("radiance-validation", ())
        return ActionResult(True)

    def summarize(self, action: SummarizeConvergence, repo_root: Path) -> ActionResult:
        self._record("convergence-summary", action.argv(repo_root))
        return ActionResult(True)

    def check_sky(self, action: CheckSkyNormalization, repo_root: Path) -> ActionResult:
        self._record("sky-normalization", action.argv(repo_root))
        return ActionResult(True)

    def relocate(self, action: RelocateArtifact) -> ActionResult:
        self._record("relocate", (str(action.source), str(action.destination)))
        return ActionResult(True)

    def emit(self, message: str, *, error: bool = False) -> None:
        print(message, file=self.stderr if error else self.stdout)

    def _record(self, kind: str, argv: tuple[str, ...]) -> None:
        self.commands.append(CommandRecord(kind, argv))
        if argv and kind != "analysis":
            print(shlex.join(argv), file=self.stdout)

    def _announce_capture(self, action: Capture) -> None:
        match action.suite:
            case Suite.CORRECTNESS:
                self.emit(
                    f"[DDGI_CORRECTNESS] case={action.scenario} "
                    f"spacing={action.spacing_voxels} backend=ddgi view={action.debug_view}"
                )
            case Suite.LIFECYCLE:
                group = (
                    f"RADIANCE-{action.spacing_voxels}"
                    if action.scenario == "radiance-changes"
                    else "DENSITY"
                )
                self.emit(
                    f"[DDGI_LIFECYCLE] group={group} scene={action.scenario} "
                    f"target={action.capture_target} running"
                )
            case Suite.RUNTIME_TERRAIN_EDITS:
                self.emit(
                    f"[DDGI_RUNTIME_EDIT] state={action.case_label} "
                    f"spacing={action.spacing_voxels} view={action.debug_view}"
                )
            case Suite.TRANSPORT:
                self.emit(
                    f"[DDGI_TRANSPORT] capture case={action.scenario} "
                    f"spacing={action.spacing_voxels} target={action.capture_target} "
                    f"order={action.batch_order}"
                )
            case _:
                return


class SubprocessHost(RecordingHost):
    """Production adapter. Every process launch uses argv with ``shell=False``."""

    def prepare(self, run_dirs: tuple[Path, ...]) -> ActionResult:
        for run_dir in run_dirs:
            try:
                run_dir.mkdir(parents=True, exist_ok=True)
            except OSError as error:
                return ActionResult(
                    False,
                    f"cannot create run directory {run_dir}: {error}",
                )
        return _ProductionActionResult(True)

    def build(self, action: BuildRelease, repo_root: Path) -> ActionResult:
        return self._run(action.argv(repo_root), cwd=repo_root)

    def capture(self, action: Capture, repo_root: Path) -> ActionResult:
        self._announce_capture(action)
        argv = action.argv(repo_root)
        environment = dict(os.environ)
        environment["RUST_LOG"] = action.rust_log
        try:
            with action.console.open("w", encoding="utf-8") as console:
                process = subprocess.Popen(
                    argv,
                    cwd=repo_root,
                    env=environment,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    text=True,
                    shell=False,
                )
                assert process.stdout is not None
                for line in process.stdout:
                    console.write(line)
                    console.flush()
                    self.stdout.write(line)
                    self.stdout.flush()
                status = process.wait()
        except OSError as error:
            return ActionResult(False, f"capture launch failed: {error}")
        if status != 0:
            return ActionResult(False, f"capture process exited {status}")
        if not action.capture.is_file():
            return ActionResult(False, f"capture process produced no artifact: {action.capture}")
        return _ProductionActionResult(True)

    def validate_process(self, action: ValidateProcessEvidence) -> ActionResult:
        from .validation import validate_process_evidence

        try:
            run_log = validate_process_evidence(
                action.console,
                require_test_scene_startup=action.require_test_scene_startup,
            )
            destination = Path(
                str(action.console).removesuffix(".console.log") + ".run.log"
            )
            if destination != run_log:
                shutil.copy2(run_log, destination)
        except (OSError, ValueError) as error:
            return ActionResult(False, str(error))
        return _ProductionActionResult(True)

    def validate_scenario(self, action: ValidateScenarioLog) -> ActionResult:
        from .validation import validate_scenario_log

        try:
            facts = validate_scenario_log(action)
        except (OSError, ValueError) as error:
            return ActionResult(False, str(error))
        return _ProductionActionResult(
            True,
            facts={name: str(value) for name, value in facts.items()},
        )

    def analyze(
        self,
        action: AnalyzeCurrentCapture,
        repo_root: Path,
        facts: dict[tuple[str, str], str],
    ) -> ActionResult:
        argv = action.argv(repo_root, facts)
        return self._run(argv, cwd=repo_root, output=action.output)

    def validate_radiance(self, action: ValidateRadianceLifecycle) -> ActionResult:
        from .validation import validate_radiance_lifecycle

        try:
            report = validate_radiance_lifecycle(action)
            action.output.write_text(report, encoding="utf-8")
        except (OSError, ValueError) as error:
            return ActionResult(False, str(error))
        return _ProductionActionResult(True)

    def summarize(self, action: SummarizeConvergence, repo_root: Path) -> ActionResult:
        return self._run(action.argv(repo_root), cwd=repo_root)

    def check_sky(self, action: CheckSkyNormalization, repo_root: Path) -> ActionResult:
        return self._run(action.argv(repo_root), cwd=repo_root)

    def relocate(self, action: RelocateArtifact) -> ActionResult:
        try:
            action.source.replace(action.destination)
        except OSError as error:
            return ActionResult(False, str(error))
        return _ProductionActionResult(True)

    def _run(
        self,
        argv: tuple[str, ...],
        *,
        cwd: Path,
        output: Path | None = None,
    ) -> ActionResult:
        try:
            if output is None:
                result = subprocess.run(argv, cwd=cwd, check=False, shell=False)
                return_code = result.returncode
            else:
                with output.open("w", encoding="utf-8") as sink:
                    process = subprocess.Popen(
                        argv,
                        cwd=cwd,
                        stdout=subprocess.PIPE,
                        text=True,
                        shell=False,
                    )
                    assert process.stdout is not None
                    for line in process.stdout:
                        sink.write(line)
                        sink.flush()
                        self.stdout.write(line)
                        self.stdout.flush()
                    return_code = process.wait()
        except OSError as error:
            return ActionResult(False, str(error))
        if return_code != 0:
            return ActionResult(False, f"command exited {return_code}: {shlex.join(argv)}")
        return _ProductionActionResult(True)


def _perform(
    host: RecordingHost,
    action: Action,
    repo_root: Path,
    facts: dict[tuple[str, str], str],
) -> ActionResult:
    match action:
        case BuildRelease():
            return host.build(action, repo_root)
        case Capture():
            return host.capture(action, repo_root)
        case ValidateProcessEvidence():
            return host.validate_process(action)
        case ValidateScenarioLog():
            return host.validate_scenario(action)
        case AnalyzeCurrentCapture():
            return host.analyze(action, repo_root, facts)
        case ValidateRadianceLifecycle():
            return host.validate_radiance(action)
        case SummarizeConvergence():
            return host.summarize(action, repo_root)
        case CheckSkyNormalization():
            return host.check_sky(action, repo_root)
        case RelocateArtifact():
            return host.relocate(action)
    raise AssertionError(f"unhandled action: {action!r}")


def _workspace_directories(execution_plan: ExecutionPlan) -> tuple[Path, ...]:
    directories: list[Path] = []

    def collect(plan: ExecutionPlan) -> None:
        if plan.run_dir not in directories:
            directories.append(plan.run_dir)
        for stage in plan.stages:
            if isinstance(stage, IncludeSuite):
                collect(stage.execution_plan)

    collect(execution_plan)
    return tuple(directories)


def execute(execution_plan: ExecutionPlan, host: RecordingHost) -> RunReport:
    if not execution_plan.request.dry_run:
        prepared = host.prepare(_workspace_directories(execution_plan))
        if not prepared.succeeded:
            return RunReport(
                execution_plan.request.suite,
                execution_plan.run_dir,
                (
                    ActionFailure(
                        FailureKey(execution_plan.request.suite, "setup"),
                        prepared.message,
                    ),
                ),
                (),
                {},
                (),
            )
    return _execute(execution_plan, host).report


def _execute(
    execution_plan: ExecutionPlan,
    host: RecordingHost,
) -> _ExecutionOutcome:
    failures: list[ActionFailure] = []
    failure_keys: set[FailureKey] = set()
    claims: list[str] = []
    facts: dict[tuple[str, str], str] = {}
    included_reports: list[IncludedRunReport] = []
    stage_status: dict[str, bool] = {}
    production_stages: set[str] = set()

    def append_failure(failure: ActionFailure) -> None:
        if failure.key in failure_keys:
            return
        failure_keys.add(failure.key)
        failures.append(failure)

    for stage in execution_plan.stages:
        if isinstance(stage, IncludeSuite):
            outcome = _execute(stage.execution_plan, host)
            nested = outcome.report
            included_reports.append(IncludedRunReport(stage.id, nested))
            if not nested.succeeded:
                append_failure(
                    ActionFailure(
                        stage.failure_key,
                        f"{nested.suite.value} include failed with "
                        f"{len(nested.failures)} failure key(s)",
                    )
                )
            claims.extend(nested.claims)
            facts.update(nested.facts)
            stage_status[stage.id] = nested.succeeded
            if nested.succeeded and outcome.production_complete:
                production_stages.add(stage.id)
            continue
        if isinstance(stage, Claim):
            accepted = (
                not execution_plan.request.dry_run
                and not failures
                and all(
                    required in production_stages for required in stage.requires
                )
            )
            stage_status[stage.id] = accepted
            if accepted:
                production_stages.add(stage.id)
                claims.append(stage.message)
                host.emit(stage.message)
            continue
        if isinstance(stage, Aggregate) and not all(
            stage_status.get(required, False) for required in stage.requires
        ):
            stage_status[stage.id] = False
            continue
        key = (
            FailureKey(execution_plan.request.suite, "setup")
            if isinstance(stage, Setup)
            else stage.failure_key
        )
        succeeded = True
        production_complete = True
        blocked_segment = False
        first_failure = ""
        for action in stage.actions:
            if isinstance(action, Capture):
                blocked_segment = False
            elif blocked_segment or (
                not succeeded and isinstance(action, AnalyzeCurrentCapture)
            ):
                continue
            try:
                result = _perform(
                    host,
                    action,
                    execution_plan.request.repo_root,
                    facts,
                )
            except KeyError as error:
                result = ActionResult(False, f"missing dynamic fact {error.args[0]!r}")
            if not result.succeeded:
                succeeded = False
                production_complete = False
                blocked_segment = True
                if not first_failure:
                    first_failure = result.message
                continue
            if type(result) is not _ProductionActionResult:
                production_complete = False
            namespace = stage.id
            if isinstance(action, ValidateScenarioLog) and action.fact_namespace:
                namespace = action.fact_namespace
            for name, value in result.facts.items():
                facts[(namespace, name)] = value
        stage_status[stage.id] = succeeded
        if succeeded and production_complete:
            production_stages.add(stage.id)
        if not succeeded:
            append_failure(ActionFailure(key, first_failure))
            if isinstance(stage, Setup):
                break
    report = RunReport(
        execution_plan.request.suite,
        execution_plan.run_dir,
        tuple(failures),
        tuple(claims),
        facts,
        tuple(included_reports),
    )
    return _ExecutionOutcome(
        report,
        report.succeeded
        and all(stage.id in production_stages for stage in execution_plan.stages),
    )
