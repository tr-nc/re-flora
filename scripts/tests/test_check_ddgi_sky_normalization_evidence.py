from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import analyze_environment_irradiance_capture as analyzer  # noqa: E402
import check_ddgi_sky_normalization_evidence as checker  # noqa: E402


class CheckDdgiSkyNormalizationEvidenceTests(unittest.TestCase):
    def write_capture(
        self,
        path: Path,
        spacing: int,
        pixels: list[tuple[float, float, float, float]],
    ) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        header = analyzer.HEADER_V2.pack(
            analyzer.MAGIC, 2, len(pixels), 1, 4, 1, spacing, 0
        )
        payload = b"".join(analyzer.PIXEL.pack(*pixel) for pixel in pixels)
        path.write_bytes(header + payload)

    def make_evidence(
        self,
        root: Path,
        *,
        delta: float = 2.5e-7,
        after_hit: float = 1.0,
    ) -> dict[str, object]:
        for spacing in checker.SPACINGS:
            before = [
                (0.25, 0.5, 0.75, 1.0),
                (0.0, 0.0, 0.0, 0.0),
            ]
            after = [
                (0.25 + delta, 0.5, 0.75, after_hit),
                (0.0, 0.0, 0.0, 0.0),
            ]
            for label, pixels in (("before", before), ("after", after)):
                capture = root / label / f"portal-spacing{spacing}-sky-only.rfirr"
                self.write_capture(capture, spacing, pixels)
                console = root / label / f"portal-spacing{spacing}-sky-only.console.log"
                console.write_text(
                    checker.AUTHORED_SCENE_MARKER
                    + "\n[ENV_IRRADIANCE_CAPTURE] saved "
                    + f"backend=ddgi spacing_voxels={spacing} view=final samples=178688 "
                    + "format=float4-linear-rgb-hit\n"
                )

        return {
            "schema_version": 1,
            "git": {
                "before_commit": checker.BEFORE_COMMIT,
                "after_commit": checker.AFTER_COMMIT,
                "subjects": dict(checker.EXPECTED_SUBJECTS),
                "changed_files": list(checker.EXPECTED_CHANGED_FILES),
                "runtime_transport_symbols_absent": list(
                    checker.RUNTIME_TRANSPORT_SYMBOLS
                ),
            },
            "capture_contract": {
                "field": "pre-transport-sky-only",
                "spacings_voxels": list(checker.SPACINGS),
                "command_template": list(checker.COMMAND_TEMPLATE),
                "authored_scene_marker": checker.AUTHORED_SCENE_MARKER,
            },
            "hard_thresholds": {
                "channel_error_max": checker.MAX_CHANNEL_ERROR,
                "luminance_error_max": checker.MAX_LUMINANCE_ERROR,
                "hit_mask_matches": True,
            },
            "cases": [checker.collect_case(root, spacing) for spacing in checker.SPACINGS],
            "overall_result": "pass",
        }

    def validate(self, evidence: dict[str, object], root: Path) -> list[str]:
        return checker.validate_evidence(
            evidence, root, repo_root=None, verify_git=False
        )

    def test_accepts_real_artifacts_below_the_hard_threshold(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            evidence = self.make_evidence(root)

            failures = self.validate(evidence, root)

        self.assertEqual(failures, [])

    def test_rejects_channel_error_above_the_hard_threshold(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            evidence = self.make_evidence(root, delta=2.0e-6)

            failures = self.validate(evidence, root)

        self.assertTrue(
            any("channel_error_max exceeds hard threshold" in item for item in failures),
            failures,
        )

    def test_rejects_a_hit_mask_change(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            evidence = self.make_evidence(root, after_hit=0.0)

            failures = self.validate(evidence, root)

        self.assertTrue(any("hit masks differ" in item for item in failures), failures)

    def test_recomputes_artifact_hashes_and_metrics(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            evidence = self.make_evidence(root)
            evidence["cases"][0]["comparison"]["channel_error_p99"] = 0.0
            capture = root / "after/portal-spacing16-sky-only.rfirr"
            capture.write_bytes(capture.read_bytes() + b"tampered")

            failures = self.validate(evidence, root)

        self.assertTrue(
            any("channel_error_p99" in item for item in failures), failures
        )
        self.assertTrue(any("cannot load artifacts" in item for item in failures), failures)

    def test_rejects_command_or_spacing_coverage_drift_without_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            evidence = self.make_evidence(root)
            evidence["capture_contract"]["command_template"][-1] = "30"
            evidence["cases"] = evidence["cases"][:1]

            failures = checker.validate_evidence(
                evidence, None, repo_root=None, verify_git=False
            )

        self.assertTrue(any("command template" in item for item in failures), failures)
        self.assertTrue(any("spacing 32 then 16" in item for item in failures), failures)

    def test_rejects_a_non_numeric_recorded_metric_without_crashing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            evidence = self.make_evidence(root)
            evidence["cases"][0]["comparison"]["channel_error_max"] = "unknown"

            failures = checker.validate_evidence(
                evidence, None, repo_root=None, verify_git=False
            )

        self.assertTrue(
            any("invalid channel_error_max" in item for item in failures), failures
        )


if __name__ == "__main__":
    unittest.main()
