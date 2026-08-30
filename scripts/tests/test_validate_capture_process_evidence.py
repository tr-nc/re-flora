from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
VALIDATOR = SCRIPTS / "validate_capture_process_evidence.py"


class ValidateCaptureProcessEvidenceTests(unittest.TestCase):
    def validate(
        self,
        *,
        console_extra: str = "",
        run_log_extra: str = "",
        publication_revision: int = 2,
        initialization_revision: int = 2,
        build_revision: int = 2,
        verified_publication_revision: int = 2,
        order: tuple[str, ...] = ("publication", "initialization", "verification"),
        duplicate_marker: bool = False,
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            run_log = (root / "run.log").resolve()
            marker = f"[RUN_LOG] path={run_log}\n"
            events = {
                "publication": (
                    "[ENV_LIGHT_TEST] static terrain ready case=sealed "
                    f"terrain_revision={publication_revision} settling_frames=2\n"
                ),
                "initialization": (
                    "[DDGI] initialization requested "
                    f"terrain_revision={initialization_revision} spacing_voxels=32\n"
                ),
                "verification": (
                    "[ENV_LIGHT_TEST] first DDGI build verified build_token_serial=1 "
                    f"geometry_revision={build_revision} "
                    f"visible_terrain_publication_revision={verified_publication_revision}\n"
                ),
            }
            body = "".join(events[name] for name in order)
            run_log.write_text(marker + body + run_log_extra, encoding="utf-8")
            console = root / "console.log"
            console.write_text(
                marker + (marker if duplicate_marker else "") + body + console_extra,
                encoding="utf-8",
            )
            return subprocess.run(
                ["python3", str(VALIDATOR), str(console)],
                check=False,
                capture_output=True,
                text=True,
            )

    def test_accepts_one_clean_process_bound_log_with_ordered_identity(self) -> None:
        result = self.validate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("status=PASS", result.stdout)

    def test_rejects_dirty_console_and_dirty_run_log(self) -> None:
        for kwargs in (
            {"console_extra": "VUID-test\n"},
            {"run_log_extra": "stale readback detected\n"},
        ):
            with self.subTest(kwargs=kwargs):
                result = self.validate(**kwargs)
                self.assertEqual(result.returncode, 1)
                self.assertIn("fatal or validation diagnostic", result.stderr)

    def test_rejects_missing_or_duplicate_process_binding(self) -> None:
        duplicate = self.validate(duplicate_marker=True)
        self.assertEqual(duplicate.returncode, 1)
        self.assertIn("found 2", duplicate.stderr)

        with tempfile.TemporaryDirectory() as directory:
            console = Path(directory) / "console.log"
            console.write_text("clean but unbound\n", encoding="utf-8")
            missing = subprocess.run(
                ["python3", str(VALIDATOR), str(console)],
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertEqual(missing.returncode, 1)
        self.assertIn("found 0", missing.stderr)

    def test_rejects_revision_or_event_order_drift(self) -> None:
        mismatch = self.validate(build_revision=1)
        self.assertEqual(mismatch.returncode, 1)
        self.assertIn("revisions differ", mismatch.stderr)

        reordered = self.validate(
            order=("initialization", "publication", "verification")
        )
        self.assertEqual(reordered.returncode, 1)
        self.assertIn("must precede", reordered.stderr)


if __name__ == "__main__":
    unittest.main()
