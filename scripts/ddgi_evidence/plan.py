from __future__ import annotations

from dataclasses import replace
from pathlib import Path

from .model import (
    Aggregate,
    AnalyzeCurrentCapture,
    BuildRelease,
    Capture,
    CheckSkyNormalization,
    Claim,
    CorrectnessOptions,
    Evidence,
    ExecutionPlan,
    FactRef,
    FailureKey,
    IncludeSuite,
    InflightTerrainEditsOptions,
    LifecycleOptions,
    LocalTerrainConvergenceOptions,
    ProductionAnalyzerOptions,
    RelocateArtifact,
    RunRequest,
    RuntimeTerrainEditsOptions,
    ScenarioValidation,
    Setup,
    Suite,
    SummarizeConvergence,
    TerrainEditCycleOptions,
    TransportOptions,
    ValidateProcessEvidence,
    ValidateRadianceLifecycle,
    ValidateScenarioLog,
)


STANDARD_RUST_LOG = (
    "warn,re_flora::run_log_binding=info,re_flora::tracer=info,"
    "re_flora::app::core::environment_irradiance_capture=info,"
    "re_flora::app::core::environment_lighting_test_scene=info"
)
CONVERGENCE_RUST_LOG = (
    "warn,re_flora::run_log_binding=info,re_flora::tracer=info,"
    "re_flora::ddgi_convergence_evidence=debug,"
    "re_flora::app::core::environment_irradiance_capture=info,"
    "re_flora::app::core::environment_lighting_test_scene=info"
)


def _options(request: RunRequest, expected: type, default):
    if request.options is None:
        return default
    if not isinstance(request.options, expected):
        raise TypeError(
            f"{request.suite.value} requires {expected.__name__}, "
            f"got {type(request.options).__name__}"
        )
    return request.options


def _run_dir(request: RunRequest, option_dir: Path | None, default: str) -> Path:
    root = option_dir or request.repo_root / "target" / default
    return root / request.run_id


def _process(capture: Capture) -> ValidateProcessEvidence:
    return ValidateProcessEvidence(
        capture=capture.capture,
        console=capture.console,
        require_test_scene_startup=capture.require_test_scene_startup,
    )


def _analysis(capture: Path, *arguments: str | FactRef, output: Path | None = None):
    return AnalyzeCurrentCapture(
        capture=capture,
        options=ProductionAnalyzerOptions(tuple(arguments)),
        output=output,
    )


def plan(request: RunRequest) -> ExecutionPlan:
    match request.suite:
        case Suite.CORRECTNESS:
            return _correctness(request)
        case Suite.INFLIGHT_TERRAIN_EDITS:
            return _inflight(request)
        case Suite.LIFECYCLE:
            return _lifecycle(request)
        case Suite.LOCAL_TERRAIN_CONVERGENCE:
            return _local(request)
        case Suite.RUNTIME_TERRAIN_EDITS:
            return _runtime(request)
        case Suite.TERRAIN_EDIT_CYCLE:
            return _cycle(request)
        case Suite.TRANSPORT:
            return _transport(request)
    raise AssertionError(f"unhandled suite: {request.suite}")


