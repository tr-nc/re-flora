#!/usr/bin/env python3
"""Black-box mutation tests for the committed RTX voxel evidence summarizer."""

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
SUMMARIZER = ROOT / "scripts" / "summarize_rtx_voxel_hardware_rt.py"
RAW = ROOT / "docs" / "evidence" / "rtx_voxel_hardware_rt" / "raw"
ARTIFACT_NAMES = ("rtx_b1.toml", "rtx_b2.toml")
FRAME_RUNS = (
    ("A1", RAW / "frame_a1.log"),
    ("B1", RAW / "frame_b1.log"),
    ("B2", RAW / "frame_b2.log"),
    ("A2", RAW / "frame_a2.log"),
)
TARGET_TMP = ROOT / "target" / "tmp"


class SummarizeRtxVoxelHardwareRtTests(unittest.TestCase):
    @staticmethod
    def replace_value_after(text: str, anchor: str, field: str, value: str) -> str:
        anchor_offset = text.index(anchor)
        field_offset = text.index(field, anchor_offset)
        value_offset = field_offset + len(field)
        line_end = text.index("\n", value_offset)
        return text[:value_offset] + value + text[line_end:]

    def copied_artifacts(self, directory: Path) -> list[Path]:
        artifacts = []
        for name in ARTIFACT_NAMES:
            destination = directory / name
            shutil.copy2(RAW / name, destination)
            artifacts.append(destination)
        return artifacts

    def mutate(
        self,
        path: Path,
        mutation: Callable[[str], str],
    ) -> None:
        before = path.read_text(encoding="utf-8")
        after = mutation(before)
        self.assertNotEqual(before, after, "mutation must change the copied artifact")
        path.write_text(after, encoding="utf-8")

    def run_summarizer(
        self,
        artifacts: list[Path],
        output: Path,
    ) -> subprocess.CompletedProcess[str]:
        command = [sys.executable, str(SUMMARIZER)]
        for artifact in artifacts:
            command.extend(("--ray-query-artifact", str(artifact)))
        for label, path in FRAME_RUNS:
            command.extend(("--frame-run", f"{label}={path}"))
        command.extend(
            (
                "--binary-a",
                sys.executable,
                "--binary-b",
                str(SUMMARIZER),
                "--tail-samples",
                "64",
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

    def with_copied_artifacts(
        self,
        assertion: Callable[[list[Path], Path], None],
    ) -> None:
        TARGET_TMP.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(
            prefix="rtx-voxel-summarizer-test-",
            dir=TARGET_TMP,
        ) as temporary:
            directory = Path(temporary)
            assertion(self.copied_artifacts(directory), directory / "summary.json")

    def test_committed_raw_artifacts_pass(self) -> None:
        def assertion(artifacts: list[Path], output: Path) -> None:
            result = self.run_summarizer(artifacts, output)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(output.exists())

        self.with_copied_artifacts(assertion)

    def test_rejects_nonzero_committed_query_disagreement(self) -> None:
        def mutation(text: str) -> str:
            hardware = text.index('mode = "hardware"')
            field = "query_committed_disagreement_count = 0"
            disagreement = text.index(field, hardware)
            return text[:disagreement] + field.replace("0", "1") + text[disagreement + len(field) :]

        def assertion(artifacts: list[Path], output: Path) -> None:
            self.mutate(artifacts[0], mutation)

            result = self.run_summarizer(artifacts, output)

            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())
            self.assertIn("committed", result.stderr)

        self.with_copied_artifacts(assertion)

    def test_rejects_zero_in_any_hardware_evidence_series(self) -> None:
        mutations = {
            "candidate": lambda text: self.replace_value_after(
                text,
                'mode = "hardware"',
                "candidate_count = ",
                "0",
            ),
            "committed-candidate": lambda text: self.replace_value_after(
                text,
                'mode = "hardware"',
                "committed_candidate_count = ",
                "0",
            ),
            "blas-gpu": lambda text: self.replace_value_after(
                text,
                "[configurations.initial.blas]",
                "gpu_build_ms = ",
                "0.0",
            ),
            "tlas-gpu": lambda text: self.replace_value_after(
                text,
                "[configurations.initial.tlas]",
                "gpu_build_ms = ",
                "0.0",
            ),
        }
        for name, mutation in mutations.items():
            with self.subTest(name=name):

                def assertion(artifacts: list[Path], output: Path) -> None:
                    self.mutate(artifacts[0], mutation)

                    result = self.run_summarizer(artifacts, output)

                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(output.exists())
                    self.assertIn("hardware evidence", result.stderr)

                self.with_copied_artifacts(assertion)

    def test_requires_exactly_two_independent_ray_query_artifacts(self) -> None:
        def assertion(artifacts: list[Path], output: Path) -> None:
            copied_duplicate = artifacts[0].with_name("rtx_b1_duplicate.toml")
            shutil.copy2(artifacts[0], copied_duplicate)
            cases = {
                "one": artifacts[:1],
                "three": [*artifacts, artifacts[0]],
                "duplicate": [artifacts[0], artifacts[0]],
                "same-content": [artifacts[0], copied_duplicate],
            }
            for name, selected in cases.items():
                with self.subTest(name=name):
                    case_output = output.with_stem(f"summary-{name}")

                    result = self.run_summarizer(selected, case_output)

                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(case_output.exists())
                    self.assertIn("exactly two independent", result.stderr)

        self.with_copied_artifacts(assertion)

    def test_rejects_missing_or_duplicate_workload_configuration(self) -> None:
        marker = "[[configurations]]"

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

                def assertion(artifacts: list[Path], output: Path) -> None:
                    self.mutate(artifacts[0], mutation)

                    result = self.run_summarizer(artifacts, output)

                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(output.exists())
                    self.assertIn("workload Cartesian product", result.stderr)

                self.with_copied_artifacts(assertion)

    def test_rejects_incomplete_or_out_of_order_phase_samples(self) -> None:
        sample_marker = "[[configurations.initial.samples]]"

        def missing_sample(text: str) -> str:
            first = text.index(sample_marker)
            second = text.index(sample_marker, first + len(sample_marker))
            return text[:first] + text[second:]

        mutations = {
            "missing-sample": missing_sample,
            "wrong-order-index": lambda text: self.replace_value_after(
                text,
                sample_marker,
                "order_index = ",
                "1",
            ),
            "wrong-mode-order": lambda text: self.replace_value_after(
                text,
                sample_marker,
                "mode = ",
                '"hardware"',
            ),
        }
        for name, mutation in mutations.items():
            with self.subTest(name=name):

                def assertion(artifacts: list[Path], output: Path) -> None:
                    self.mutate(artifacts[0], mutation)

                    result = self.run_summarizer(artifacts, output)

                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(output.exists())
                    self.assertIn("phase sample sequence", result.stderr)

                self.with_copied_artifacts(assertion)

    def test_rejects_any_nonzero_correctness_or_exhaustion_count(self) -> None:
        sample_marker = "[[configurations.initial.samples]]"
        mutations = {
            "correctness": lambda text: self.replace_value_after(
                text,
                sample_marker,
                "false_positive_count = ",
                "-1",
            ),
            "traversal-exhaustion": lambda text: self.replace_value_after(
                text,
                sample_marker,
                "traversal_exhausted_count = ",
                "-1",
            ),
            "software-committed-disagreement": lambda text: self.replace_value_after(
                text,
                sample_marker,
                "query_committed_disagreement_count = ",
                "-1",
            ),
        }
        for name, mutation in mutations.items():
            with self.subTest(name=name):

                def assertion(artifacts: list[Path], output: Path) -> None:
                    self.mutate(artifacts[0], mutation)

                    result = self.run_summarizer(artifacts, output)

                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(output.exists())
                    self.assertIn("zero-count evidence", result.stderr)

                self.with_copied_artifacts(assertion)


if __name__ == "__main__":
    unittest.main()
