#!/usr/bin/env python3
"""Validate the committed before/after DDGI sky-normalization evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import subprocess
from pathlib import Path
from typing import Any

from analyze_environment_irradiance_capture import (
    compare_reference,
    load_capture,
    summarize,
)


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_EVIDENCE = REPO_ROOT / "docs/evidence/ddgi_sky_normalization.json"
BEFORE_COMMIT = "3b731b6e73a6412107bcb28eb74a741930e4e1bf"
AFTER_COMMIT = "91e23918d9f65b33e0cffa928d0418d69aa038ec"
EXPECTED_SUBJECTS = {
    "before": "specify DDGI indirect transport",
    "after": "normalize DDGI diffuse irradiance energy",
}
EXPECTED_CHANGED_FILES = [
    "shader/slang/ddgi_global_sky_filter.slang",
    "shader/slang/ddgi_irradiance_filter.slang",
    "shader/slang/ddgi_probe_trace.slang",
]
RUNTIME_TRANSPORT_SYMBOLS = [
    "DdgiFieldStage",
    "DdgiFieldKey",
    "environment_irradiance_capture_target",
]
SPACINGS = (32, 16)
MAX_CHANNEL_ERROR = 1.0e-6
MAX_LUMINANCE_ERROR = 1.0e-6
COMMAND_TEMPLATE = [
    "cargo",
    "run",
    "--release",
    "--",
    "--hidden",
    "--mute",
    "--environment-lighting-test-scene",
    "portal",
    "--environment-probe-spacing-voxels",
    "{spacing}",
    "--environment-irradiance-capture",
    "{capture}",
    "--auto-exit",
    "12",
]
AUTHORED_SCENE_MARKER = (
    "[ENV_LIGHT_TEST] case=portal camera position=(0.650,0.520,1.380) "
    "target=(0.650,0.780,1.100) time_of_day=0.455705 auto_cycle=false "
    "voxel_color_variance=0.000"
)
ERROR_MARKER = re.compile(
    r"(^|[^A-Za-z])(ERROR|panic|VUID-|validation error|stale readback)",
    re.IGNORECASE | re.MULTILINE,
)
SHA256 = re.compile(r"[0-9a-f]{64}")


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _capture_record(path: Path) -> dict[str, Any]:
    capture = load_capture(path)
    summary = summarize(capture)
    return {
        "path": str(path),
        "file_sha256": file_sha256(path),
        "payload_sha256": summary["payload_sha256"],
        "version": summary["version"],
        "width": summary["width"],
        "height": summary["height"],
        "backend": summary["backend"],
        "spacing_voxels": summary["spacing_voxels"],
        "debug_view": summary["debug_view"],
        "sample_count": summary["sample_count"],
        "terrain_hit_count": summary["terrain_hit_count"],
        "finite": summary["finite"],
        "luminance_mean": summary["luminance_mean"],
        "luminance_p99": summary["luminance_p99"],
        "luminance_max": summary["luminance_max"],
    }


def collect_case(artifact_root: Path, spacing: int) -> dict[str, Any]:
    names = {
        label: f"{label}/portal-spacing{spacing}-sky-only.rfirr"
        for label in ("before", "after")
    }
    captures = {
        label: load_capture(artifact_root / relative)
        for label, relative in names.items()
    }
    records = {
        label: _capture_record(artifact_root / relative)
        for label, relative in names.items()
    }
    for record, relative in zip(records.values(), names.values()):
        record["path"] = relative
    comparison = compare_reference(captures["after"], captures["before"])
    console = {}
    for label in ("before", "after"):
        relative = f"{label}/portal-spacing{spacing}-sky-only.console.log"
        console[label] = {
            "path": relative,
            "file_sha256": file_sha256(artifact_root / relative),
        }
    passed = bool(
        comparison["compatible"]
        and comparison["hit_mask_matches"]
        and comparison["channel_error_max"] <= MAX_CHANNEL_ERROR
        and comparison["luminance_error_max"] <= MAX_LUMINANCE_ERROR
    )
    return {
        "spacing_voxels": spacing,
        "captures": records,
        "console_logs": console,
        "comparison": comparison,
        "result": "pass" if passed else "fail",
    }


def _same_number(expected: object, actual: object) -> bool:
    if isinstance(expected, bool) or isinstance(actual, bool):
        return expected == actual
    if isinstance(expected, (int, float)) and isinstance(actual, (int, float)):
        return math.isfinite(float(expected)) and math.isfinite(float(actual)) and math.isclose(
            float(expected), float(actual), rel_tol=1.0e-12, abs_tol=1.0e-15
        )
    return expected == actual


def _compare_recorded(
    failures: list[str], prefix: str, recorded: object, actual: object
) -> None:
    if isinstance(recorded, dict) and isinstance(actual, dict):
        if set(recorded) != set(actual):
            failures.append(
                f"{prefix}: keys differ recorded={sorted(recorded)} actual={sorted(actual)}"
            )
            return
        for key in recorded:
            _compare_recorded(failures, f"{prefix}.{key}", recorded[key], actual[key])
        return
    if isinstance(recorded, list) and isinstance(actual, list):
        if len(recorded) != len(actual):
            failures.append(f"{prefix}: list lengths differ")
            return
        for index, (expected_item, actual_item) in enumerate(zip(recorded, actual)):
            _compare_recorded(
                failures, f"{prefix}[{index}]", expected_item, actual_item
            )
        return
    if not _same_number(recorded, actual):
        failures.append(f"{prefix}: recorded={recorded!r} actual={actual!r}")


def _run_git(repo_root: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(repo_root), *arguments],
        check=False,
        capture_output=True,
        text=True,
    )


def validate_evidence(
    evidence: dict[str, Any],
    artifact_root: Path | None,
    repo_root: Path | None = REPO_ROOT,
    *,
    verify_git: bool = True,
) -> list[str]:
    failures: list[str] = []
    if evidence.get("schema_version") != 1:
        failures.append("schema_version must be 1")

    git_record = evidence.get("git", {})
    if git_record.get("before_commit") != BEFORE_COMMIT:
        failures.append("before_commit is not the audited normalization parent")
    if git_record.get("after_commit") != AFTER_COMMIT:
        failures.append("after_commit is not the audited normalization commit")
    if git_record.get("subjects") != EXPECTED_SUBJECTS:
        failures.append("commit subjects differ from the audited pair")
    if git_record.get("changed_files") != EXPECTED_CHANGED_FILES:
        failures.append("changed_files differ from the adjacent normalization diff")
    if git_record.get("runtime_transport_symbols_absent") != RUNTIME_TRANSPORT_SYMBOLS:
        failures.append("runtime transport absence contract is incomplete")

    capture_contract = evidence.get("capture_contract", {})
    if capture_contract.get("field") != "pre-transport-sky-only":
        failures.append("capture field must be pre-transport-sky-only")
    if capture_contract.get("spacings_voxels") != list(SPACINGS):
        failures.append("capture spacings must be exactly [32, 16]")
    if capture_contract.get("command_template") != COMMAND_TEMPLATE:
        failures.append("capture command template is not the audited release command")
    if capture_contract.get("authored_scene_marker") != AUTHORED_SCENE_MARKER:
        failures.append("authored scene/camera settings changed")

    thresholds = evidence.get("hard_thresholds", {})
    if thresholds != {
        "channel_error_max": MAX_CHANNEL_ERROR,
        "luminance_error_max": MAX_LUMINANCE_ERROR,
        "hit_mask_matches": True,
    }:
        failures.append("hard thresholds differ from the committed acceptance gate")

    cases = evidence.get("cases")
    if not isinstance(cases, list) or [case.get("spacing_voxels") for case in cases] != list(SPACINGS):
        failures.append("evidence cases must cover spacing 32 then 16 exactly once")
        cases = []
    for case in cases:
        spacing = case["spacing_voxels"]
        comparison = case.get("comparison", {})
        valid_metrics: dict[str, float] = {}
        for field in (
            "channel_error_p99",
            "channel_error_max",
            "luminance_error_mean",
            "luminance_error_p99",
            "luminance_error_max",
        ):
            value = comparison.get(field)
            if not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
                failures.append(f"spacing {spacing}: invalid {field}")
            else:
                valid_metrics[field] = float(value)
        if comparison.get("compatible") is not True:
            failures.append(f"spacing {spacing}: captures are incompatible")
        if comparison.get("hit_mask_matches") is not True:
            failures.append(f"spacing {spacing}: hit masks differ")
        if valid_metrics.get("channel_error_max", math.inf) > MAX_CHANNEL_ERROR:
            failures.append(f"spacing {spacing}: channel_error_max exceeds hard threshold")
        if valid_metrics.get("luminance_error_max", math.inf) > MAX_LUMINANCE_ERROR:
            failures.append(f"spacing {spacing}: luminance_error_max exceeds hard threshold")
        if case.get("result") != "pass":
            failures.append(f"spacing {spacing}: recorded result is not pass")
        captures = case.get("captures", {})
        for label in ("before", "after"):
            record = captures.get(label, {})
            if record.get("spacing_voxels") != spacing:
                failures.append(f"spacing {spacing}: {label} header spacing differs")
            if record.get("version") != 2 or record.get("debug_view") != "final":
                failures.append(f"spacing {spacing}: {label} is not a v2 final-view capture")
            if record.get("finite") is not True:
                failures.append(f"spacing {spacing}: {label} contains non-finite values")
            for hash_field in ("file_sha256", "payload_sha256"):
                if not SHA256.fullmatch(str(record.get(hash_field, ""))):
                    failures.append(f"spacing {spacing}: invalid {label} {hash_field}")
        if captures.get("before", {}).get("terrain_hit_count") != captures.get("after", {}).get("terrain_hit_count"):
            failures.append(f"spacing {spacing}: terrain hit counts differ")

        if artifact_root is not None:
            try:
                actual = collect_case(artifact_root, spacing)
            except (OSError, ValueError) as error:
                failures.append(f"spacing {spacing}: cannot load artifacts: {error}")
                continue
            _compare_recorded(failures, f"spacing {spacing}", case, actual)
            for label in ("before", "after"):
                console_record = case["console_logs"][label]
                console_path = artifact_root / console_record["path"]
                try:
                    console_text = console_path.read_text(errors="replace")
                except OSError as error:
                    failures.append(f"spacing {spacing}: cannot read {label} console: {error}")
                    continue
                if AUTHORED_SCENE_MARKER not in console_text:
                    failures.append(f"spacing {spacing}: {label} console lacks authored scene marker")
                expected_capture_marker = (
                    f"backend=ddgi spacing_voxels={spacing} view=final samples=178688 "
                    "format=float4-linear-rgb-hit"
                )
                if expected_capture_marker not in console_text:
                    failures.append(f"spacing {spacing}: {label} console lacks capture marker")
                if ERROR_MARKER.search(console_text):
                    failures.append(f"spacing {spacing}: {label} console contains error marker")

    if evidence.get("overall_result") != "pass":
        failures.append("overall_result must be pass")

    if verify_git:
        if repo_root is None:
            failures.append("repo_root is required for git verification")
        else:
            parent = _run_git(repo_root, "rev-parse", f"{AFTER_COMMIT}^")
            if parent.returncode != 0 or parent.stdout.strip() != BEFORE_COMMIT:
                failures.append("normalization commits are not adjacent")
            subjects = {
                label: _run_git(repo_root, "show", "-s", "--format=%s", commit)
                for label, commit in (("before", BEFORE_COMMIT), ("after", AFTER_COMMIT))
            }
            for label, result in subjects.items():
                if result.returncode != 0 or result.stdout.strip() != EXPECTED_SUBJECTS[label]:
                    failures.append(f"git subject mismatch for {label}")
            changed = _run_git(
                repo_root, "diff", "--name-only", BEFORE_COMMIT, AFTER_COMMIT
            )
            if changed.returncode != 0 or changed.stdout.splitlines() != EXPECTED_CHANGED_FILES:
                failures.append("live adjacent diff does not match recorded shader-only files")
            for commit in (BEFORE_COMMIT, AFTER_COMMIT):
                for symbol in RUNTIME_TRANSPORT_SYMBOLS:
                    grep = _run_git(
                        repo_root, "grep", "-F", symbol, commit, "--", "src", "shader"
                    )
                    if grep.returncode == 0:
                        failures.append(f"{commit[:8]} unexpectedly contains runtime symbol {symbol}")
                    elif grep.returncode != 1:
                        failures.append(f"git grep failed for {commit[:8]} symbol {symbol}")

    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("evidence", nargs="?", type=Path, default=DEFAULT_EVIDENCE)
    parser.add_argument(
        "--artifact-root",
        type=Path,
        help="recompute hashes and metrics from uncommitted local .rfirr/log artifacts",
    )
    parser.add_argument("--skip-git", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()

    try:
        evidence = json.loads(args.evidence.read_text())
    except (OSError, json.JSONDecodeError) as error:
        print(json.dumps({"pass": False, "failures": [str(error)]}, indent=2))
        return 1
    failures = validate_evidence(
        evidence,
        args.artifact_root,
        REPO_ROOT,
        verify_git=not args.skip_git,
    )
    print(
        json.dumps(
            {
                "pass": not failures,
                "evidence": str(args.evidence),
                "artifact_root": str(args.artifact_root) if args.artifact_root else None,
                "failures": failures,
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