def _correctness(request: RunRequest) -> ExecutionPlan:
    options = _options(request, CorrectnessOptions, CorrectnessOptions())
    run_dir = _run_dir(request, options.output_dir, "ddgi-correctness")
    stages: list = [Setup("correctness.setup", (BuildRelease(),))]
    views = (
        ("final", "final-a"),
        ("final", "final-b"),
        ("moment-visibility", "moment-visibility"),
        ("exact-visibility", "exact-visibility"),
        ("exact-irradiance", "exact-irradiance"),
        ("unoccluded-irradiance", "unoccluded-irradiance"),
        ("equal-weight-irradiance", "equal-weight-irradiance"),
        ("raw-cage-irradiance", "raw-cage-irradiance"),
    )
    for case_name in ("sealed", "portal", "walls"):
        for spacing in (32, 16):
            paths = {
                suffix: run_dir / f"{case_name}-spacing{spacing}-{suffix}.rfirr"
                for _, suffix in views
            }
            capture_stage_ids = []
            for view, suffix in views:
                capture = paths[suffix]
                action = Capture(
                    suite=Suite.CORRECTNESS,
                    capture=capture,
                    console=capture.with_suffix(".console.log"),
                    scenario=case_name,
                    spacing_voxels=spacing,
                    auto_exit=options.auto_exit,
                    capture_target="converged",
                    debug_view=view,
                    rust_log=STANDARD_RUST_LOG,
                    extra_arguments=(
                        ("--ddgi-terrain-hard-origin", options.terrain_hard_origin)
                        if options.terrain_hard_origin
                        else ()
                    ),
                )
                capture_stage_id = (
                    f"correctness.{case_name}.{spacing}.capture.{suffix}"
                )
                capture_stage_ids.append(capture_stage_id)
                stages.append(
                    Evidence(
                        capture_stage_id,
                        FailureKey(
                            Suite.CORRECTNESS,
                            f"{case_name}-{suffix}",
                            spacing,
                        ),
                        (action, _process(action)),
                    )
                )
            key = FailureKey(Suite.CORRECTNESS, case_name, spacing)
            final_args: list[str] = [
                "--correctness",
                "--require-nonnegative-rgb",
                "--expect-debug-view",
                "final",
                "--reference",
                str(paths["exact-irradiance"]),
            ]
            if case_name == "sealed":
                final_args += ["--max-luminance", "0.00001", "--max-reference-error-p99", "0.00001"]
            elif case_name == "portal":
                final_args += ["--min-luminance-p99", "0.10", "--max-reference-error-p99", "0.01"]
            else:
                final_args += [
                    "--max-reference-error-p99",
                    "0.40" if spacing == 32 else "0.375",
                    "--min-filter-visibility-reject-count",
                    "1",
                ]
            analyses = [_analysis(paths["final-a"], *final_args)]
            if case_name == "walls":
                analyses.append(
                    _analysis(
                        paths["final-a"],
                        "--correctness",
                        "--require-nonnegative-rgb",
                        "--expect-debug-view",
                        "final",
                        "--reference",
                        str(paths["final-b"]),
                        "--max-reference-error-p99",
                        "0.00001",
                        "--max-reference-error-max",
                        "0.00001",
                    )
                )
            else:
                analyses[0] = _analysis(paths["final-a"], *final_args, "--compare", str(paths["final-b"]))
            visibility_delta = ("--min-reference-error-p99", "0.01") if case_name == "walls" else ()
            analyses.extend(
                (
                    _analysis(
                        paths["moment-visibility"],
                        "--correctness",
                        "--require-nonnegative-rgb",
                        "--expect-debug-view",
                        "moment-visibility",
                        "--reference",
                        str(paths["exact-visibility"]),
                        *visibility_delta,
                    ),
                    _analysis(paths["exact-visibility"], "--correctness", "--require-nonnegative-rgb", "--expect-debug-view", "exact-visibility"),
                    _analysis(paths["exact-irradiance"], "--correctness", "--require-nonnegative-rgb", "--expect-debug-view", "exact-irradiance"),
                    _analysis(paths["unoccluded-irradiance"], "--correctness", "--require-nonnegative-rgb", "--expect-debug-view", "unoccluded-irradiance", "--reference", str(paths["final-a"]), *visibility_delta),
                    _analysis(paths["equal-weight-irradiance"], "--correctness", "--require-nonnegative-rgb", "--expect-debug-view", "equal-weight-irradiance", "--reference", str(paths["unoccluded-irradiance"]), *visibility_delta),
                    _analysis(paths["raw-cage-irradiance"], "--correctness", "--require-nonnegative-rgb", "--expect-debug-view", "raw-cage-irradiance", "--reference", str(paths["equal-weight-irradiance"]), *visibility_delta),
                )
            )
            stages.append(
                Aggregate(
                    f"correctness.{case_name}.{spacing}.analysis",
                    tuple(analyses),
                    key,
                    tuple(capture_stage_ids),
                )
            )
    return ExecutionPlan(request, run_dir, tuple(stages))


