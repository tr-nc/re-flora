from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import perf_suite  # noqa: E402


CONFIG_TEXT = """
version = 1

[scenario.test]
description = "fixture"
args = []
env = {}
warmup_frame = 2
required_markers = ["[PERF][GPU_FRAME_SCOPE]"]
match_surface_workload = true

[[scenario.test.metric]]
name = "frame.render"
source = "gpu_scope"
key = "frame.render"
min_samples = 2
budget_percent = 2.0

[[scenario.test.metric]]
name = "render.record"
source = "cpu_scope"
key = "render.record"
min_samples = 2
budget_percent = 2.0

[[scenario.test.metric]]
name = "surface.make_sparse"
source = "surface_pass"
key = "make_surface_sparse"
min_samples = 1
budget_percent = 3.0

[[scenario.test.metric]]
name = "surface.build"
source = "gpu_job_scope"
key = "surface.build"
min_samples = 1
budget_percent = 3.0

[[scenario.test.metric]]
name = "tree.replace_deferred_total"
source = "tree_bench"
key = "replace_deferred_total"
min_samples = 1
budget_percent = 5.0
"""

LOG_TEXT = """
[INFO] Selected physical device: Fixture GPU
[INFO] [PERF][GPU_FRAME_SCOPE] frame 1 scopes=1 dropped=0 frame.render=999us
[INFO] [PERF][GPU_FRAME_SCOPE] frame 2 scopes=2 dropped=0 frame.render=100us tracer.render=60us
[INFO] [PERF][GPU_FRAME_SCOPE] frame 3 scopes=2 dropped=0 frame.render=110us tracer.render=65us
[INFO] [PERF][CPU_FRAME_SCOPE] frame 1 render.record=999us
[INFO] [PERF][CPU_FRAME_SCOPE] frame 2 render.record=40us
[INFO] [PERF][CPU_FRAME_SCOPE] frame 3 render.record=45us
[DEBUG] [PERF][SURFACE_BUILD_PASS_TIMING] chunk UVec3(0, 0, 0) pass_total=0.220ms make_surface_sparse=0.125ms write_instances=0.095ms
[INFO] [PERF][GPU_JOB_SCOPE] name=surface.build queue=Compute chunk UVec3(0, 0, 0) duration=240us
[INFO] [PERF][SURFACE_BUILD] chunk UVec3(0, 0, 0) total 1.0ms active_voxels 12 active_bricks 4 solid_workgroups 8 place_flora true flora_rebuilt true
[INFO] [PERF][TREE_BENCH] sample 1/1 replace_deferred_total 2.50ms initial_length 32.00 seed 122
"""


class PerfSuiteTests(unittest.TestCase):
    def scenario(self) -> perf_suite.Scenario:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "config.toml"
            path.write_text(CONFIG_TEXT, encoding="utf-8")
            version, scenarios = perf_suite.load_config(path)
        self.assertEqual(version, 1)
        return scenarios["test"]

    def test_parses_all_metric_sources_and_applies_frame_warmup(self) -> None:
        samples = perf_suite.parse_samples(LOG_TEXT, self.scenario())

        self.assertEqual(samples["frame.render"], [100.0, 110.0])
        self.assertEqual(samples["render.record"], [40.0, 45.0])
        self.assertEqual(samples["surface.make_sparse"], [125.0])
        self.assertEqual(samples["surface.build"], [240.0])
        self.assertEqual(samples["tree.replace_deferred_total"], [2500.0])

    def test_parses_surface_workload_signature(self) -> None:
        self.assertEqual(
            perf_suite.parse_surface_workload(LOG_TEXT),
            [
                {
                    "chunk": "UVec3(0, 0, 0)",
                    "active_voxels": 12,
                    "active_bricks": 4,
                    "solid_workgroups": 8,
                }
            ],
        )

    def test_summary_uses_interpolated_p95(self) -> None:
        summary = perf_suite.summarize([100.0, 110.0])
        self.assertEqual(summary.median_us, 105.0)
        self.assertEqual(summary.p95_us, 109.5)
        self.assertEqual(summary.stddev_us, 5.0)
        self.assertEqual(summary.variance_us2, 25.0)

    def test_comparison_pools_order_reversed_reports_and_flags_budget(self) -> None:
        def report(samples: list[float]) -> dict[str, object]:
            return {
                "scenario": "test",
                "workload": [],
                "metrics": {
                    "frame.render": {
                        "budget_percent": 2.0,
                        "samples": samples,
                    }
                },
            }

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = []
            for index, samples in enumerate(([100.0], [104.0], [102.0], [106.0])):
                path = root / f"{index}.json"
                path.write_text(json.dumps(report(samples)), encoding="utf-8")
                paths.append(path)

            scenario, rows = perf_suite.compare_reports(
                [paths[0], paths[2]], [paths[1], paths[3]]
            )

        self.assertEqual(scenario, "test")
        self.assertEqual(rows[0].baseline.median_us, 101.0)
        self.assertEqual(rows[0].candidate.median_us, 105.0)
        self.assertTrue(rows[0].regression)

    def test_validation_rejects_missing_markers_and_errors(self) -> None:
        scenario = self.scenario()
        with self.assertRaisesRegex(ValueError, "missing required markers"):
            perf_suite.validate_log("ordinary line", scenario)
        with self.assertRaisesRegex(ValueError, "fatal or validation"):
            perf_suite.validate_log(
                "[PERF][GPU_FRAME_SCOPE]\nvalidation error: VUID-test", scenario
            )


if __name__ == "__main__":
    unittest.main()
