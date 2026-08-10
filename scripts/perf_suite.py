#!/usr/bin/env python3
"""Run and compare versioned re-flora release benchmark scenarios."""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import re
import socket
import statistics
import subprocess
import sys
import tomllib
from dataclasses import asdict, dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

DEFAULT_CONFIG = Path("config/perf_scenarios.toml")
ERROR_PATTERN = re.compile(
    r"validation error|\bERROR\b|panic|panicked|device lost|VUID-",
    re.IGNORECASE,
)
GPU_FRAME_PATTERN = re.compile(r"\[PERF\]\[GPU_FRAME_SCOPE\] frame (\d+)(.*)")
CPU_FRAME_PATTERN = re.compile(r"\[PERF\]\[CPU_FRAME_SCOPE\] frame (\d+)(.*)")
GPU_SCOPE_PATTERN = re.compile(r"(?:^|\s)([A-Za-z0-9_.-]+)=([0-9]+(?:\.[0-9]+)?)us")
GPU_JOB_PATTERN = re.compile(
    r"\[PERF\]\[GPU_JOB_SCOPE\] name=([A-Za-z0-9_.-]+).*duration=([0-9]+(?:\.[0-9]+)?)us"
)
SURFACE_PASS_PATTERN = re.compile(
    r"\[PERF\]\[SURFACE_BUILD_PASS_TIMING\].*pass_total=[0-9.]+ms (.*)"
)
PASS_VALUE_PATTERN = re.compile(r"([A-Za-z0-9_.-]+)=([0-9]+(?:\.[0-9]+)?)ms")
TREE_BENCH_PATTERN = re.compile(
    r"\[PERF\]\[TREE_BENCH\].*\s([A-Za-z0-9_.-]+)\s+([0-9]+(?:\.[0-9]+)?)ms"
)
SURFACE_WORKLOAD_PATTERN = re.compile(
    r"\[PERF\]\[SURFACE_BUILD\] chunk (UVec3\([^)]*\)).*"
    r"active_voxels (\d+) active_bricks (\d+) solid_workgroups (\d+)"
)
GPU_NAME_PATTERN = re.compile(r"Selected physical device: (.+)")


@dataclass(frozen=True)
class Summary:
    samples: int
    mean_us: float
    median_us: float
    p95_us: float
    stddev_us: float
    variance_us2: float
    min_us: float
    max_us: float


@dataclass(frozen=True)
class MetricConfig:
    name: str
    source: str
    key: str
    min_samples: int
    budget_percent: float


@dataclass(frozen=True)
class Scenario:
    name: str
    description: str
    args: tuple[str, ...]
    env: dict[str, str]
    warmup_frame: int
    required_markers: tuple[str, ...]
    match_surface_workload: bool
    metrics: tuple[MetricConfig, ...]


@dataclass(frozen=True)
class ComparisonRow:
    metric: str
    baseline: Summary
    candidate: Summary
    median_delta_us: float
    median_delta_percent: float
    p95_delta_percent: float
    budget_percent: float
    regression: bool


def percentile(values: list[float], fraction: float) -> float:
    ordered = sorted(values)
    rank = (len(ordered) - 1) * fraction
    low = math.floor(rank)
    high = math.ceil(rank)
    if low == high:
        return ordered[low]
    weight = rank - low
    return ordered[low] * (1.0 - weight) + ordered[high] * weight


def summarize(values: list[float]) -> Summary:
    if not values:
        raise ValueError("cannot summarize an empty sample set")
    return Summary(
        samples=len(values),
        mean_us=statistics.fmean(values),
        median_us=statistics.median(values),
        p95_us=percentile(values, 0.95),
        stddev_us=statistics.pstdev(values),
        variance_us2=statistics.pvariance(values),
        min_us=min(values),
        max_us=max(values),
    )


def percent_delta(baseline: float, candidate: float) -> float:
    if baseline == 0.0:
        raise ValueError("cannot calculate a percentage delta from zero")
    return (candidate - baseline) / baseline * 100.0


