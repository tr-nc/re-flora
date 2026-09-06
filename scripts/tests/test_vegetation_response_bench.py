import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("vegetation_bench", Path(__file__).resolve().parents[1] / "analyze_vegetation_response_bench.py")
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


def fixture():
    lines = ["Hidden window render extent is 1600x1000", "[VEGETATION_RESPONSE][MEMORY] mode=all frame_slot=0 capacity=4096 buffer_bytes=786464", "[VEGETATION_RESPONSE][BENCH] phase=sample app_frame=600 flora=[1,2,3,4,5] leaves=6 apples=7"]
    for frame in range(608, 2600):
        lines.append(f"[PERF][GPU_FRAME_SCOPE] frame {frame} dropped=0 frame.render=2000us vegetation_response.pass=12us graphics.flora=100us graphics.leaves=40us graphics.apples=13us graphics.leaf_lighting_cache=20us")
        lines.append(f"[PERF][CPU_FRAME_SCOPE] frame {frame} frame.cpu_total=4000us render.record=1000us render.acquire=2000us")
    lines.extend(["[VEGETATION_RESPONSE][BENCH] phase=complete app_frame=2600 flora=[1,2,3,4,5] leaves=6 apples=7", "Application exited successfully"])
    return "\n".join(lines)


class EvidenceTest(unittest.TestCase):
    def test_complete_fixture(self):
        result = bench.analyze_text(fixture())
        self.assertEqual(result["frames"], 1992)
        self.assertEqual(result["metrics_us"]["GPU.frame.render"]["median"], 2000)

    def test_rejects_invalid_raw_sample_missing_frame_and_incomplete_run(self):
        source = fixture()
        mutations = [source.replace("frame.render=2000us", "frame.render=NaNus", 1), source.replace("frame.render=2000us", "frame.render=-1us", 1), source.replace("dropped=0", "dropped=1", 1), source.replace("frame 608 ", "frame 607 ", 1), source.replace("leaves=6 apples=7", "leaves=8 apples=7", 1), source.replace("Application exited successfully", "")]
        for altered in mutations:
            with self.subTest():
                self.assertNotEqual(altered, source)
                with self.assertRaises((AssertionError, ValueError)):
                    bench.analyze_text(altered)


if __name__ == "__main__":
    unittest.main()
