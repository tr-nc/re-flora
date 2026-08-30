from __future__ import annotations

import dataclasses
import tempfile
import unittest
from pathlib import Path

import sys


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from ddgi_evidence.executor import RecordingHost, execute  # noqa: E402
from ddgi_evidence.model import (  # noqa: E402
    AnalyzeCurrentCapture,
    Capture,
    ProductionAnalyzerOptions,
    RunRequest,
    Suite,
    iter_actions,
)
from ddgi_evidence.plan import plan  # noqa: E402


EXPECTED_COUNTS = {
    Suite.CORRECTNESS: (48, 44),
    Suite.INFLIGHT_TERRAIN_EDITS: (4, 6),
    Suite.LIFECYCLE: (3, 3),
    Suite.LOCAL_TERRAIN_CONVERGENCE: (1, 1),
    Suite.RUNTIME_TERRAIN_EDITS: (29, 11),
    Suite.TERRAIN_EDIT_CYCLE: (4, 4),
    Suite.TRANSPORT: (100, 78),
}


class TypedDdgiEvidencePlanTests(unittest.TestCase):
    def request(self, suite: Suite, root: Path) -> RunRequest:
        return RunRequest(suite=suite, repo_root=root, dry_run=True)

    def test_closed_suite_inventory_and_exact_matrix_sizes(self) -> None:
        self.assertEqual(len(Suite), 7)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for suite, expected in EXPECTED_COUNTS.items():
                with self.subTest(suite=suite.value):
                    actions = tuple(iter_actions(plan(self.request(suite, root))))
                    captures = sum(isinstance(action, Capture) for action in actions)
                    analyses = sum(
                        isinstance(action, AnalyzeCurrentCapture) for action in actions
                    )
                    self.assertEqual((captures, analyses), expected)

    def test_correctness_and_transport_keep_the_120_second_budget(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for suite in (Suite.CORRECTNESS, Suite.TRANSPORT):
                captures = (
                    action
                    for action in iter_actions(plan(self.request(suite, root)))
                    if isinstance(action, Capture) and action.suite is suite
                )
                self.assertTrue(all(action.auto_exit == "120" for action in captures))

    def test_production_analyzer_is_current_only_and_has_no_version_selection(self) -> None:
        self.assertNotIn(
            "version", {field.name for field in dataclasses.fields(ProductionAnalyzerOptions)}
        )
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for suite in Suite:
                for action in iter_actions(plan(self.request(suite, root))):
                    if not isinstance(action, AnalyzeCurrentCapture):
                        continue
                    self.assertEqual(
                        action.argv(root)[0],
                        str(root / "scripts/analyze_current_environment_irradiance_capture.py"),
                    )
                    self.assertNotIn("--expect-version", action.argv(root))

    def test_recording_host_executes_the_same_plan_without_side_effects(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            before = tuple(root.iterdir())
            execution_plan = plan(self.request(Suite.TRANSPORT, root))
            host = RecordingHost()

            report = execute(execution_plan, host)

            self.assertTrue(report.succeeded, report.failures)
            self.assertEqual(tuple(root.iterdir()), before)
            self.assertEqual(
                sum(command.kind == "capture" for command in host.commands), 100
            )
            self.assertEqual(
                sum(command.kind == "analysis" for command in host.commands), 78
            )


if __name__ == "__main__":
    unittest.main()