def load_config(path: Path) -> tuple[int, dict[str, Scenario]]:
    with path.open("rb") as file:
        root = tomllib.load(file)
    version = int(root.get("version", 0))
    if version != 1:
        raise ValueError(f"unsupported performance scenario config version {version}")

    scenarios: dict[str, Scenario] = {}
    for name, raw in root.get("scenario", {}).items():
        raw_metrics = raw.get("metric", [])
        metrics = tuple(
            MetricConfig(
                name=str(metric["name"]),
                source=str(metric["source"]),
                key=str(metric["key"]),
                min_samples=int(metric.get("min_samples", 1)),
                budget_percent=float(metric.get("budget_percent", 0.0)),
            )
            for metric in raw_metrics
        )
        if not metrics:
            raise ValueError(f"scenario {name!r} defines no metrics")
        scenarios[name] = Scenario(
            name=name,
            description=str(raw.get("description", "")),
            args=tuple(str(value) for value in raw.get("args", [])),
            env={str(key): str(value) for key, value in raw.get("env", {}).items()},
            warmup_frame=int(raw.get("warmup_frame", 0)),
            required_markers=tuple(str(value) for value in raw.get("required_markers", [])),
            match_surface_workload=bool(raw.get("match_surface_workload", False)),
            metrics=metrics,
        )
    return version, scenarios


def parse_samples(log_text: str, scenario: Scenario) -> dict[str, list[float]]:
    by_source_key: dict[tuple[str, str], list[float]] = {}

    def add(source: str, key: str, value_us: float) -> None:
        by_source_key.setdefault((source, key), []).append(value_us)

    for line in log_text.splitlines():
        if frame_match := GPU_FRAME_PATTERN.search(line):
            if int(frame_match.group(1)) >= scenario.warmup_frame:
                for key, value in GPU_SCOPE_PATTERN.findall(frame_match.group(2)):
                    add("gpu_scope", key, float(value))

        if frame_match := CPU_FRAME_PATTERN.search(line):
            if int(frame_match.group(1)) >= scenario.warmup_frame:
                for key, value in GPU_SCOPE_PATTERN.findall(frame_match.group(2)):
                    add("cpu_scope", key, float(value))

        if job_match := GPU_JOB_PATTERN.search(line):
            add("gpu_job_scope", job_match.group(1), float(job_match.group(2)))

        if pass_match := SURFACE_PASS_PATTERN.search(line):
            for key, value in PASS_VALUE_PATTERN.findall(pass_match.group(1)):
                add("surface_pass", key, float(value) * 1000.0)

        if "[PERF][TREE_BENCH]" in line:
            for key, value in TREE_BENCH_PATTERN.findall(line):
                add("tree_bench", key, float(value) * 1000.0)

    return {
        metric.name: by_source_key.get((metric.source, metric.key), [])
        for metric in scenario.metrics
    }


def parse_surface_workload(log_text: str) -> list[dict[str, Any]]:
    return [
        {
            "chunk": match.group(1),
            "active_voxels": int(match.group(2)),
            "active_bricks": int(match.group(3)),
            "solid_workgroups": int(match.group(4)),
        }
        for match in SURFACE_WORKLOAD_PATTERN.finditer(log_text)
    ]


