import contextlib
import io
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import benchmark_tree_update as bench


class TreeBenchmarkTests(unittest.TestCase):
    def test_only_static_and_smooth_modes_remain(self):
        parser = bench.build_parser()
        args = parser.parse_args(['--output', 'target/unused-bench'])
        self.assertEqual(args.modes, ['static', 'smooth'])
        self.assertEqual(bench.MODES, {'static': False, 'smooth': True})
        with contextlib.redirect_stderr(io.StringIO()) as errors, self.assertRaises(SystemExit):
            parser.parse_args(['--output', 'target/unused-bench', '--modes', 'blocks'])
        self.assertTrue(errors.getvalue().startswith('error:'))
        self.assertIn('use smooth', errors.getvalue())

    def test_capture_rejects_removed_flag_and_advertises_wind(self):
        script = Path(__file__).resolve().parents[1] / 'check_raster_tree_static.py'
        help_result = subprocess.run([sys.executable, str(script), '--help'], capture_output=True, text=True, check=False)
        self.assertEqual(help_result.returncode, 0)
        self.assertIn('Use --wind', help_result.stdout)
        removed = subprocess.run([sys.executable, str(script), '--axis-aligned'], capture_output=True, text=True, check=False)
        self.assertEqual(removed.returncode, 2)
        self.assertIn('unrecognized arguments: --axis-aligned', removed.stderr)
        self.assertIn('--wind', removed.stderr)

    def test_wall_clock_warmup_midnight_and_frame_join(self):
        text = '''
[23:59:58.000 INFO fixture] [PERF][FRAME] frame 1 total 900.00ms
[23:59:59.000 INFO fixture] [PERF][FRAME] frame 2 total 10.00ms
[00:00:01.100 INFO fixture] [PERF][TREE_UPDATE] frame=3 cpu_skin_us=125.00 pose_gpu_us=Some(70.5) total_cpu_us=500.0
[00:00:01.100 INFO fixture] [PERF][GPU_FRAME_SCOPE] frame 3 scopes=2 tree_surface_update.pass=65us frame.render=8000us
[00:00:01.100 INFO fixture] [PERF][FRAME] frame 3 total 11.00ms
[00:00:02.000 INFO fixture] [PERF][TREE_UPDATE] frame=4 cpu_skin_us=75.00 pose_gpu_us=None total_cpu_us=300.0
[00:00:02.000 INFO fixture] [PERF][FRAME] frame 4 total 9.00ms
'''
        samples = bench.samples_from_log(text, 3.)
        result = bench.summarize(samples)
        self.assertEqual(result['samples'], 2)
        self.assertEqual(result['frame_median_ms'], 10.)
        self.assertEqual(result['tree_update_us']['cpu_skin_us']['median'], 100.)
        self.assertEqual(result['tree_update_us']['pose_gpu_us']['samples'], 1)
        self.assertEqual(result['gpu_scope_us']['tree_surface_update.pass']['median'], 65.)

    def test_old_logs_without_new_instrumentation_remain_supported(self):
        samples = bench.samples_from_log('[01:00:00.000 INFO fixture] [PERF][FRAME] frame 2 total 12.00ms', 0.)
        result = bench.summarize(samples)
        self.assertEqual(result['frame_median_ms'], 12.)
        self.assertEqual(result['tree_update_us'], {})

    def test_help_and_errors_explain_mode_difference_and_recovery(self):
        parser = bench.build_parser()
        help_text = parser.format_help()
        self.assertIn('Static disables wind', help_text)
        self.assertIn('--repeats 3', help_text)
        self.assertIn('--modes', help_text)
        with contextlib.redirect_stderr(io.StringIO()) as errors, self.assertRaises(SystemExit) as exit_code:
            parser.parse_args(['--output', 'target/unused-bench', '--modes', 'invalid'])
        self.assertEqual(exit_code.exception.code, 2)
        self.assertTrue(errors.getvalue().startswith('error:'))
        self.assertIn('static', errors.getvalue())
        self.assertIn('--repeats', errors.getvalue())


if __name__ == '__main__':
    unittest.main()
