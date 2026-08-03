from __future__ import annotations

import argparse
import sys
import unittest

from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import benchmark_ddgi_publication  # noqa: E402


LOG_TEXT = """
[INFO] Selected physical device: Fixture GPU
[INFO] [DDGI][PUBLICATION_TIMING] token_serial=7 descriptor_rebind_ms=0.125 resource_swap_ms=0.250 total_publication_ms=0.400 descriptor_generation=3
[INFO] [PERF][GPU_FRAME_SCOPE] frame 1 scopes=2 dropped=0 frame.render=120us tracer.render=60us
[INFO] [DDGI][PUBLICATION_TIMING] token_serial=8 descriptor_rebind_ms=0.100 resource_swap_ms=0.200 total_publication_ms=0.350 descriptor_generation=4
[INFO] [PERF][GPU_FRAME_SCOPE] frame 2 scopes=2 dropped=0 frame.render=100us tracer.render=55us
"""


class BenchmarkDdgiPublicationTests(unittest.TestCase):
    def test_parses_publication_and_frame_markers(self) -> None:
        parsed = benchmark_ddgi_publication.parse_log(LOG_TEXT)

        self.assertEqual(parsed["device"], "Fixture GPU")
        self.assertEqual(parsed["frame_render_us"], [120, 100])
        self.assertEqual(parsed["publications"][0]["token_serial"], 7)
        self.assertAlmostEqual(parsed["publications"][1]["total_publication_ms"], 0.35)

    def test_rejects_logs_without_publication(self) -> None:
        with self.assertRaisesRegex(ValueError, "no DDGI publication"):
            benchmark_ddgi_publication.parse_log("[INFO] Selected physical device: Fixture GPU\n")

    def test_command_records_reproducible_capture_target(self) -> None:
        args = argparse.Namespace(spacing_voxels=32, auto_exit=90.0)
        command = benchmark_ddgi_publication.command(args, Path("sample.rfirr"))

        self.assertEqual(command[:4], ["cargo", "run", "--quiet", "--release"])
        self.assertIn("radiance-changes", command)
        self.assertEqual(command[-2:], ["--environment-irradiance-capture-target", "published"])


if __name__ == "__main__":
    unittest.main()
