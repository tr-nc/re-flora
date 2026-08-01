from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
SUMMARIZER = SCRIPTS / "summarize_ddgi_convergence.py"


class SummarizeDdgiConvergenceTests(unittest.TestCase):
    def write_curve(self, run_dir: Path, *, hard_max: int = 8) -> None:
        stem = "sealed-spacing32-converged-forward"
        lines = []
        samples = (
            ("SeedSky", 0, 0.0, 0.0, 0),
            ("SingleBounce", 1, 0.5, 1.0, 0),
            ("Feedback", 2, 0.002, 0.01, 1),
            ("Feedback", 3, 0.001, 0.005, 2),
        )
        for stage, iteration, absolute, relative, consecutive in samples:
            lines.append(
                "[DDGI] full-atlas validated "
                f"transport={stage} iteration={iteration} source=None "
                f"max_abs_rgb_delta={absolute:.8f} "
                f"max_rel_rgb_delta={relative:.8f} "
                "non_finite=0 negative_rgb_texels=0 "
                "valid_texels=64 scanned_stored_texels=100 "
                "abs_threshold=0.00250000 rel_threshold=0.02000000 "
                f"consecutive_below={consecutive}/2 hard_max={hard_max}"
            )
        (run_dir / f"{stem}.console.log").write_text("\n".join(lines) + "\n")
        (run_dir / f"{stem}.analysis.json").write_text(
            json.dumps(
                {
                    "capture": {
                        "transport_stage": "converged",
                        "transport_iteration": 3,
                        "spacing_voxels": 32,
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
                "--absolute-threshold",
                "0.0025",
                "--relative-threshold",
                "0.02",
                "--consecutive-iterations",
                "2",
                "--hard-max-iteration",
                "8",
                "--cases",
                "sealed",
                "--spacings",
                "32",
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_emits_qualified_curve_with_threshold_margins(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir)

            result = self.run_summarizer(run_dir, output)
            report = json.loads(output.read_text())

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(report["qualified"])
        self.assertEqual(report["matrix"]["curve_count"], 1)
        curve = report["curves"][0]
        self.assertEqual(curve["final_iteration"], 3)
        self.assertEqual(curve["consecutive_below_threshold"], 2)
        self.assertAlmostEqual(curve["absolute_threshold_margin"], 0.0015)
        self.assertAlmostEqual(curve["relative_threshold_margin"], 0.015)
        self.assertEqual(curve["iterations_before_hard_max"], 5)

    def test_rejects_curve_whose_logged_hard_max_drifted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir, hard_max=9)

            result = self.run_summarizer(run_dir, output)

        self.assertEqual(result.returncode, 1)
        self.assertFalse(output.exists())
        self.assertIn("hard-max policy drift", result.stderr)


if __name__ == "__main__":
    unittest.main()
