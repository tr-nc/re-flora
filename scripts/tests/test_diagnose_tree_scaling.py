import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from diagnose_tree_scaling import summarize


def replay(times):
    lines = []
    frame = 0
    for count, duration in enumerate(times, 1):
        lines.append(f"[TREE_SCALE_DIAG] phase trees={count} frame={frame}")
        for _ in range(3):
            frame += 1
            lines.append(f"[TREE_SCALE_DIAG] sample trees={count} frame={frame} "
                         f"total_ms={duration} gpu_present_ms=7.000 untracked_cpu_ms=2.000")
    lines.extend([f"[TREE_SCALE_DIAG] completed trees={len(times)}",
                  "Application exited successfully"])
    return "\n".join(lines)


class TreeScalingParserTests(unittest.TestCase):
    def test_persistent_cliff_goes_red_even_when_app_exits_successfully(self):
        phases = summarize(replay([17, 25, 108]), 3)
        self.assertFalse(phases[2]["frame_cliff"])
        self.assertTrue(phases[3]["frame_cliff"])
        self.assertEqual(phases[3]["median_total_ms"], 108)

    def test_normal_gradual_cost_is_not_an_abrupt_cliff(self):
        phases = summarize(replay([17, 25, 35]), 3)
        self.assertFalse(any(phase["frame_cliff"] for phase in phases.values()))

    def test_one_spike_does_not_count_as_persistent_failure(self):
        log = replay([17, 20]).replace("total_ms=20", "total_ms=200", 1)
        phases = summarize(log, 2)
        self.assertEqual(phases[2]["max_total_ms"], 200)
        self.assertFalse(phases[2]["frame_cliff"])

    def test_expected_raster_failure_is_captured_not_mistaken_for_an_app_crash(self):
        error = "[TREE][RASTER_STATIC] preparation failed; raster surface unavailable"
        log = replay([17, 100]).replace("[TREE_SCALE_DIAG] sample trees=2 frame=4",
                                      error + "\n[TREE_SCALE_DIAG] sample trees=2 frame=4")
        phases = summarize(log, 2)
        self.assertEqual(phases[2]["errors"], [error])
        self.assertTrue(phases[2]["frame_cliff"])

    def test_incomplete_replay_is_invalid_not_green(self):
        log = replay([17, 100]).replace("[TREE_SCALE_DIAG] completed trees=2", "")
        with self.assertRaisesRegex(ValueError, "did not complete"):
            summarize(log, 2)

    def test_gpu_validation_must_complete_for_every_tree_count_when_requested(self):
        with self.assertRaisesRegex(ValueError, "missing GPU"):
            summarize(replay([17, 25]), 2, validate_gpu=True)
        markers = "\n[TREE_SCALE_DIAG] validated trees=1\n[TREE_SCALE_DIAG] validated trees=2"
        phases = summarize(replay([17, 25]) + markers, 2, validate_gpu=True)
        self.assertFalse(phases[2]["frame_cliff"])

    def test_cpu_updates_are_attributed_to_settled_sample_frames(self):
        log = replay([17, 25]) + "\n" + "\n".join(
            f"[PERF][TREE_UPDATE] frame={i} cpu_skin_us=100.0 physics_us=50.0 total_cpu_us=160.0"
            for i in range(5)
        )
        # An unsampled warm-up/hitch must not pollute the settled median.
        log += "\n[PERF][TREE_UPDATE] frame=99 cpu_skin_us=9000.0 total_cpu_us=9999.0"
        phases = summarize(log, 2)
        self.assertEqual(phases[1]["cpu_update_medians_us"]["cpu_skin"], 100.0)
        self.assertEqual(phases[1]["cpu_update_medians_us"]["total_cpu"], 160.0)

    def test_frame_scopes_match_settled_frame_ids_even_when_gpu_results_arrive_late(self):
        log = replay([17, 25]) + (
            "\n[PERF][CPU_FRAME_SCOPE] frame 1 render.record=1000us render.acquire=6000us"
            "\n[PERF][CPU_FRAME_SCOPE] frame 2 render.record=3000us"
            "\n[PERF][CPU_FRAME_SCOPE] frame 99 render.record=90000us"
            "\n[PERF][GPU_FRAME_SCOPE] frame 1 frame.render=4000us"
            "\n[PERF][GPU_FRAME_SCOPE] frame 2 frame.render=6000us"
            "\n[PERF][GPU_FRAME_SCOPE] frame 4 frame.render=8000us"
            "\n[PERF][GPU_FRAME_SCOPE] frame 99 frame.render=99000us"
        )
        phases = summarize(log, 2)
        self.assertEqual(phases[1]["cpu_scope_medians_us"]["render.record"], 2000)
        self.assertEqual(phases[1]["gpu_scope_medians_us"]["frame.render"], 5000)
        self.assertEqual(phases[2]["gpu_scope_medians_us"]["frame.render"], 8000)
        self.assertEqual(phases[1]["gpu_scope_sample_counts"]["frame.render"], 2)
        self.assertEqual(phases[1]["cpu_scope_sample_counts"]["render.acquire"], 1)

    def test_twelve_tree_replay_completes_and_validates_every_count(self):
        log = replay([17] * 12) + "\n" + "\n".join(
            f"[TREE_SCALE_DIAG] validated trees={i}" for i in range(1, 13)
        )
        phases = summarize(log, 12, validate_gpu=True)
        self.assertEqual(len(phases), 12)
        self.assertFalse(phases[12]["frame_cliff"])

    def test_fruit_step_profile_excludes_other_phase_and_unsampled_hitches(self):
        log = replay([17, 25]) + (
            "\n[PERF][FRUIT_UPDATE] frame=1 bodies=10 steps=2 step_us=8000 render_us=20"
            "\n[PERF][FRUIT_UPDATE] frame=2 bodies=10 steps=1 step_us=4000 render_us=20"
            "\n[PERF][FRUIT_UPDATE] frame=4 bodies=20 steps=2 step_us=12000 render_us=30"
            "\n[PERF][FRUIT_UPDATE] frame=99 bodies=20 steps=4 step_us=99000 render_us=40"
        )
        phases = summarize(log, 2)
        self.assertEqual(phases[1]["fruit_update_medians_us"]["step"], 6000)
        self.assertEqual(phases[2]["fruit_update_medians_us"]["step"], 12000)

    def test_validation_error_is_invalid_not_green(self):
        with self.assertRaisesRegex(ValueError, "validation failed"):
            summarize(replay([17, 100]) + "\nVUID-example", 2)


if __name__ == "__main__":
    unittest.main()
