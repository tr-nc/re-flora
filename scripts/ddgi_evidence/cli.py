#!/usr/bin/env python3
from __future__ import annotations

import os
import sys
from collections.abc import Mapping
from datetime import datetime, timezone
from pathlib import Path


if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from ddgi_evidence.executor import RecordingHost, SubprocessHost, execute
from ddgi_evidence.model import (
    CorrectnessOptions,
    InflightTerrainEditsOptions,
    LifecycleOptions,
    LocalTerrainConvergenceOptions,
    RunRequest,
    RuntimeTerrainEditsOptions,
    Suite,
    TerrainEditCycleOptions,
    TransportOptions,
)
from ddgi_evidence.plan import plan


RUNNER_NAMES = {
    Suite.CORRECTNESS: "check_ddgi_correctness.sh",
    Suite.INFLIGHT_TERRAIN_EDITS: "check_ddgi_inflight_terrain_edits.sh",
    Suite.LIFECYCLE: "check_ddgi_lifecycle_acceptance.sh",
    Suite.LOCAL_TERRAIN_CONVERGENCE: "check_ddgi_local_terrain_convergence.sh",
    Suite.RUNTIME_TERRAIN_EDITS: "check_ddgi_runtime_terrain_edits.sh",
    Suite.TERRAIN_EDIT_CYCLE: "check_ddgi_terrain_edit_cycle.sh",
    Suite.TRANSPORT: "check_ddgi_transport_acceptance.sh",
}


def _path(environment: Mapping[str, str], name: str) -> Path | None:
    value = environment.get(name)
    return Path(value) if value else None


def _integer(environment: Mapping[str, str], name: str, default: int) -> int:
    value = environment.get(name)
    if value is None:
        return default
    try:
        return int(value)
    except ValueError as error:
        raise ValueError(f"{name} must be an integer, got {value!r}") from error


def _correctness_options(environment: Mapping[str, str]) -> CorrectnessOptions:
    return CorrectnessOptions(
        auto_exit=environment.get("DDGI_CORRECTNESS_AUTO_EXIT", "120"),
        output_dir=_path(environment, "DDGI_CORRECTNESS_OUTPUT_DIR"),
        terrain_hard_origin=environment.get("DDGI_CORRECTNESS_TERRAIN_HARD_ORIGIN") or None,
    )


def _lifecycle_options(environment: Mapping[str, str]) -> LifecycleOptions:
    return LifecycleOptions(
        auto_exit=environment.get("DDGI_LIFECYCLE_AUTO_EXIT", "90"),
        output_dir=_path(environment, "DDGI_LIFECYCLE_OUTPUT_DIR"),
    )


def _runtime_options(environment: Mapping[str, str]) -> RuntimeTerrainEditsOptions:
    return RuntimeTerrainEditsOptions(
        auto_exit=environment.get("DDGI_RUNTIME_TERRAIN_EDIT_AUTO_EXIT", "60"),
        output_dir=_path(environment, "DDGI_RUNTIME_TERRAIN_EDIT_OUTPUT_DIR"),
        minimum_local_recovery_epoch=_integer(
            environment, "DDGI_RUNTIME_TERRAIN_EDIT_MIN_RECOVERY_EPOCH", 4
        ),
    )


def request_from_environment(
    suite: Suite,
    repo_root: Path,
    dry_run: bool,
    environment: Mapping[str, str],
    *,
    run_id: str,
) -> RunRequest:
    match suite:
        case Suite.CORRECTNESS:
            options = _correctness_options(environment)
        case Suite.INFLIGHT_TERRAIN_EDITS:
            options = InflightTerrainEditsOptions(
                auto_exit=environment.get("DDGI_INFLIGHT_EDIT_AUTO_EXIT", "12"),
                output_dir=_path(environment, "DDGI_INFLIGHT_EDIT_OUTPUT_DIR"),
            )
        case Suite.LIFECYCLE:
            options = _lifecycle_options(environment)
        case Suite.LOCAL_TERRAIN_CONVERGENCE:
            options = LocalTerrainConvergenceOptions(
                auto_exit=environment.get("DDGI_LOCAL_TERRAIN_AUTO_EXIT", "30"),
                output_dir=_path(environment, "DDGI_LOCAL_TERRAIN_OUTPUT_DIR"),
                minimum_recovery_epoch=_integer(
                    environment, "DDGI_LOCAL_TERRAIN_MIN_RECOVERY_EPOCH", 4
                ),
                maximum_post_promotion_high_delta_epochs=_integer(
                    environment,
                    "DDGI_LOCAL_TERRAIN_MAX_POST_PROMOTION_HIGH_DELTA_EPOCHS",
                    0,
                ),
            )
        case Suite.RUNTIME_TERRAIN_EDITS:
            options = _runtime_options(environment)
        case Suite.TERRAIN_EDIT_CYCLE:
            options = TerrainEditCycleOptions(
                auto_exit=environment.get("DDGI_TERRAIN_EDIT_AUTO_EXIT", "60"),
                output_dir=_path(environment, "DDGI_TERRAIN_EDIT_OUTPUT_DIR"),
            )
        case Suite.TRANSPORT:
            options = TransportOptions(
                auto_exit=environment.get("DDGI_TRANSPORT_ACCEPTANCE_AUTO_EXIT", "120"),
                output_dir=_path(environment, "DDGI_TRANSPORT_ACCEPTANCE_OUTPUT_DIR"),
                correctness=_correctness_options(environment),
                runtime=_runtime_options(environment),
                lifecycle=_lifecycle_options(environment),
            )
    return RunRequest(suite, repo_root, dry_run, options, run_id)


