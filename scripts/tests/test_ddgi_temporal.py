import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from scripts import ddgi_temporal as temporal


class TemporalTests(unittest.TestCase):
    def test_spike_not_hidden_by_mean(self):
        result = temporal.frame_delta(bytes(300), bytes([255] * 3 + [0] * 297), 3)
        self.assertEqual(result['mean'], 2.55)
        self.assertEqual(result['max'], 255)
        self.assertEqual(result['spike_area'], .01)

    def test_rgb_excludes_alpha_and_rejects_mismatched_frames(self):
        self.assertEqual(temporal.frame_delta(bytes(3), bytes([3, 6, 9]), 3)['mean'], 6)
        for a, b in [(b'', b''), (bytes(3), bytes(6)), (bytes(4), bytes(4))]:
            with self.assertRaises(ValueError):
                temporal.frame_delta(a, b, 3)

    def test_capture_record_time_not_completion_selects_interval(self):
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory) / 'frame'
            paths = [Path(str(prefix) + f'.{i:06}.png') for i in range(3)]
            for path in paths:
                path.touch()
            text = '[DDGI_SUSTAINED] begin\n'
            text += ''.join(f'[SCREENSHOT] Capturing after {i * .1}s to {path}\n'
                            for i, path in enumerate(paths))
            text += '[DDGI_SUSTAINED] end\n'
            text += ''.join(f'[SCREENSHOT] Saved 16x9 to {path}\n' for path in paths)
            with patch.object(temporal, 'read_roi', side_effect=[bytes(3), bytes([20]*3), bytes(3)]):
                report = temporal.analyze(text, prefix, {'wall': (0, 0, 1, 1)})
            self.assertEqual(report['rois']['wall']['temporal']['mean']['max'], 20)
            self.assertEqual(report['captures'], 3)
            with self.assertRaisesRegex(ValueError, 'missing completed'):
                temporal.analyze(text.split('[SCREENSHOT] Saved')[0], prefix, {})
            paths[0].unlink()
            with self.assertRaisesRegex(ValueError, 'missing completed'):
                temporal.analyze(text, prefix, {})

    def test_no_edit_interval_fails_closed(self):
        with self.assertRaises(ValueError):
            temporal.analyze('', Path('frame'), {})


class ExperimentConfigTests(unittest.TestCase):
    def test_modes_are_explicit_and_other_controls_unchanged(self):
        from scripts.check_ddgi_cave_edits import configure_experiments
        text = Path("config/gui.toml").read_text()
        enabled = configure_experiments(text, True, True)
        self.assertNotEqual(enabled, text)
        self.assertEqual(configure_experiments(enabled), text)
        sequence = configure_experiments(text, True, False)
        self.assertIn('id = "ddgi_continuous_sampling"', sequence)
        self.assertEqual(configure_experiments(sequence), text)

    def test_missing_control_cannot_modify_the_following_control(self):
        from scripts.check_ddgi_cave_edits import configure_experiments
        text = Path("config/gui.toml").read_text().replace('id = "ddgi_aggregate_history"', 'id = "unrelated"')
        with self.assertRaises(ValueError):
            configure_experiments(text, True, True)


if __name__ == '__main__':
    unittest.main()