def _inflight(request: RunRequest) -> ExecutionPlan:
    options = _options(
        request, InflightTerrainEditsOptions, InflightTerrainEditsOptions()
    )
    run_dir = _run_dir(request, options.output_dir, "ddgi-inflight-terrain-edits")
    stages: list = [Setup("inflight.setup", (BuildRelease(),))]
    for spacing in (32, 16):
        paths = []
        for repeat in (1, 2):
            capture = run_dir / f"terrain-edits-inflight-spacing{spacing}-repeat{repeat}.rfirr"
            action = Capture(
                Suite.INFLIGHT_TERRAIN_EDITS,
                capture,
                capture.with_suffix(".console.log"),
                "terrain-edits-inflight",
                spacing,
                options.auto_exit,
            )
            key = FailureKey(Suite.INFLIGHT_TERRAIN_EDITS, "latest-wins", spacing, repeat)
            stages.append(
                Evidence(
                    f"inflight.{spacing}.{repeat}",
                    key,
                    (
                        action,
                        _process(action),
                        ValidateScenarioLog(
                            ScenarioValidation.INFLIGHT_FINAL,
                            action.console,
                            spacing,
                        ),
                        _analysis(capture, "--min-luminance-p99", "0.10"),
                    ),
                )
            )
            paths.append(capture)
        stages.append(
            Aggregate(
                f"inflight.{spacing}.determinism",
                (
                    _analysis(
                        paths[0],
                        "--compare",
                        str(paths[1]),
                        "--compare-direct-light",
                    ),
                ),
                FailureKey(Suite.INFLIGHT_TERRAIN_EDITS, "determinism", spacing),
            )
        )
    return ExecutionPlan(request, run_dir, tuple(stages))


def _lifecycle(request: RunRequest) -> ExecutionPlan:
    options = _options(request, LifecycleOptions, LifecycleOptions())
    run_dir = _run_dir(request, options.output_dir, "ddgi-lifecycle-acceptance")
    stages: list = [Setup("lifecycle.setup", (BuildRelease(),))]
    roi = (0.85, 0.60, 1.025, 0.875, 0.675, 1.125)
    for spacing in (32, 16):
        capture = run_dir / f"radiance-changes-spacing-{spacing}.rfirr"
        action = Capture(
            Suite.LIFECYCLE,
            capture,
            capture.with_suffix(".console.log"),
            "radiance-changes",
            spacing,
            options.auto_exit,
            capture_target="published",
        )
        stage_id = f"lifecycle.radiance.{spacing}"
        facts = f"{stage_id}.stream"
        stages.append(
            Evidence(
                stage_id,
                FailureKey(Suite.LIFECYCLE, "radiance", spacing),
                (
                    action,
                    _process(action),
                    ValidateScenarioLog(
                        ScenarioValidation.RADIANCE_STREAM,
                        action.console,
                        spacing,
                        fact_namespace=facts,
                    ),
                    _analysis(
                        capture,
                        "--expect-spacing-voxels",
                        str(spacing),
                        "--expect-geometry-revision",
                        FactRef(facts, "geometry_revision"),
                        "--expect-radiance-revision",
                        "4",
                        "--expect-build-token-serial",
                        FactRef(facts, "build_token_serial"),
                        "--expect-field-serial",
                        FactRef(facts, "field_serial"),
                        "--expect-lifecycle-state",
                        "converging",
                        "--expect-update-epoch",
                        "0",
                        "--expect-source-state",
                        "converging",
                        "--expect-source-update-epoch",
                        "0",
                        "--expect-source-field-serial",
                        FactRef(facts, "source_field_serial"),
                        "--expect-source-radiance-revision",
                        "2",
                        "--expect-publication-state",
                        "published",
                        "--expect-batch-order",
                        "forward",
                        "--require-nonnegative-rgb",
                        output=run_dir / f"radiance-changes-spacing-{spacing}.analysis.json",
                    ),
                    ValidateRadianceLifecycle(
                        capture,
                        action.console,
                        spacing,
                        roi,
                        0.02,
                        run_dir / f"radiance-changes-spacing-{spacing}.lifecycle.json",
                    ),
                ),
            )
        )
    capture = run_dir / "density-changes.rfirr"
    action = Capture(
        Suite.LIFECYCLE,
        capture,
        capture.with_suffix(".console.log"),
        "density-changes",
        32,
        options.auto_exit,
        capture_target="e0",
    )
    facts = "lifecycle.density.stream"
    stages.append(
        Evidence(
            "lifecycle.density",
            FailureKey(Suite.LIFECYCLE, "density", 32),
            (
                action,
                _process(action),
                ValidateScenarioLog(
                    ScenarioValidation.DENSITY_STREAM,
                    action.console,
                    32,
                    fact_namespace=facts,
                ),
                _analysis(
                    capture,
                    "--expect-spacing-voxels",
                    "16",
                    "--expect-geometry-revision",
                    FactRef(facts, "geometry_revision"),
                    "--expect-radiance-revision",
                    "1",
                    "--expect-build-token-serial",
                    FactRef(facts, "build_token_serial"),
                    "--expect-field-serial",
                    FactRef(facts, "field_serial"),
                    "--expect-lifecycle-state",
                    "converging",
                    "--expect-update-epoch",
                    "0",
                    "--expect-publication-state",
                    "published",
                    "--expect-batch-order",
                    "forward",
                    "--require-nonnegative-rgb",
                    output=run_dir / "density-changes.analysis.json",
                ),
            ),
        )
    )
    return ExecutionPlan(request, run_dir, tuple(stages))


