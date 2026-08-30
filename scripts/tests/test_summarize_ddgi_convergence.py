from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
SUMMARIZER = SCRIPTS / "summarize_ddgi_convergence.py"


class SummarizeDdgiConvergenceTests(unittest.TestCase):
    def write_curve(
        self,
        run_dir: Path,
        *,
        absolute_threshold: float = 0.0025,
        terminal_reason: str = "Threshold",
        maximum_update_epochs: int = 128,
        include_policy: bool = True,
    ) -> None:
        stem = "sealed-spacing32-converged-forward"
        lines = []
        if include_policy:
            lines.append(
                "[DDGI] initialization requested terrain_revision=2 spacing_voxels=32 "
                "probes=4913 stage=RelocationPending "
                "convergence_max_absolute_rgb_delta=0.0025 "
                "convergence_max_relative_rgb_delta=0.02 "
                "convergence_consecutive_epochs=2 "
                "convergence_minimum_update_epochs=4 "
                f"convergence_maximum_update_epochs={maximum_update_epochs}"
            )
        samples = (
            (0, 0.0, 0.0, 0),
            (1, 0.5, 1.0, 0),
            (2, 0.002, 0.01, 1),
            (3, 0.001, 0.005, 2),
        )
        lines.append(
            "[DDGI] full-atlas validated geometry_revision=1 radiance_revision=1 "
            "spacing_voxels=32 state=Converging update_epoch=0 source=None "
            "max_abs_rgb_delta=0.00000000 max_rel_rgb_delta=0.00000000 "
            "non_finite=0 negative_rgb_texels=0 valid_texels=64 "
            "scanned_stored_texels=100 abs_threshold=0.00250000 "
            "rel_threshold=0.02000000 consecutive_below=0/2"
        )
        for epoch, absolute, relative, consecutive in samples:
            lines.append(
                "[DDGI] full-atlas validated "
                "geometry_revision=2 radiance_revision=1 spacing_voxels=32 "
                f"state=Converging update_epoch={epoch} source=None "
                f"max_abs_rgb_delta={absolute:.8f} "
                f"max_rel_rgb_delta={relative:.8f} "
                "non_finite=0 negative_rgb_texels=0 "
                "valid_texels=64 scanned_stored_texels=100 "
                f"abs_threshold={absolute_threshold:.8f} rel_threshold=0.02000000 "
                f"consecutive_below={consecutive}/2"
            )
        lines.append(
            "[DDGI] transport converged "
            f"field_serial=4 update_epoch=3 reason={terminal_reason}"
        )
        (run_dir / f"{stem}.console.log").write_text("\n".join(lines) + "\n")
        (run_dir / f"{stem}.analysis.json").write_text(
            json.dumps(
                {
                    "capture": {
                        "lifecycle_state": "converged",
                        "update_epoch": 3,
                        "spacing_voxels": 32,
                        "geometry_revision": 2,
                        "radiance_revision": 1,
                        "max_abs_delta": 0.001,
                        "max_rel_delta": 0.005,
                    },
                    "validation_failures": [],
                }
            )
        )

    def run_summarizer(self, run_dir: Path, output: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                str(SUMMARIZER),
                "--run-dir",
                str(run_dir),
                "--output",
                str(output),
                "--cases",
                "sealed",
                "--spacings",
                "32",
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_emits_qualified_temporal_curve(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir)

            result = self.run_summarizer(run_dir, output)
            report = json.loads(output.read_text())

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(report["qualified"])
        self.assertEqual(report["schema_version"], 2)
        self.assertEqual(report["matrix"]["curve_count"], 1)
        curve = report["curves"][0]
        self.assertEqual(curve["final_update_epoch"], 3)
        self.assertEqual(curve["terminal_reason"], "Threshold")
        self.assertEqual(len(curve["epochs"]), 4)
        self.assertEqual(report["policy"]["maximum_update_epoch"], 127)

    def test_derives_the_terminal_epoch_from_the_runtime_epoch_count(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir, maximum_update_epochs=64)

            result = self.run_summarizer(run_dir, output)
            report = json.loads(output.read_text())

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report["policy"]["maximum_update_epoch"], 63)

    def test_rejects_a_curve_without_the_authoritative_runtime_policy(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir, include_policy=False)

            result = self.run_summarizer(run_dir, output)

        self.assertEqual(result.returncode, 1)
        self.assertFalse(output.exists())
        self.assertIn("authoritative runtime convergence policy", result.stderr)

    def test_rejects_curve_whose_logged_threshold_drifted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir, absolute_threshold=0.003)

            result = self.run_summarizer(run_dir, output)

        self.assertEqual(result.returncode, 1)
        self.assertFalse(output.exists())
        self.assertIn("absolute policy drift", result.stderr)

    def test_rejects_terminal_reason_that_disagrees_with_curve(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir, terminal_reason="SampleBudget")

            result = self.run_summarizer(run_dir, output)

        self.assertEqual(result.returncode, 1)
        self.assertFalse(output.exists())
        self.assertIn("terminal reason SampleBudget, expected Threshold", result.stderr)


if __name__ == "__main__":
    unittest.main()
