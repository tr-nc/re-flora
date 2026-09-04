from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import run_slang_tests as runner  # noqa: E402


class RunSlangTestsTests(unittest.TestCase):
    def test_generated_test_uses_the_configured_slang_runtime_directory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            test_root = Path(directory) / "tests"
            module_root = Path(directory) / "modules"
            slang_root = Path(directory) / "slang"
            test_root.mkdir()
            module_root.mkdir()
            (slang_root / "lib").mkdir(parents=True)
            (test_root / "policy.slang").write_text("", encoding="utf-8")
            calls: list[mock._Call] = []

            def record_run(*args: object, **kwargs: object) -> mock.Mock:
                calls.append(mock.call(*args, **kwargs))
                return mock.Mock(returncode=0)

            with (
                mock.patch.object(runner, "ROOT", Path(directory)),
                mock.patch.object(runner, "TEST_ROOT", test_root),
                mock.patch.object(runner, "MODULE_ROOT", module_root),
                mock.patch.object(runner, "find_slangc", return_value=slang_root / "bin/slangc"),
                mock.patch.object(runner.sys, "platform", "darwin"),
                mock.patch.dict(
                    runner.os.environ,
                    {
                        "SLANG_LIB": str(slang_root / "lib/libslang.dylib"),
                        "DYLD_LIBRARY_PATH": "/opt/vulkan/lib",
                    },
                    clear=False,
                ),
                mock.patch.object(runner.subprocess, "run", side_effect=record_run),
            ):
                self.assertEqual(runner.main(), 0)

        execute_environment = calls[1].kwargs["env"]
        self.assertEqual(
            execute_environment["DYLD_LIBRARY_PATH"],
            os.pathsep.join((str(slang_root / "lib"), "/opt/vulkan/lib")),
        )

    def test_runtime_library_search_variable_is_host_specific(self) -> None:
        self.assertEqual(runner.runtime_library_search_variable("nt", "win32"), "PATH")
        self.assertEqual(
            runner.runtime_library_search_variable("posix", "darwin"),
            "DYLD_LIBRARY_PATH",
        )
        self.assertEqual(
            runner.runtime_library_search_variable("posix", "linux"),
            "LD_LIBRARY_PATH",
        )

    def test_compile_and_execution_subprocesses_have_bounded_timeouts(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            test_root = Path(directory) / "tests"
            module_root = Path(directory) / "modules"
            test_root.mkdir()
            module_root.mkdir()
            (test_root / "policy.slang").write_text("", encoding="utf-8")
            calls: list[mock._Call] = []

            def record_run(*args: object, **kwargs: object) -> mock.Mock:
                calls.append(mock.call(*args, **kwargs))
                return mock.Mock(returncode=0)

            with (
                mock.patch.object(runner, "ROOT", Path(directory)),
                mock.patch.object(runner, "TEST_ROOT", test_root),
                mock.patch.object(runner, "MODULE_ROOT", module_root),
                mock.patch.object(runner, "find_slangc", return_value=Path("/tool/slangc")),
                mock.patch.object(runner.subprocess, "run", side_effect=record_run),
            ):
                self.assertEqual(runner.main(), 0)

        self.assertEqual(len(calls), 2)
        compile_call, execute_call = calls
        self.assertEqual(compile_call.kwargs["timeout"], 120)
        self.assertIn("-std", compile_call.args[0])
        self.assertIn("2025", compile_call.args[0])
        self.assertEqual(execute_call.kwargs["timeout"], 30)
        self.assertIn("env", execute_call.kwargs)


if __name__ == "__main__":
    unittest.main()