def _usage(suite: Suite) -> str:
    return f"usage: {RUNNER_NAMES[suite]} [--dry-run]"


def _announce(suite: Suite) -> None:
    if suite is Suite.RUNTIME_TERRAIN_EDITS:
        print(
            "[DDGI_RUNTIME_EDIT] direct-sun-evidence=v6-direct-light-plane "
            "sunlit_min_mean=0.15 shadowed_max=0"
        )
    elif suite is Suite.TRANSPORT:
        print("[DDGI_TRANSPORT] threshold_provenance=docs/ddgi_transport_acceptance.md")
        print("[DDGI_TRANSPORT] convergence_provenance=docs/ddgi_convergence_calibration.md")
        print("[DDGI_TRANSPORT] convergence-policy=RUNTIME_LOG source=DDGI_CONVERGENCE_POLICY")
        print(
            "[DDGI_TRANSPORT] direct-sun-framebuffer=REQUIRED "
            "seam=v6-direct-light-plane runner=check_ddgi_runtime_terrain_edits.sh"
        )
        print(
            "[DDGI_TRANSPORT] filter-history-outcome=REQUIRED "
            "seam=dogleg-e0-e1-production-capture"
        )
        print(
            "[DDGI_TRANSPORT] filter-history-action=REQUIRED "
            "seam=owner-generated-filter-epoch-v10"
        )


def _summary(suite: Suite, dry_run: bool, run_dir: Path, failures: int) -> str:
    if dry_run:
        return {
            Suite.CORRECTNESS: "[DDGI_CORRECTNESS] dry-run matrix cases=3 spacings=2 views=8",
            Suite.INFLIGHT_TERRAIN_EDITS: "[DDGI_INFLIGHT_EDIT] dry-run matrix spacings=2 repeats=2",
            Suite.LIFECYCLE: "[DDGI_LIFECYCLE] dry-run complete scenarios=3",
            Suite.LOCAL_TERRAIN_CONVERGENCE: "[DDGI_LOCAL_TERRAIN] dry-run",
            Suite.RUNTIME_TERRAIN_EDITS: "[DDGI_RUNTIME_EDIT] dry-run matrix final_states=4x2x3 transient=2x2 flora=1 total_runs=29",
            Suite.TERRAIN_EDIT_CYCLE: "[DDGI_TERRAIN_EDIT] dry-run matrix spacings=2 scenarios=closed,reopened",
            Suite.TRANSPORT: "[DDGI_TRANSPORT] dry-run complete spacings=2 sealed_epochs=3 donor_epochs=2 dogleg_epochs=2 convergence_curves=8 batch_orders=2",
        }[suite]
    label = {
        Suite.CORRECTNESS: "DDGI_CORRECTNESS",
        Suite.INFLIGHT_TERRAIN_EDITS: "DDGI_INFLIGHT_EDIT",
        Suite.LIFECYCLE: "DDGI_LIFECYCLE",
        Suite.LOCAL_TERRAIN_CONVERGENCE: "DDGI_LOCAL_TERRAIN",
        Suite.RUNTIME_TERRAIN_EDITS: "DDGI_RUNTIME_EDIT",
        Suite.TERRAIN_EDIT_CYCLE: "DDGI_TERRAIN_EDIT",
        Suite.TRANSPORT: "DDGI_TRANSPORT",
    }[suite]
    return f"[{label}] output={run_dir} failures={failures}"


def main(arguments: list[str] | None = None) -> int:
    arguments = list(sys.argv[1:] if arguments is None else arguments)
    if not arguments:
        print("usage: cli.py SUITE [--dry-run]", file=sys.stderr)
        return 2
    try:
        suite = Suite(arguments.pop(0))
    except ValueError:
        print("usage: cli.py SUITE [--dry-run]", file=sys.stderr)
        return 2
    if arguments not in ([], ["--dry-run"]):
        print(_usage(suite), file=sys.stderr)
        return 2
    dry_run = arguments == ["--dry-run"]
    repo_root = Path(__file__).resolve().parents[2]
    run_id = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ") + f"-{os.getpid()}"
    try:
        request = request_from_environment(
            suite, repo_root, dry_run, os.environ, run_id=run_id
        )
    except ValueError as error:
        print(f"{_usage(suite)}\n{error}", file=sys.stderr)
        return 2
    _announce(suite)
    host = RecordingHost() if dry_run else SubprocessHost()
    report = execute(plan(request), host)
    for failure in report.failures:
        print(
            f"[{suite.value}] FAIL key={failure.key.case} {failure.message}",
            file=sys.stderr,
        )
    print(_summary(suite, dry_run, report.run_dir, len(report.failures)))
    return report.exit_code


if __name__ == "__main__":
    raise SystemExit(main())