def _local(request: RunRequest) -> ExecutionPlan:
    options = _options(
        request, LocalTerrainConvergenceOptions, LocalTerrainConvergenceOptions()
    )
    run_dir = _run_dir(request, options.output_dir, "ddgi-local-terrain-convergence")
    capture = run_dir / "closed-spacing32.rfirr"
    action = Capture(
        Suite.LOCAL_TERRAIN_CONVERGENCE,
        capture,
        capture.with_suffix(".console.log"),
        "terrain-edits-closed",
        32,
        options.auto_exit,
        capture_target="converged",
        rust_log=CONVERGENCE_RUST_LOG,
    )
    stages = (
        Setup("local.setup", (BuildRelease(quiet=True),)),
        Evidence(
            "local.closed.32",
            FailureKey(Suite.LOCAL_TERRAIN_CONVERGENCE, "closed", 32),
            (
                action,
                _process(action),
                ValidateScenarioLog(
                    ScenarioValidation.LOCAL_RECOVERY,
                    action.console,
                    32,
                    minimum_epoch=options.minimum_recovery_epoch,
                    maximum_high_delta_epochs=options.maximum_post_promotion_high_delta_epochs,
                ),
                _analysis(capture, "--max-luminance", "0.00005"),
            ),
        ),
    )
    return ExecutionPlan(request, run_dir, stages)


def _runtime_capture(
    run_dir: Path,
    options: RuntimeTerrainEditsOptions,
    spacing: int,
    state: str,
    view: str,
    label: str,
    *,
    flora_enabled: bool = False,
    target: str = "e8",
) -> Capture:
    scenario = {
        "initial-open": "portal",
        "closed": "terrain-edits-closed",
        "sequential-reopened": "terrain-edits",
        "inflight-latest-wins": "terrain-edits-inflight",
        "inflight-stale-active": "terrain-edits-inflight-capture",
    }[state]
    capture = run_dir / f"{state}-spacing{spacing}-{label}.rfirr"
    return Capture(
        Suite.RUNTIME_TERRAIN_EDITS,
        capture,
        capture.with_suffix(".console.log"),
        scenario,
        spacing,
        options.auto_exit,
        capture_target=target,
        debug_view=view,
        flora_enabled=flora_enabled,
        case_label=state,
    )


