from __future__ import annotations

import unittest
import re
import stat
import subprocess
import tempfile
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

        helper = (SCRIPTS / "lib" / "capture_process_evidence.sh").read_text(
            encoding="utf-8"
        )
        self.assertIn("--preserve-run-log", helper)
        self.assertIn('${console%.console.log}.run.log', helper)

    def test_sky_normalization_reuses_the_canonical_fatal_matcher(self) -> None:
        source = (SCRIPTS / "check_ddgi_sky_normalization_evidence.py").read_text(
            encoding="utf-8"
        )
        self.assertIn(
            "from runtime_log_diagnostics import first_fatal_diagnostic", source
        )
        self.assertNotIn("ERROR_MARKER =", source)

    def test_terrain_edit_cycle_enables_initialization_evidence_in_a_real_helper_run(
        self,
    ) -> None:
        source = (SCRIPTS / "check_ddgi_terrain_edit_cycle.sh").read_text(
            encoding="utf-8"
        )
        rust_log = re.search(r'^capture_rust_log="([^"]+)"$', source, re.MULTILINE)
        self.assertIsNotNone(rust_log)
        self.assertIn("re_flora::tracer=info", rust_log.group(1))

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            capture = root / "terrain-edit.rfirr"
            console = root / "terrain-edit.console.log"
            canonical_log = root / "canonical run.log"
            fake_app = root / "fake-app.sh"
            fake_app.write_text(
                """#!/usr/bin/env bash
set -euo pipefail
[[ "${RUST_LOG:-}" == *"re_flora::tracer=info"* ]] || exit 91
capture="$1"
run_log="$2"
: >"$capture"
: >"$run_log"
run_log="$(realpath "$run_log")"
events="[12:34:56.789 INFO re_flora] [RUN_LOG] path=$run_log
[12:34:56.790 INFO re_flora::app::core::environment_lighting_test_scene] [ENV_LIGHT_TEST] static terrain ready case=terrain-edits terrain_revision=2 settling_frames=2
[12:34:56.791 INFO re_flora::tracer] [DDGI] initialization requested terrain_revision=2 spacing_voxels=32
[12:34:56.792 INFO re_flora::app::core::environment_lighting_test_scene] [ENV_LIGHT_TEST] first DDGI build verified build_token_serial=1 geometry_revision=2 visible_terrain_publication_revision=2
[12:34:56.793 INFO re_flora::app::core::environment_irradiance_capture] [ENV_IRRADIANCE_CAPTURE] saved path=$capture
[12:34:56.794 INFO re_flora::app::core] [ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run"
printf '%s\n' "$events" >"$run_log"
printf '%s\n' "$events"
""",
                encoding="utf-8",
            )
            fake_app.chmod(fake_app.stat().st_mode | stat.S_IXUSR)
            command = f"""
repo_root={SCRIPTS.parent!s}
source "$repo_root/scripts/lib/capture_process_evidence.sh"
run_capture_with_process_evidence \
  {console!s} {capture!s} '{rust_log.group(1)}' \
  --require-test-scene-startup -- {fake_app!s} {capture!s} '{canonical_log!s}'
"""
            result = subprocess.run(
                ["bash", "-c", command],
                check=False,
                capture_output=True,
                text=True,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("found 0", result.stderr)


if __name__ == "__main__":
    unittest.main()
