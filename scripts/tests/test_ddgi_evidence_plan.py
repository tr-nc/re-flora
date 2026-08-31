from __future__ import annotations

import dataclasses
import io
import tempfile
import unittest
from pathlib import Path

import sys


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from ddgi_evidence.executor import ActionResult, RecordingHost, execute  # noqa: E402
from ddgi_evidence.model import (  # noqa: E402
    AnalyzeCurrentCapture,
    BuildRelease,
    Capture,
    Claim,
    CorrectnessOptions,
    FailureKey,
    IncludeSuite,
    LifecycleOptions,
    ProductionAnalyzerOptions,
    RunRequest,
    RuntimeTerrainEditsOptions,
    Setup,
    Suite,
    TransportOptions,
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
        with self.assertRaisesRegex(ValueError, "cannot select"):
            ProductionAnalyzerOptions(("--expect-version", "9"))

    def test_recording_host_executes_the_same_plan_without_side_effects(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            before = tuple(root.iterdir())
            execution_plan = plan(self.request(Suite.TRANSPORT, root))
            host = RecordingHost(stdout=io.StringIO(), stderr=io.StringIO())

            report = execute(execution_plan, host)

            self.assertTrue(report.succeeded, report.failures)
            self.assertEqual(tuple(root.iterdir()), before)
            self.assertEqual(
                sum(command.kind == "capture" for command in host.commands), 100
            )
            self.assertEqual(
                sum(command.kind == "analysis" for command in host.commands), 78
            )

    def test_transport_owns_one_release_setup_and_one_workspace_preparation(self) -> None:
        class PreparingHost(RecordingHost):
            def __init__(self) -> None:
                super().__init__(stdout=io.StringIO(), stderr=io.StringIO())
                self.prepare_calls = 0

            def prepare(self, workspace):
                self.prepare_calls += 1
                return super().prepare(workspace)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "outputs"
            request = RunRequest(
                Suite.TRANSPORT,
                root,
                dry_run=False,
                options=TransportOptions(
                    output_dir=output / "transport",
                    correctness=CorrectnessOptions(
                        output_dir=output / "correctness"
                    ),
                    runtime=RuntimeTerrainEditsOptions(
                        output_dir=output / "runtime"
                    ),
                    lifecycle=LifecycleOptions(
                        output_dir=output / "lifecycle"
                    ),
                ),
                run_id="one-workspace",
            )
            execution_plan = plan(request)
            release_builds = tuple(
                action
                for action in iter_actions(execution_plan)
                if isinstance(action, BuildRelease)
            )
            host = PreparingHost()

            report = execute(execution_plan, host)

        self.assertTrue(report.succeeded, report.failures)
        self.assertEqual(len(release_builds), 1)
        self.assertEqual(
            sum(isinstance(stage, Setup) for stage in execution_plan.stages),
            1,
        )
        child_plans = tuple(
            stage.execution_plan
            for stage in execution_plan.stages
            if isinstance(stage, IncludeSuite)
        )
        self.assertTrue(child_plans)
        self.assertTrue(
            all(
                not any(isinstance(stage, Setup) for stage in child.stages)
                for child in child_plans
            )
        )
        self.assertEqual(
            sum(command.kind == "build" for command in host.commands), 1
        )
        self.assertEqual(host.prepare_calls, 1)
        self.assertEqual(
            sum(command.kind == "capture" for command in host.commands), 100
        )
        self.assertEqual(
            sum(command.kind == "analysis" for command in host.commands), 78
        )

    def test_recording_host_cannot_issue_production_evidence_claims(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = io.StringIO()
            errors = io.StringIO()
            request = RunRequest(
                Suite.TRANSPORT,
                Path(temporary),
                dry_run=False,
            )
            host = RecordingHost(stdout=output, stderr=errors)

            report = execute(plan(request), host)

        self.assertTrue(report.succeeded, report.failures)
        self.assertEqual(report.claims, ())
        self.assertNotIn("=PROVEN", output.getvalue())
        self.assertIn("{dry-run:geometry_revision}", errors.getvalue())

    def test_failure_keys_accumulate_the_full_correctness_capture_matrix(self) -> None:
        class FailingCaptureHost(RecordingHost):
            def capture(self, action, repo_root):
                super().capture(action, repo_root)
                return ActionResult(False, "fixture capture failure")

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            host = FailingCaptureHost(stdout=io.StringIO(), stderr=io.StringIO())
            report = execute(plan(self.request(Suite.CORRECTNESS, root)), host)

        self.assertEqual(len(report.failures), 48)
        self.assertEqual(
            sum(command.kind == "capture" for command in host.commands), 48
        )
        self.assertEqual(
            sum(command.kind == "analysis" for command in host.commands), 0
        )

    def test_transport_claims_require_prior_typed_evidence_stages(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            execution_plan = plan(
                self.request(Suite.TRANSPORT, Path(temporary))
            )
        claims = {
            stage.id: stage
            for stage in execution_plan.stages
            if isinstance(stage, Claim)
        }
        self.assertEqual(
            claims["transport.filter-history-outcome"].requires,
            (
                "transport.dogleg.32.e0.forward",
                "transport.dogleg.32.e1.forward",
                "transport.dogleg.16.e0.forward",
                "transport.dogleg.16.e1.forward",
            ),
        )
        self.assertEqual(
            claims["transport.direct-sun"].requires,
            ("transport.include.runtime",),
        )
        self.assertEqual(
            claims["transport.filter-history-action"].requires[-5:],
            (
                "transport.convergence",
                "transport.include.correctness",
                "transport.include.runtime",
                "transport.sky-normalization",
                "transport.include.lifecycle",
            ),
        )

        prior_stage_ids: set[str] = set()
        for stage in execution_plan.stages:
            if isinstance(stage, Claim):
                self.assertLessEqual(set(stage.requires), prior_stage_ids)
            prior_stage_ids.add(stage.id)

    def test_direct_sun_proof_keeps_its_pre_normalization_order(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            execution_plan = plan(
                self.request(Suite.TRANSPORT, Path(temporary))
            )
        stage_ids = tuple(stage.id for stage in execution_plan.stages)

        self.assertLess(
            stage_ids.index("transport.direct-sun"),
            stage_ids.index("transport.sky-normalization"),
        )

    def test_transport_folds_a_failed_include_and_retains_nested_details(self) -> None:
        class FailingCorrectnessHost(RecordingHost):
            def capture(self, action, repo_root):
                recorded = super().capture(action, repo_root)
                if action.suite is Suite.CORRECTNESS:
                    return ActionResult(False, "fixture correctness failure")
                return recorded

        with tempfile.TemporaryDirectory() as temporary:
            report = execute(
                plan(self.request(Suite.TRANSPORT, Path(temporary))),
                FailingCorrectnessHost(stdout=io.StringIO(), stderr=io.StringIO()),
            )

        self.assertEqual(len(report.failures), 1)
        self.assertEqual(
            report.failures[0].key,
            FailureKey(Suite.TRANSPORT, "include-correctness"),
        )
        correctness = next(
            included.report
            for included in report.included_reports
            if included.stage_id == "transport.include.correctness"
        )
        self.assertEqual(len(correctness.failures), 48)

    def test_runtime_transient_failures_fold_once_per_spacing(self) -> None:
        class FailingTransientHost(RecordingHost):
            def capture(self, action, repo_root):
                recorded = super().capture(action, repo_root)
                if (
                    action.suite is Suite.RUNTIME_TERRAIN_EDITS
                    and action.scenario == "terrain-edits-inflight-capture"
                ):
                    return ActionResult(False, "fixture transient failure")
                return recorded

        with tempfile.TemporaryDirectory() as temporary:
            report = execute(
                plan(self.request(Suite.RUNTIME_TERRAIN_EDITS, Path(temporary))),
                FailingTransientHost(stdout=io.StringIO(), stderr=io.StringIO()),
            )

        self.assertEqual(len(report.failures), 2)
        self.assertEqual(
            {failure.key for failure in report.failures},
            {
                FailureKey(Suite.RUNTIME_TERRAIN_EDITS, "inflight-stale-active", 32),
                FailureKey(Suite.RUNTIME_TERRAIN_EDITS, "inflight-stale-active", 16),
            },
        )


if __name__ == "__main__":
    unittest.main()