def git_value(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        check=False,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip() if result.returncode == 0 else "unknown"


def validate_log(log_text: str, scenario: Scenario) -> None:
    diagnostics = [line for line in log_text.splitlines() if ERROR_PATTERN.search(line)]
    if diagnostics:
        excerpt = "\n".join(diagnostics[:20])
        raise ValueError(f"run contains fatal or validation diagnostics:\n{excerpt}")
    missing = [marker for marker in scenario.required_markers if marker not in log_text]
    if missing:
        raise ValueError(f"run is missing required markers: {', '.join(missing)}")


def make_report(
    *,
    root: Path,
    config_path: Path,
    config_version: int,
    scenario: Scenario,
    label: str,
    binary: Path,
    command: list[str],
    log_path: Path,
    log_text: str,
) -> dict[str, Any]:
    validate_log(log_text, scenario)
    samples = parse_samples(log_text, scenario)
    metric_reports: dict[str, Any] = {}
    for metric in scenario.metrics:
        values = samples[metric.name]
        if len(values) < metric.min_samples:
            raise ValueError(
                f"metric {metric.name!r} has {len(values)} samples; "
                f"requires at least {metric.min_samples}"
            )
        metric_reports[metric.name] = {
            "unit": "us",
            "source": metric.source,
            "key": metric.key,
            "budget_percent": metric.budget_percent,
            "samples": values,
            "summary": asdict(summarize(values)),
        }

    gpu_match = GPU_NAME_PATTERN.search(log_text)
    return {
        "schema_version": 1,
        "scenario_config_version": config_version,
        "scenario_config": str(config_path),
        "scenario": scenario.name,
        "description": scenario.description,
        "label": label,
        "timestamp_utc": datetime.now(UTC).isoformat(),
        "environment": {
            "commit": git_value(root, "rev-parse", "HEAD"),
            "dirty": bool(git_value(root, "status", "--short")),
            "platform": platform.platform(),
            "python": platform.python_version(),
            "hostname": socket.gethostname(),
            "gpu": gpu_match.group(1).strip() if gpu_match else "unknown",
            "binary": str(binary),
            "command": command,
        },
        "log": str(log_path),
        "workload": parse_surface_workload(log_text)
        if scenario.match_surface_workload
        else [],
        "metrics": metric_reports,
    }


def executable_name() -> str:
    return "re-flora.exe" if os.name == "nt" else "re-flora"


def build_release(root: Path, features: str | None) -> Path:
    command = ["cargo", "build", "--release"]
    if features:
        command.extend(["--features", features])
    subprocess.run(command, cwd=root, check=True)
    return (root / "target" / "release" / executable_name()).resolve()


def runtime_library_environment(binary: Path) -> tuple[str, str]:
    target_root = binary.parent.parent
    library_dirs = [binary.parent / "deps"]
    library_dirs.extend(sorted((target_root / "release" / "build").glob("*/out/lib")))
    slang_lib = os.environ.get("SLANG_LIB")
    if slang_lib:
        library_dirs.append(Path(slang_lib).resolve().parent)

    environment_key = "PATH" if os.name == "nt" else "DYLD_LIBRARY_PATH" if sys.platform == "darwin" else "LD_LIBRARY_PATH"
    existing = os.environ.get(environment_key, "")
    values = [str(path) for path in library_dirs if path.is_dir()]
    if existing:
        values.append(existing)
    return environment_key, os.pathsep.join(values)


def run_binary(
    *,
    root: Path,
    scenario: Scenario,
    binary: Path,
    log_path: Path,
    extra_args: list[str],
) -> tuple[list[str], str]:
    command = [str(binary), *scenario.args, *extra_args]
    environment = os.environ.copy()
    environment.update(scenario.env)
    library_key, library_value = runtime_library_environment(binary)
    if library_value:
        environment[library_key] = library_value
    result = subprocess.run(
        command,
        cwd=root,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    log_text = result.stdout + result.stderr
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text(log_text, encoding="utf-8")
    if result.returncode != 0:
        raise RuntimeError(
            f"benchmark exited with status {result.returncode}; log: {log_path}"
        )
    return command, log_text


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def print_report(report: dict[str, Any]) -> None:
    print(f"scenario={report['scenario']} label={report['label']} gpu={report['environment']['gpu']}")
    for name, metric in report["metrics"].items():
        summary = metric["summary"]
        print(
            f"{name:32} n={summary['samples']:4d} "
            f"median={summary['median_us']:9.2f}us "
            f"p95={summary['p95_us']:9.2f}us "
            f"sd={summary['stddev_us']:9.2f}us "
            f"range={summary['min_us']:.2f}..{summary['max_us']:.2f}us"
        )


def read_report(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def combined_metric(report_paths: list[Path], metric: str) -> list[float]:
    values: list[float] = []
    for path in report_paths:
        report = read_report(path)
        values.extend(float(value) for value in report["metrics"][metric]["samples"])
    return values


def validate_comparable(baselines: list[Path], candidates: list[Path]) -> tuple[str, list[str]]:
    paths = [*baselines, *candidates]
    if not paths:
        raise ValueError("no reports supplied")
    reports = [read_report(path) for path in paths]
    scenario = reports[0]["scenario"]
    metric_names = list(reports[0]["metrics"])
    workload = reports[0].get("workload", [])
    for path, report in zip(paths[1:], reports[1:], strict=True):
        if report["scenario"] != scenario:
            raise ValueError(f"scenario mismatch in {path}")
        if list(report["metrics"]) != metric_names:
            raise ValueError(f"metric mismatch in {path}")
        if report.get("workload", []) != workload:
            raise ValueError(f"workload mismatch in {path}")
    return scenario, metric_names


def compare_reports(
    baselines: list[Path], candidates: list[Path]
) -> tuple[str, list[ComparisonRow]]:
    scenario, metric_names = validate_comparable(baselines, candidates)
    first_report = read_report(baselines[0])
    rows: list[ComparisonRow] = []
    for metric in metric_names:
        baseline = summarize(combined_metric(baselines, metric))
        candidate = summarize(combined_metric(candidates, metric))
        budget = float(first_report["metrics"][metric]["budget_percent"])
        median_delta_percent = percent_delta(baseline.median_us, candidate.median_us)
        rows.append(
            ComparisonRow(
                metric=metric,
                baseline=baseline,
                candidate=candidate,
                median_delta_us=candidate.median_us - baseline.median_us,
                median_delta_percent=median_delta_percent,
                p95_delta_percent=percent_delta(baseline.p95_us, candidate.p95_us),
                budget_percent=budget,
                regression=median_delta_percent > budget,
            )
        )
    return scenario, rows


def print_comparison(scenario: str, rows: list[ComparisonRow]) -> None:
    print(f"scenario={scenario}")
    print("metric                           baseline     candidate       median Δ       p95 Δ  budget")
    for row in rows:
        status = "REGRESSION" if row.regression else "ok"
        print(
            f"{row.metric:32} {row.baseline.median_us:9.2f}us "
            f"{row.candidate.median_us:9.2f}us "
            f"{row.median_delta_percent:+8.2f}% "
            f"{row.p95_delta_percent:+8.2f}% "
            f"{row.budget_percent:5.1f}% {status}"
        )


def run_one(args: argparse.Namespace) -> int:
    root = args.root.resolve()
    config_path = (root / args.config).resolve()
    version, scenarios = load_config(config_path)
    scenario = scenarios[args.scenario]
    binary = args.binary.resolve() if args.binary else build_release(root, args.features)
    output = args.output.resolve()
    log_path = output.with_suffix(".log")
    command, log_text = run_binary(
        root=root,
        scenario=scenario,
        binary=binary,
        log_path=log_path,
        extra_args=args.extra_arg,
    )
    report = make_report(
        root=root,
        config_path=config_path,
        config_version=version,
        scenario=scenario,
        label=args.label,
        binary=binary,
        command=command,
        log_path=log_path,
        log_text=log_text,
    )
    write_json(output, report)
    print_report(report)
    print(f"report={output}\nlog={log_path}")
    return 0


def compare_command(args: argparse.Namespace) -> int:
    scenario, rows = compare_reports(args.baseline, args.candidate)
    print_comparison(scenario, rows)
    if args.output:
        write_json(
            args.output,
            {
                "schema_version": 1,
                "scenario": scenario,
                "baseline_reports": [str(path) for path in args.baseline],
                "candidate_reports": [str(path) for path in args.candidate],
                "rows": [asdict(row) for row in rows],
            },
        )
    return int(not args.allow_regression and any(row.regression for row in rows))


def run_ab(args: argparse.Namespace) -> int:
    root = args.root.resolve()
    config_path = (root / args.config).resolve()
    version, scenarios = load_config(config_path)
    scenario = scenarios[args.scenario]
    output_dir = args.output_dir.resolve()
    binaries = {
        "A": (args.baseline_binary.resolve(), "baseline"),
        "B": (args.candidate_binary.resolve(), "candidate"),
    }
    order = [item.strip().upper() for item in args.order.split(",")]
    if not order or any(item not in binaries for item in order):
        raise ValueError("--order must be a comma-separated sequence of A and B")

    report_paths: dict[str, list[Path]] = {"A": [], "B": []}
    for index, item in enumerate(order, start=1):
        binary, label = binaries[item]
        stem = f"{index:02d}-{item.lower()}-{label}"
        report_path = output_dir / f"{stem}.json"
        log_path = output_dir / f"{stem}.log"
        command, log_text = run_binary(
            root=root,
            scenario=scenario,
            binary=binary,
            log_path=log_path,
            extra_args=args.extra_arg,
        )
        report = make_report(
            root=root,
            config_path=config_path,
            config_version=version,
            scenario=scenario,
            label=label,
            binary=binary,
            command=command,
            log_path=log_path,
            log_text=log_text,
        )
        write_json(report_path, report)
        report_paths[item].append(report_path)
        print_report(report)

    scenario_name, rows = compare_reports(report_paths["A"], report_paths["B"])
    print_comparison(scenario_name, rows)
    comparison_path = output_dir / "comparison.json"
    write_json(
        comparison_path,
        {
            "schema_version": 1,
            "scenario": scenario_name,
            "order": order,
            "baseline_reports": [str(path) for path in report_paths["A"]],
            "candidate_reports": [str(path) for path in report_paths["B"]],
            "rows": [asdict(row) for row in rows],
        },
    )
    print(f"comparison={comparison_path}")
    return int(not args.allow_regression and any(row.regression for row in rows))


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--root", type=Path, default=Path.cwd(), help="project root")
    result.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    subparsers = result.add_subparsers(dest="command", required=True)

    run_parser = subparsers.add_parser("run", help="build and run one scenario")
    run_parser.add_argument("scenario")
    run_parser.add_argument("--output", type=Path, required=True)
    run_parser.add_argument("--label", default="run")
    run_parser.add_argument("--features", help="Cargo feature list")
    run_parser.add_argument("--binary", type=Path, help="use an existing release binary")
    run_parser.add_argument("--extra-arg", action="append", default=[])
    run_parser.set_defaults(handler=run_one)

    compare_parser = subparsers.add_parser("compare", help="compare existing JSON reports")
    compare_parser.add_argument("--baseline", type=Path, action="append", required=True)
    compare_parser.add_argument("--candidate", type=Path, action="append", required=True)
    compare_parser.add_argument("--output", type=Path)
    compare_parser.add_argument("--allow-regression", action="store_true")
    compare_parser.set_defaults(handler=compare_command)

    ab_parser = subparsers.add_parser("run-ab", help="run prebuilt binaries in A/B order")
    ab_parser.add_argument("scenario")
    ab_parser.add_argument("--baseline-binary", type=Path, required=True)
    ab_parser.add_argument("--candidate-binary", type=Path, required=True)
    ab_parser.add_argument("--output-dir", type=Path, required=True)
    ab_parser.add_argument("--order", default="A,B,B,A")
    ab_parser.add_argument("--extra-arg", action="append", default=[])
    ab_parser.add_argument("--allow-regression", action="store_true")
    ab_parser.set_defaults(handler=run_ab)
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        return int(args.handler(args))
    except (KeyError, OSError, RuntimeError, subprocess.SubprocessError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
