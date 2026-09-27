"""No GPU launch: fixture lock ownership must not change checks or saved config."""

import contextlib
import io
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts import check_ddgi_indirect_response, check_ddgi_sustained_edits

RUNNERS = (check_ddgi_sustained_edits, check_ddgi_indirect_response)


class FixtureLockOwnershipTests(unittest.TestCase):
    def test_default_and_external_lock_keep_failure_cleanup(self):
        for runner in RUNNERS:
            for externally_locked in (False, True):
                with (
                    self.subTest(runner=runner.__name__, external=externally_locked),
                    tempfile.TemporaryDirectory() as directory,
                    contextlib.chdir(directory),
                ):
                    paths = (Path("config/gui.toml"), Path("config/camera_snapshots.toml"))
                    paths[0].parent.mkdir()
                    originals = {path: f"original {path}\n".encode() for path in paths}
                    for path, content in originals.items():
                        path.write_bytes(content)
                    argv = [runner.__file__, "output", "--binary", sys.executable]
                    if externally_locked:
                        argv.append("--gpu-lock-held")
                    with (
                        mock.patch.object(sys, "argv", argv),
                        mock.patch.object(runner, "open", mock.mock_open(), create=True),
                        mock.patch.object(runner.fcntl, "flock") as acquire,
                        mock.patch.object(
                            runner.subprocess,
                            "run",
                            side_effect=RuntimeError("stop before native launch"),
                        ) as launch,
                        self.assertRaisesRegex(RuntimeError, "stop before native launch"),
                    ):
                        runner.main()
                    if externally_locked:
                        acquire.assert_not_called()
                    else:
                        acquire.assert_called_once()
                        self.assertEqual(acquire.call_args.args[1], runner.fcntl.LOCK_EX)
                    launch.assert_called_once()
                    command = launch.call_args.args[0]
                    self.assertIn("--hidden", command)
                    self.assertIn("--mute", command)
                    for path, content in originals.items():
                        self.assertEqual(path.read_bytes(), content)

    def test_help_explains_the_external_ownership_precondition(self):
        for runner in RUNNERS:
            with self.subTest(runner=runner.__name__):
                output = io.StringIO()
                with (
                    mock.patch.object(sys, "argv", [runner.__file__, "--help"]),
                    contextlib.redirect_stdout(output),
                    mock.patch.object(runner.subprocess, "run") as launch,
                    self.assertRaises(SystemExit) as result,
                ):
                    runner.main()
                self.assertEqual(result.exception.code, 0)
                text = " ".join(output.getvalue().split())
                self.assertIn("--gpu-lock-held", text)
                self.assertIn("ONLY when the caller holds", text)
                self.assertIn("flock --close /tmp/re-flora-summer-gpu.lock", text)
                launch.assert_not_called()


if __name__ == "__main__":
    unittest.main()
