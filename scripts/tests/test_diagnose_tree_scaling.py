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

    def test_validation_error_is_invalid_not_green(self):
        with self.assertRaisesRegex(ValueError, "validation failed"):
            summarize(replay([17, 100]) + "\nVUID-example", 2)


if __name__ == "__main__":
    unittest.main()
