#!/usr/bin/env python3
"""Capture permanent thin-terrain lighting or measure repeated Release runs.

Build first: cargo build --release
Examples:
  python3 scripts/check_terrain_hybrid_lighting.py --output target/terrain-hybrid/visual
  python3 scripts/check_terrain_hybrid_lighting.py --irradiance --output target/terrain-hybrid/light
  python3 scripts/check_terrain_hybrid_lighting.py --benchmark --runs 4 --output target/terrain-hybrid/perf
Captures are named hybrid.png / hybrid.rfirr. Linear captures use uniform rock
albedo for a self-contained energy check. The GPU lock covers configuration and
runs; saved configuration is restored. There is no longer an off mode or --order.
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
DEFAULT_RUNS = 4


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
        if irradiance:
            # All identified receivers are rock. Equal albedo lets the broad
            # control cancel material/solar color without retaining an old renderer.
            source = setting(source, "terrain_rock_strength", "0.0")
        gui.write_text(source)
        (out / "hybrid.gui.toml").write_bytes(gui.read_bytes())
        artifact = out / f"hybrid.{'rfirr' if irradiance else 'png'}"
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
        # One-shot irradiance capture must not preempt a delayed screenshot.
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
        perf_suite.write_json(out / "hybrid-command.json", command)
        with (out / "hybrid.log").open("w") as log:
            subprocess.run(
                command,
                cwd=ROOT,
                stdout=log,
                stderr=subprocess.STDOUT,
                check=True,
                timeout=120,
            )
        validate_capture_log((out / "hybrid.log").read_text())
        if not artifact.is_file():
            raise RuntimeError("missing capture output; inspect hybrid.log")
        print(artifact, flush=True)


def render_extents(text: str) -> dict[str, str]:
    surface = re.findall(r"\[RESIZE\] published[^\n]*?extent=(\d+x\d+)", text)
    scene = re.findall(r"\[GOD_RAY\]\[RESOURCES\] scene=(\d+x\d+)", text)
    if not surface or not scene:
        raise ValueError("missing actual render extents; cannot validate workload")
    return {"surface": surface[-1], "scene": scene[-1]}


def benchmark(binary: Path, out: Path, selected: list[str], runs: int):
    config = ROOT / "config/perf_scenarios.toml"
    version, scenarios = perf_suite.load_config(config)
    summaries = {}
    with fixed_configuration() as (gui, source):
        gui.write_text(source)
        for name in selected:
            scenario = scenarios[name]
            paths = []
            expected_extents = None
            for index in range(runs):
                stem = out / f"{name}-{index}-hybrid"
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
                    label="hybrid",
                    binary=binary,
                    command=command,
                    log_path=stem.with_suffix(".log"),
                    log_text=text,
                )
                extents = render_extents(text)
                if expected_extents is not None and extents != expected_extents:
                    raise ValueError(
                        "render resolution changed; rerun with stable window/monitor settings"
                    )
                expected_extents = extents
                report["environment"]["render_extents"] = extents
                path = stem.with_suffix(".json")
                perf_suite.write_json(path, report)
                perf_suite.print_report(report)
                paths.append(path)
            if len(paths) > 1:
                perf_suite.validate_comparable(paths[:1], paths[1:])
            summaries[name] = {
                metric.name: asdict(
                    perf_suite.summarize(perf_suite.combined_metric(paths, metric.name))
                )
                for metric in scenario.metrics
            }
            perf_suite.write_json(out / "summary.json", summaries)
    print(out / "summary.json", flush=True)


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
        help="Measure repeated runs instead of taking captures",
    )
    parser.add_argument(
        "--irradiance",
        action="store_true",
        help="Capture linear lighting planes instead of PNGs (not with --benchmark)",
    )
    parser.add_argument(
        "--scenario",
        choices=SCENARIOS,
        action="append",
        help="Benchmark only this scenario; default: both",
    )
    parser.add_argument(
        "--runs",
        type=int,
        help=f"Repetitions; requires --benchmark (default: {DEFAULT_RUNS})",
    )
    parser.add_argument(
        "--order", help="Removed: use --benchmark --runs N, not the retired A/B order"
    )
    args = parser.parse_args()
    if args.order is not None:
        parser.error(
            "terrain hybrid lighting is permanent; --order was removed. Use --benchmark --runs 4 for repeated measurements"
        )
    if args.irradiance and args.benchmark:
        parser.error("--irradiance is for captures, not benchmark runs")
    if (args.scenario or args.runs is not None) and not args.benchmark:
        parser.error("--scenario and --runs require --benchmark")
    if args.runs is not None and args.runs < 1:
        parser.error("--runs must be positive; example: --benchmark --runs 4")
    binary, out = args.binary.resolve(), args.output.resolve()
    if not binary.is_file():
        parser.error("Release binary missing; run cargo build --release first")
    out.mkdir(parents=True, exist_ok=True)
    if args.benchmark:
        benchmark(
            binary, out, args.scenario or list(SCENARIOS), args.runs or DEFAULT_RUNS
        )
    else:
        capture(binary, out, args.irradiance)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        raise SystemExit(str(error)) from error
