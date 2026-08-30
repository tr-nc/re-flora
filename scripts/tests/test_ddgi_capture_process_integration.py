from __future__ import annotations

import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]


class DdgiCaptureProcessIntegrationTests(unittest.TestCase):
    def test_every_live_ddgi_capture_runner_uses_the_shared_process_boundary(self) -> None:
        runners = (
            "check_ddgi_correctness.sh",
            "check_ddgi_transport_acceptance.sh",
            "check_ddgi_runtime_terrain_edits.sh",
            "check_ddgi_lifecycle_acceptance.sh",
            "check_ddgi_local_terrain_convergence.sh",
            "check_ddgi_terrain_edit_cycle.sh",
            "check_ddgi_inflight_terrain_edits.sh",
        )
        for runner in runners:
            with self.subTest(runner=runner):
                source = (SCRIPTS / runner).read_text(encoding="utf-8")
                self.assertIn(
                    'source "$repo_root/scripts/lib/capture_process_evidence.sh"',
                    source,
                )
                self.assertIn("run_capture_with_process_evidence", source)
                self.assertIn("re_flora::run_log_binding=info", source)
                self.assertNotIn("grep -Eiq '(^|[^[:alpha:]])(ERROR", source)

    def test_sky_normalization_reuses_the_canonical_fatal_matcher(self) -> None:
        source = (SCRIPTS / "check_ddgi_sky_normalization_evidence.py").read_text(
            encoding="utf-8"
        )
        self.assertIn(
            "from validate_capture_process_evidence import FATAL_DIAGNOSTIC", source
        )
        self.assertNotIn("ERROR_MARKER =", source)


if __name__ == "__main__":
    unittest.main()
