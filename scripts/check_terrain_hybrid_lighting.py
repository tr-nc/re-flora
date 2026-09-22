#!/usr/bin/env python3
"""Capture ordinary-terrain thin-voxel lighting A/B, or measure Release on/off cost.

Build first: cargo build --release
Examples:
  python3 scripts/check_terrain_hybrid_lighting.py --output target/terrain-hybrid/visual
  python3 scripts/check_terrain_hybrid_lighting.py --irradiance --output target/terrain-hybrid/light
  python3 scripts/check_terrain_hybrid_lighting.py --benchmark --output target/terrain-hybrid/perf
The GPU lock covers GUI changes and runs. Saved configuration is always restored.
"""

from __future__ import annotations

import argparse
import fcntl
import re
import subprocess
from contextlib import contextmanager
from dataclasses import asdict
from pathlib import Path

import perf_suite
from check_raster_tree_static import setting

ROOT = Path(__file__).resolve().parents[1]
SCENARIOS = ("terrain-hybrid-player", "terrain-hybrid-thin")


@contextmanager
def fixed_configuration():
    gui = ROOT / "config/gui.toml"
    with open("/tmp/re-flora-summer-gpu.lock", "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        original = gui.read_bytes()
        try:
            source = original.decode()
            for name, value in (
                ("auto_daynight_cycle", "false"),
                ("time_of_day", "0.47"),
                ("path_tracing_reference", "false"),
                ("raster_tree_wind", "false"),
            ):
                source = setting(source, name, value)
            yield gui, source
        finally:
            gui.write_bytes(original)


def validate_capture_log(text: str):
    if "Application exited successfully" not in text:
        raise RuntimeError("capture did not finish successfully; inspect its log")
    if any(marker in text for marker in (" ERROR ", "VUID-", "panicked at")):
        raise RuntimeError("capture contains rendering errors; inspect its log")


def capture(binary: Path, out: Path, irradiance: bool):
    with fixed_configuration() as (gui, source):
        for mode in ("A", "B"):
            gui.write_text(
                setting(source, "terrain_hybrid_lighting", str(mode == "B").lower())
            )
            artifact = out / f"{mode}.{'rfirr' if irradiance else 'png'}"
            artifact.unlink(missing_ok=True)
            command = [
                str(binary),
                "--hidden",
                "--mute",
                "--windowed",
                "--no-flora",
                "--no-particles",
                "--no-clouds",
                "--no-god-rays",
                "--no-lens-flare",
                "--ddgi-debug-view",
                "final",
                "--environment-lighting-test-scene",
                "thin-voxels",
                "--auto-exit",
                "7",
            ]
            # Irradiance capture is a one-shot app mode and exits immediately
            # on completion, so it must not share a run with a delayed screenshot.
            if irradiance:
                command += [
                    "--environment-irradiance-capture",
                    str(artifact),
                    "--environment-irradiance-capture-target",
                    "published",
                ]
            else:
                command += [
                    "--screenshot",
                    "environment-test-scene",
                    str(artifact),
                    "--screenshot-delay",
                    "4",
                ]
            perf_suite.write_json(out / f"{mode}-command.json", command)
            with (out / f"{mode}.log").open("w") as log:
                subprocess.run(
                    command,
                    cwd=ROOT,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    check=True,
                    timeout=120,
                )
            validate_capture_log((out / f"{mode}.log").read_text())
            if not artifact.is_file():
                raise RuntimeError(f"{mode}: missing capture output")
            print(artifact, flush=True)


def render_extents(text: str) -> dict[str, str]:
    surface = re.findall(r"\[RESIZE\] published[^\n]*?extent=(\d+x\d+)", text)
    scene = re.findall(r"\[GOD_RAY\]\[RESOURCES\] scene=(\d+x\d+)", text)
    if not surface or not scene:
        raise ValueError("missing actual render extents; cannot validate A/B workload")
    return {"surface": surface[-1], "scene": scene[-1]}


def benchmark(binary: Path, out: Path, selected: list[str], order: list[str]):
    config = ROOT / "config/perf_scenarios.toml"
    version, scenarios = perf_suite.load_config(config)
    comparisons = {}
    with fixed_configuration() as (gui, source):
        for name in selected:
            scenario = scenarios[name]
            paths = {"A": [], "B": []}
            expected_extents = None
            for index, mode in enumerate(order):
                gui.write_text(
                    setting(source, "terrain_hybrid_lighting", str(mode == "B").lower())
                )
                stem = out / f"{name}-{index}-{mode}"
                stem.with_suffix(".gui.toml").write_bytes(gui.read_bytes())
                command, text = perf_suite.run_binary(
                    root=ROOT,
                    scenario=scenario,
                    binary=binary,
                    log_path=stem.with_suffix(".log"),
                    extra_args=[],
                )
                report = perf_suite.make_report(
                    root=ROOT,
                    config_path=config,
                    config_version=version,
                    scenario=scenario,
                    label=mode,
                    binary=binary,
                    command=command,
                    log_path=stem.with_suffix(".log"),
                    log_text=text,
                )
                extents = render_extents(text)
                if expected_extents is not None and extents != expected_extents:
                    raise ValueError(
                        "A/B render resolution changed; rerun with stable window/monitor settings"
                    )
                expected_extents = extents
                report["environment"]["render_extents"] = extents
                path = stem.with_suffix(".json")
                perf_suite.write_json(path, report)
                perf_suite.print_report(report)
                paths[mode].append(path)
            perf_suite.validate_comparable(paths["A"], paths["B"])
            rows = {}
            for metric in scenario.metrics:
                a = perf_suite.summarize(
                    perf_suite.combined_metric(paths["A"], metric.name)
                )
                b = perf_suite.summarize(
                    perf_suite.combined_metric(paths["B"], metric.name)
                )
                rows[metric.name] = {
                    "A": asdict(a),
                    "B": asdict(b),
                    "median_delta_us": b.median_us - a.median_us,
                    "median_delta_percent": perf_suite.percent_delta(
                        a.median_us, b.median_us
                    ),
                    "p95_delta_percent": perf_suite.percent_delta(a.p95_us, b.p95_us),
                }
            comparisons[name] = rows
            perf_suite.write_json(out / "comparison.json", comparisons)
    print(out / "comparison.json", flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--binary",
        type=Path,
        default=ROOT / "target/release/re-flora",
        help="Built Release binary (default: this worktree)",
    )
    parser.add_argument(
        "--output", type=Path, default=ROOT / "target/terrain-hybrid/evidence"
    )
    parser.add_argument(
        "--benchmark",
        action="store_true",
        help="Measure cost instead of taking visual captures",
    )
    parser.add_argument(
        "--irradiance",
        action="store_true",
        help="Capture linear lighting planes instead of PNGs (not for benchmark runs)",
    )
    parser.add_argument(
        "--scenario",
        choices=SCENARIOS,
        action="append",
        help="Benchmark only this scenario; default: both",
    )
    parser.add_argument(
        "--order",
        default="A,B,B,A",
        help="Benchmark order; A=off, B=on (default: A,B,B,A)",
    )
    args = parser.parse_args()
    if args.irradiance and args.benchmark:
        parser.error("--irradiance is for visual captures, not benchmark runs")
    if args.scenario and not args.benchmark:
        parser.error("--scenario requires --benchmark")
    order = args.order.split(",")
    if set(order) != {"A", "B"}:
        parser.error(
            "--order must contain both A and B, separated by commas (example: A,B,B,A)"
        )
    binary, out = args.binary.resolve(), args.output.resolve()
    if not binary.is_file():
        parser.error("Release binary missing; run cargo build --release first")
    out.mkdir(parents=True, exist_ok=True)
    if args.benchmark:
        benchmark(binary, out, args.scenario or list(SCENARIOS), order)
    else:
        capture(binary, out, args.irradiance)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        raise SystemExit(str(error)) from error