def _runtime(request: RunRequest) -> ExecutionPlan:
    options = _options(
        request, RuntimeTerrainEditsOptions, RuntimeTerrainEditsOptions()
    )
    run_dir = _run_dir(request, options.output_dir, "ddgi-runtime-terrain-edits")
    stages: list = [Setup("runtime.setup", (BuildRelease(),))]
    states = ("initial-open", "closed", "sequential-reopened", "inflight-latest-wins")
    for spacing in (32, 16):
        for state in states:
            captures = []
            for view, label in (
                ("final", "final-a"),
                ("final", "final-b"),
                ("exact-irradiance", "exact-irradiance"),
            ):
                target = "converged" if state == "closed" else "e8"
                action = _runtime_capture(run_dir, options, spacing, state, view, label, target=target)
                captures.extend(
                    (
                        action,
                        _process(action),
                        ValidateScenarioLog(
                            ScenarioValidation.RUNTIME_FINAL,
                            action.console,
                            spacing,
                            state=state,
                            minimum_epoch=options.minimum_local_recovery_epoch,
                        ),
                    )
                )
            first = run_dir / f"{state}-spacing{spacing}-final-a.rfirr"
            second = run_dir / f"{state}-spacing{spacing}-final-b.rfirr"
            reference = run_dir / f"{state}-spacing{spacing}-exact-irradiance.rfirr"
            args = ["--correctness", "--compare", str(second), "--reference", str(reference)]
            if state == "closed":
                args += ["--max-luminance", "0.00005", "--max-reference-error-p99", "0.00005"]
            else:
                args += ["--max-reference-error-p99", "0.01", "--min-luminance-p99", "0.10"]
            if state == "sequential-reopened":
                args += ["--require-filter-history-retain-blend", "--require-filter-local-recovery-policy"]
            captures.append(_analysis(first, *args))
            stages.append(
                Evidence(
                    f"runtime.{state}.{spacing}",
                    FailureKey(Suite.RUNTIME_TERRAIN_EDITS, state, spacing),
                    tuple(captures),
                )
            )
        transient_paths = []
        for label in ("final-a", "final-b"):
            action = _runtime_capture(
                run_dir,
                options,
                spacing,
                "inflight-stale-active",
                "final",
                label,
                target="published",
            )
            transient_paths.append(action.capture)
            stages.append(
                Evidence(
                    f"runtime.transient.{spacing}.{label}",
                    FailureKey(Suite.RUNTIME_TERRAIN_EDITS, "inflight-stale-active", spacing),
                    (
                        action,
                        _process(action),
                        ValidateScenarioLog(
                            ScenarioValidation.RUNTIME_TRANSIENT,
                            action.console,
                            spacing,
                            fact_namespace=f"runtime.transient.{spacing}.{label}",
                        ),
                    ),
                )
            )
        facts = f"runtime.transient.{spacing}.final-a"
        stages.append(
            Aggregate(
                f"runtime.transient.{spacing}.analysis",
                (
                    _analysis(
                        transient_paths[0],
                        "--compare",
                        str(transient_paths[1]),
                        "--compare-direct-light",
                        "--expect-geometry-revision",
                        FactRef(facts, "active_revision"),
                        "--expect-publication-state",
                        "published",
                        "--min-luminance-p99",
                        "0.10",
                        "--require-nonnegative-rgb",
                        "--correctness",
                        "--direct-light-sunlit-roi",
                        "0.85",
                        "0.60",
                        "1.025",
                        "0.875",
                        "0.675",
                        "1.125",
                        "--min-direct-light-sunlit-luminance-mean",
                        "0.15",
                        "--direct-light-shadowed-roi",
                        "0.425",
                        "0.60",
                        "1.075",
                        "0.45",
                        "0.85",
                        "1.275",
                        "--max-direct-light-shadowed-luminance-max",
                        "0",
                    ),
                ),
                FailureKey(Suite.RUNTIME_TERRAIN_EDITS, "inflight-stale-active-analysis", spacing),
            )
        )
    source = run_dir / "sequential-reopened-spacing32-flora-final.rfirr"
    action = _runtime_capture(
        run_dir,
        options,
        32,
        "sequential-reopened",
        "final",
        "flora-final",
        flora_enabled=True,
    )
    destination = run_dir / "flora-consumer-spacing32-final.rfirr"
    stages.append(
        Evidence(
            "runtime.flora.32",
            FailureKey(Suite.RUNTIME_TERRAIN_EDITS, "flora", 32),
            (
                action,
                _process(action),
                RelocateArtifact(source, destination),
                RelocateArtifact(action.console, destination.with_suffix(".console.log")),
                ValidateScenarioLog(
                    ScenarioValidation.RUNTIME_FINAL,
                    destination.with_suffix(".console.log"),
                    32,
                    state="sequential-reopened",
                    minimum_epoch=options.minimum_local_recovery_epoch,
                ),
                ValidateScenarioLog(
                    ScenarioValidation.FLORA_CONSUMER,
                    destination.with_suffix(".console.log"),
                    32,
                ),
                _analysis(destination, "--min-luminance-p99", "0.10"),
            ),
        )
    )
    return ExecutionPlan(request, run_dir, tuple(stages))


