import unittest

from scripts.check_tree_terrain_edit_perf import analyze


def replay(extra=""):
    return f"""[00:00:00.100 INFO test] [TREE][RASTER_STATIC] revision=1 compile_ms=80
[00:00:00.200 INFO test] [PERF][FRAME] frame 1 total 100.0ms
[00:00:02.000 INFO test] [WATER][EDIT_SOAK] applied shore-dig-a
{extra}
[00:00:02.100 INFO test] [PERF][FRAME] frame 2 total 18.0ms
[00:00:04.000 INFO test] [WATER][EDIT_SOAK] applied shore-rock-place
[00:00:06.000 INFO test] [WATER][EDIT_SOAK] applied shore-dig-b
[00:00:06.100 INFO test] [PERF][FRAME] frame 3 total 20.0ms
completed deterministic terrain-edit sequence
Application exited successfully"""


class TreeTerrainEditPerfTests(unittest.TestCase):
    def test_initial_compile_is_allowed_but_excluded_from_edit_timings(self):
        report = analyze(replay())
        self.assertTrue(report["passed"])
        self.assertEqual(report["logged_edit_frame_ms"]["max"], 20)

    def test_recompiling_an_unchanged_tree_is_red(self):
        report = analyze(
            replay(
                "[00:00:02.050 INFO test] [TREE][RASTER_STATIC] revision=2 compile_ms=80"
            )
        )
        self.assertFalse(report["passed"])
        self.assertEqual(report["during_edit_rebuilds"], 1)

    def test_missing_edits_tree_or_errors_cannot_pass(self):
        for text in [
            replay().replace("applied shore-dig-b", "not-applied"),
            replay().replace("[TREE][RASTER_STATIC] revision=", "not-a-tree="),
            replay() + "\nERROR: renderer failed",
        ]:
            self.assertFalse(analyze(text)["passed"])

    def test_edit_window_crosses_midnight(self):
        text = replay().replace("00:00:00.", "23:59:57.")
        text = text.replace("00:00:02.", "23:59:59.")
        text = text.replace("00:00:04.", "00:00:01.")
        text = text.replace("00:00:06.", "00:00:03.")
        self.assertTrue(analyze(text)["passed"])
        self.assertEqual(analyze(text)["logged_edit_frame_ms"]["max"], 20)


if __name__ == "__main__":
    unittest.main()
