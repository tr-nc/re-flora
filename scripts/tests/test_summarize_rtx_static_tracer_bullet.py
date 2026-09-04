#!/usr/bin/env python3
"""Black-box mutation tests for the static RTX tracer-bullet summarizer."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[2]
SUMMARIZER = ROOT / "scripts" / "summarize_rtx_static_tracer_bullet.py"
RAW = ROOT / "docs" / "evidence" / "rtx_static_tracer_bullet" / "raw"
ARTIFACT_NAMES = ("static_b1.toml", "static_b2.toml")
LOG_NAMES = ("static_b1.console.log", "static_b2.console.log")
TARGET_TMP = ROOT / "target" / "tmp"
BINARY_SHA256 = "0f26d405ea2059d0d2df2f47f4daf49c87a025a53df1a2e0fdc0dff5cb4054ce"
SOURCE_HEAD = "49945c5909d8565253881b27fd33f0b4b16a8e26"


class SummarizeRtxStaticTracerBulletTests(unittest.TestCase):
    @staticmethod
    def replace_value_after(text: str, anchor: str, field: str, value: str) -> str:
        anchor_offset = text.index(anchor)
        field_offset = text.index(field, anchor_offset)
        value_offset = field_offset + len(field)
        line_end = text.index("\n", value_offset)
        return text[:value_offset] + value + text[line_end:]

    def copied_inputs(self, directory: Path) -> tuple[list[Path], list[Path]]:
        artifacts = []
        logs = []
        for name in ARTIFACT_NAMES:
            destination = directory / name
            shutil.copy2(RAW / name, destination)
            artifacts.append(destination)
        for name in LOG_NAMES:
            destination = directory / name
            shutil.copy2(RAW / name, destination)
            logs.append(destination)
        return artifacts, logs

    def mutate(self, path: Path, mutation: Callable[[str], str]) -> None:
        before = path.read_text(encoding="utf-8")
        after = mutation(before)
        self.assertNotEqual(before, after, "mutation must change the copied input")
        path.write_text(after, encoding="utf-8")

    def run_summarizer(
        self,
        artifacts: list[Path],
        logs: list[Path],
        output: Path,
    ) -> subprocess.CompletedProcess[str]:
        command = [sys.executable, str(SUMMARIZER)]
        for artifact in artifacts:
            command.extend(("--artifact", str(artifact)))
        for log in logs:
            command.extend(("--run-log", str(log)))
        command.extend(
            (
                "--binary-sha256",
                BINARY_SHA256,
                "--source-head",
                SOURCE_HEAD,
                "--output",
                str(output),
            )
        )
        environment = os.environ.copy()
        environment["PATH"] = ""
        return subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            env=environment,
        )

    def with_copied_inputs(
        self,
        assertion: Callable[[list[Path], list[Path], Path], None],
    ) -> None:
        TARGET_TMP.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(
            prefix="rtx-static-summarizer-test-",
            dir=TARGET_TMP,
        ) as temporary:
            directory = Path(temporary)
            artifacts, logs = self.copied_inputs(directory)
            assertion(artifacts, logs, directory / "summary.json")

    def assert_rejected(
        self,
        mutation: Callable[[str], str],
        expected_error: str,
    ) -> None:
        def assertion(artifacts: list[Path], logs: list[Path], output: Path) -> None:
            self.mutate(artifacts[0], mutation)
            result = self.run_summarizer(artifacts, logs, output)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())
            self.assertIn(expected_error, result.stderr)

        self.with_copied_inputs(assertion)

    def test_committed_raw_artifacts_pass(self) -> None:
        def assertion(artifacts: list[Path], logs: list[Path], output: Path) -> None:
            result = self.run_summarizer(artifacts, logs, output)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(output.exists())

        self.with_copied_inputs(assertion)

    def test_rejects_triangle_primitive_or_time_mutation(self) -> None:
        mutations = {
            "primitive-count": lambda text: self.replace_value_after(
                text, 'name = "sparse_5_percent_with_fixture"', "triangle_primitive_count = ", "1"
            ),
            "traversal-time": lambda text: self.replace_value_after(
                text, 'mode = "exposed_face_triangles"', "gpu_ms = ", "0.0"
            ),
            "blas-time": lambda text: self.replace_value_after(
                text, "[volumes.build.triangle_blas]", "gpu_build_ms = ", "0.0"
            ),
            "tlas-time": lambda text: self.replace_value_after(
                text, "[volumes.build.triangle_tlas]", "gpu_build_ms = ", "0.0"
            ),
        }
        for name, mutation in mutations.items():
            with self.subTest(name=name):
                self.assert_rejected(mutation, "triangle evidence")

    def test_rejects_aabb_candidate_confirmation_mutation(self) -> None:
        mutations = {
            "zero-candidate": lambda text: self.replace_value_after(
                text, 'mode = "voxel_aabb_exact"', "candidate_count = ", "0"
            ),
            "broken-confirmation": lambda text: self.replace_value_after(
                text, 'mode = "voxel_aabb_exact"', "confirmed_candidate_count = ", "1"
            ),
            "zero-committed": lambda text: self.replace_value_after(
                text, 'mode = "voxel_aabb_exact"', "committed_candidate_count = ", "0"
            ),
        }
        for name, mutation in mutations.items():
            with self.subTest(name=name):
                self.assert_rejected(mutation, "AABB candidate evidence")

    def test_rejects_any_correctness_exhaustion_or_disagreement(self) -> None:
        mutations = {
            "false-positive": lambda text: self.replace_value_after(
                text, 'mode = "software_dda"', "false_positive_count = ", "1"
            ),
            "wrong-face": lambda text: self.replace_value_after(
                text, 'mode = "exposed_face_triangles"', "wrong_face_count = ", "1"
            ),
            "exhaustion": lambda text: self.replace_value_after(
                text, 'mode = "voxel_aabb_exact"', "traversal_exhausted_count = ", "1"
            ),
            "committed-disagreement": lambda text: self.replace_value_after(
                text, 'mode = "voxel_aabb_exact"', "committed_disagreement_count = ", "1"
            ),
        }
        for name, mutation in mutations.items():
            with self.subTest(name=name):
                self.assert_rejected(mutation, "zero-count correctness")

    def test_requires_exactly_two_independent_artifacts_and_logs(self) -> None:
        def assertion(artifacts: list[Path], logs: list[Path], output: Path) -> None:
            duplicate = artifacts[0].with_name("static_duplicate.toml")
            shutil.copy2(artifacts[0], duplicate)
            cases = {
                "one-artifact": (artifacts[:1], logs),
                "duplicate-path": ([artifacts[0], artifacts[0]], logs),
                "same-content": ([artifacts[0], duplicate], logs),
                "one-log": (artifacts, logs[:1]),
            }
            for name, (selected_artifacts, selected_logs) in cases.items():
                with self.subTest(name=name):
                    case_output = output.with_stem(f"summary-{name}")
                    result = self.run_summarizer(selected_artifacts, selected_logs, case_output)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(case_output.exists())
                    self.assertIn("exactly two independent", result.stderr)

        self.with_copied_inputs(assertion)

    def test_rejects_missing_or_duplicate_volume(self) -> None:
        marker = "[[volumes]]"

        def missing(text: str) -> str:
            first = text.index(marker)
            second = text.index(marker, first + len(marker))
            return text[:first] + text[second:]

        def duplicate(text: str) -> str:
            first = text.index(marker)
            second = text.index(marker, first + len(marker))
            return text + "\n" + text[first:second]

        for name, mutation in {"missing": missing, "duplicate": duplicate}.items():
            with self.subTest(name=name):
                self.assert_rejected(mutation, "volume Cartesian product")

    def test_rejects_incomplete_or_out_of_order_samples(self) -> None:
        marker = "[[volumes.samples]]"

        def missing(text: str) -> str:
            first = text.index(marker)
            second = text.index(marker, first + len(marker))
            return text[:first] + text[second:]

        mutations = {
            "missing": missing,
            "wrong-index": lambda text: self.replace_value_after(
                text, marker, "order_index = ", "9"
            ),
            "wrong-mode": lambda text: self.replace_value_after(
                text, marker, "mode = ", '"voxel_aabb_exact"'
            ),
        }
        for name, mutation in mutations.items():
            with self.subTest(name=name):
                self.assert_rejected(mutation, "sample sequence")

    def test_rejects_missing_runtime_authority_marker(self) -> None:
        def assertion(artifacts: list[Path], logs: list[Path], output: Path) -> None:
            self.mutate(
                logs[0],
                lambda text: text.replace(
                    "[RTX_STATIC_TRACER_BULLET][PROTOTYPE][CAPABILITY]",
                    "[REMOVED_CAPABILITY]",
                    1,
                ),
            )
            result = self.run_summarizer(artifacts, logs, output)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())
            self.assertIn("runtime-authority", result.stderr)

        self.with_copied_inputs(assertion)


if __name__ == "__main__":
    unittest.main()
