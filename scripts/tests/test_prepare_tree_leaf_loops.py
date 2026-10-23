from __future__ import annotations

import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import prepare_tree_leaf_loops as loops


class TreeLeafLoopTests(unittest.TestCase):
    def test_level_matched_loop_keeps_contiguous_source_boundary(self) -> None:
        source = [0.01 * math.sin(i * 0.37) + 0.004 for i in range(1000)]
        output = loops.prepare(source, 100)
        self.assertEqual(len(output), 900)
        rms = math.sqrt(sum(x * x for x in output) / len(output))
        self.assertAlmostEqual(20 * math.log10(rms), loops.TARGET_RMS_DB)
        # New last and first come from adjacent samples 899 and 900, not an arbitrary cut.
        mean = sum(source) / len(source)
        gain = output[-1] / (source[899] - mean)
        self.assertAlmostEqual(output[0], (source[900] - mean) * gain)
        self.assertAlmostEqual(output[99], (source[99] - mean) * gain)
        self.assertAlmostEqual(output[100], (source[100] - mean) * gain)

    def test_invalid_or_silent_source_is_not_silently_normalized(self) -> None:
        for source, overlap in [([0.0] * 100, 10), ([1.0] * 100, 10), ([0.0] * 10, 5), ([0.0] * 100, 1)]:
            with self.assertRaises(ValueError):
                loops.prepare(source, overlap)

    def test_preparation_is_deterministic(self) -> None:
        source = [0.01 * math.sin(i * 0.71) for i in range(500)]
        self.assertEqual(loops.prepare(source, 20), loops.prepare(source, 20))


if __name__ == "__main__":
    unittest.main()
