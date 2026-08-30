from __future__ import annotations

import os
import subprocess
import unittest
from pathlib import Path

import sys


SCRIPTS = Path(__file__).resolve().parents[1]
ROOT = SCRIPTS.parent
sys.path.insert(0, str(SCRIPTS))

from ddgi_evidence.cli import request_from_environment  # noqa: E402
from ddgi_evidence.model import (  # noqa: E402
    CorrectnessOptions,
    LocalTerrainConvergenceOptions,
    Suite,
)


RUNNERS = {
    Suite.CORRECTNESS: "check_ddgi_correctness.sh",
    Suite.INFLIGHT_TERRAIN_EDITS: "check_ddgi_inflight_terrain_edits.sh",
    Suite.LIFECYCLE: "check_ddgi_lifecycle_acceptance.sh",
    Suite.LOCAL_TERRAIN_CONVERGENCE: "check_ddgi_local_terrain_convergence.sh",
    Suite.RUNTIME_TERRAIN_EDITS: "check_ddgi_runtime_terrain_edits.sh",
    Suite.TERRAIN_EDIT_CYCLE: "check_ddgi_terrain_edit_cycle.sh",
    Suite.TRANSPORT: "check_ddgi_transport_acceptance.sh",
}


class TypedDdgiEvidenceCliTests(unittest.TestCase):
    def test_seven_shell_entrypoints_are_exact_four_line_adapters(self) -> None:
        for suite, name in RUNNERS.items():
            with self.subTest(runner=name):
                lines = (SCRIPTS / name).read_text(encoding="utf-8").splitlines()
                self.assertEqual(len(lines), 4)
                self.assertEqual(lines[0], "#!/usr/bin/env bash")
                self.assertEqual(lines[1], "set -euo pipefail")
                self.assertIn("readonly repo_root=", lines[2])
                self.assertEqual(
                    lines[3],
                    'exec /usr/bin/env python3 "$repo_root/scripts/ddgi_evidence/cli.py" '
                    f'{suite.value} "$@"',
                )

    def test_environment_is_decoded_once_into_suite_specific_options(self) -> None:
        correctness = request_from_environment(
            Suite.CORRECTNESS,
            ROOT,
            True,
            {
                "DDGI_CORRECTNESS_AUTO_EXIT": "321",
                "DDGI_CORRECTNESS_OUTPUT_DIR": "/tmp/correctness-output",
                "DDGI_CORRECTNESS_TERRAIN_HARD_ORIGIN": "1,2,3",
            },
            run_id="test-run",
        )
        self.assertEqual(
            correctness.options,
            CorrectnessOptions(
                auto_exit="321",
                output_dir=Path("/tmp/correctness-output"),
                terrain_hard_origin="1,2,3",
            ),
        )
        local = request_from_environment(
            Suite.LOCAL_TERRAIN_CONVERGENCE,
            ROOT,
            True,
            {
                "DDGI_LOCAL_TERRAIN_MIN_RECOVERY_EPOCH": "6",
                "DDGI_LOCAL_TERRAIN_MAX_POST_PROMOTION_HIGH_DELTA_EPOCHS": "2",
            },
            run_id="test-run",
        )
        self.assertEqual(
            local.options,
            LocalTerrainConvergenceOptions(
                minimum_recovery_epoch=6,
                maximum_post_promotion_high_delta_epochs=2,
            ),
        )

    def test_invalid_runner_arguments_keep_exit_code_two(self) -> None:
        result = subprocess.run(
            [str(SCRIPTS / RUNNERS[Suite.CORRECTNESS]), "--unknown"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
            env=dict(os.environ),
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage:", result.stderr)


if __name__ == "__main__":
    unittest.main()