def _cycle(request: RunRequest) -> ExecutionPlan:
    options = _options(request, TerrainEditCycleOptions, TerrainEditCycleOptions())
    run_dir = _run_dir(request, options.output_dir, "ddgi-terrain-edit-cycle")
    stages: list = [Setup("cycle.setup", (BuildRelease(),))]
    for spacing in (32, 16):
        for scenario, mode in (("terrain-edits-closed", "closed"), ("terrain-edits", "reopened")):
            capture = run_dir / f"terrain-edits-spacing{spacing}-{mode}.rfirr"
            action = Capture(
                Suite.TERRAIN_EDIT_CYCLE,
                capture,
                capture.with_suffix(".console.log"),
                scenario,
                spacing,
                options.auto_exit,
                capture_target="converged" if mode == "closed" else None,
                case_label=mode,
            )
            threshold = ("--max-luminance", "0.00005") if mode == "closed" else ("--min-luminance-p99", "0.10")
            stages.append(
                Evidence(
                    f"cycle.{mode}.{spacing}",
                    FailureKey(Suite.TERRAIN_EDIT_CYCLE, mode, spacing),
                    (
                        action,
                        _process(action),
                        ValidateScenarioLog(
                            ScenarioValidation.TERRAIN_EDIT,
                            action.console,
                            spacing,
                            state=mode,
                        ),
                        _analysis(capture, *threshold),
                    ),
                )
            )
    return ExecutionPlan(request, run_dir, tuple(stages))


def _transport(request: RunRequest) -> ExecutionPlan:
    options = _options(request, TransportOptions, TransportOptions())
    run_dir = _run_dir(request, options.output_dir, "ddgi-transport-acceptance")
    stages: list = [Setup("transport.setup", (BuildRelease(),))]
    donor_roi = ("0.53125", "0.4375", "0.9375", "0.8125", "0.59375", "0.9375")
    dogleg_roi = ("1.125", "0.4375", "0.5", "1.3125", "0.625", "0.5")

    def add_stage(case: str, spacing: int, target: str, order: str, *extra: str) -> None:
        capture = run_dir / f"{case}-spacing{spacing}-{target}-{order}.rfirr"
        action = Capture(
            Suite.TRANSPORT,
            capture,
            capture.with_suffix(".console.log"),
            case,
            spacing,
            options.auto_exit,
            capture_target=target,
            debug_view="final",
            batch_order=order,
            rust_log=CONVERGENCE_RUST_LOG,
        )
        analysis = _analysis(
            capture,
            "--correctness",
            "--expect-debug-view",
            "final",
            "--require-nonnegative-rgb",
            "--expect-spacing-voxels",
            str(spacing),
            "--expect-batch-order",
            order,
            *extra,
            output=capture.with_suffix(".analysis.json"),
        )
        stages.append(
            Evidence(
                f"transport.{case}.{spacing}.{target}.{order}",
                FailureKey(Suite.TRANSPORT, f"{case}-{target}-{order}", spacing),
                (action, _process(action), analysis),
            )
        )

    for spacing in (32, 16):
        add_stage("sealed", spacing, "e0", "forward", "--expect-lifecycle-state", "converging", "--expect-update-epoch", "0", "--expect-publication-state", "published", "--require-zero-rgb")
        add_stage("sealed", spacing, "e1", "forward", "--expect-lifecycle-state", "converging", "--expect-update-epoch", "1", "--expect-source-state", "converging", "--expect-source-update-epoch", "0", "--expect-publication-state", "published", "--max-luminance", "0.00001")
        add_stage("sealed", spacing, "converged", "forward", "--expect-lifecycle-state", "converged", "--expect-publication-state", "published", "--max-luminance", "0.00001")
        donor = run_dir / f"donor-spacing{spacing}-e0-forward.rfirr"
        add_stage("donor", spacing, "e0", "forward", "--expect-lifecycle-state", "converging", "--expect-update-epoch", "0", "--expect-publication-state", "published", "--world-roi", *donor_roi, "--min-roi-luminance-mean", "0.045", "--max-exact-direct-sun-visibility", "0")
        add_stage("donor", spacing, "e0", "reverse", "--expect-lifecycle-state", "converging", "--expect-update-epoch", "0", "--expect-publication-state", "published", "--world-roi", *donor_roi, "--min-roi-luminance-mean", "0.045", "--max-exact-direct-sun-visibility", "0", "--compare", str(donor))
        dogleg = run_dir / f"dogleg-spacing{spacing}-e0-forward.rfirr"
        add_stage("dogleg", spacing, "e0", "forward", "--expect-lifecycle-state", "converging", "--expect-update-epoch", "0", "--expect-publication-state", "published", "--world-roi", *dogleg_roi, "--max-roi-luminance-mean", "0.00002", "--max-exact-direct-sun-visibility", "0")
        add_stage("dogleg", spacing, "e1", "forward", "--expect-lifecycle-state", "converging", "--expect-update-epoch", "1", "--expect-source-state", "converging", "--expect-source-update-epoch", "0", "--expect-publication-state", "published", "--world-roi", *dogleg_roi, "--baseline", str(dogleg), "--min-roi-luminance-gain", "0.000035", "--max-exact-direct-sun-visibility", "0")
        for convergence_case in ("portal", "donor", "dogleg"):
            add_stage(convergence_case, spacing, "converged", "forward", "--expect-lifecycle-state", "converged", "--expect-publication-state", "published")
    stages.append(
        Aggregate(
            "transport.convergence",
            (SummarizeConvergence(run_dir, run_dir / "convergence-calibration.json"),),
            FailureKey(Suite.TRANSPORT, "convergence"),
        )
    )
    child_run_id = request.run_id
    correctness = plan(
        RunRequest(
            Suite.CORRECTNESS,
            request.repo_root,
            request.dry_run,
            options.correctness,
            child_run_id,
        )
    )
    runtime = plan(
        RunRequest(
            Suite.RUNTIME_TERRAIN_EDITS,
            request.repo_root,
            request.dry_run,
            options.runtime,
            child_run_id,
        )
    )
    lifecycle = plan(
        RunRequest(
            Suite.LIFECYCLE,
            request.repo_root,
            request.dry_run,
            options.lifecycle,
            child_run_id,
        )
    )
    stages.extend(
        (
            IncludeSuite("transport.include.correctness", correctness),
            IncludeSuite("transport.include.runtime", runtime),
            Aggregate(
                "transport.sky-normalization",
                (CheckSkyNormalization(),),
                FailureKey(Suite.TRANSPORT, "sky-normalization"),
            ),
            IncludeSuite("transport.include.lifecycle", lifecycle),
            Claim(
                "transport.filter-history-outcome",
                "[DDGI_TRANSPORT] filter-history-outcome=ACCEPTED seam=dogleg-e0-e1-production-capture",
                (
                    "transport.dogleg.32.e0.forward",
                    "transport.dogleg.32.e1.forward",
                    "transport.dogleg.16.e0.forward",
                    "transport.dogleg.16.e1.forward",
                ),
            ),
            Claim(
                "transport.direct-sun",
                "[DDGI_TRANSPORT] direct-sun-framebuffer=PROVEN seam=v6-direct-light-plane runner=check_ddgi_runtime_terrain_edits.sh",
                ("transport.include.runtime",),
            ),
            Claim(
                "transport.filter-history-action",
                "[DDGI_TRANSPORT] filter-history-action=PROVEN seam=owner-generated-filter-epoch-v10",
                ("transport.include.correctness", "transport.include.runtime"),
            ),
        )
    )
    return ExecutionPlan(request, run_dir, tuple(stages))
