from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
RUNNER = SCRIPTS / "check_ddgi_lifecycle_acceptance.sh"


class CheckDdgiLifecycleAcceptanceTests(unittest.TestCase):
    def run_runner(
        self, *arguments: str, environment: dict[str, str] | None = None
    ) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        if environment is not None:
            env.update(environment)
        return subprocess.run(
            [str(RUNNER), *arguments],
            check=False,
            capture_output=True,
            text=True,
            env=env,
        )

    def test_dry_run_prints_both_radiance_spacings_and_density_without_side_effects(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            temp_root = Path(directory)
            fake_bin = temp_root / "bin"
            fake_bin.mkdir()
            fake_cargo = fake_bin / "cargo"
            fake_cargo.write_text("#!/usr/bin/env bash\nexit 99\n")
            fake_cargo.chmod(0o755)
            output_root = temp_root / "outputs"

            result = self.run_runner(
                "--dry-run",
                environment={
                    "DDGI_LIFECYCLE_OUTPUT_DIR": str(output_root),
                    "PATH": f"{fake_bin}:{os.environ['PATH']}",
                },
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(output_root.exists())

        output = result.stdout
        self.assertEqual(output.count("cargo run --quiet --release"), 3)
        self.assertIn(
            "--environment-lighting-test-scene radiance-changes", output
        )
        self.assertEqual(
            output.count("--environment-lighting-test-scene radiance-changes"), 2
        )
        self.assertEqual(output.count("--environment-irradiance-capture-target published"), 2)
        self.assertIn("--environment-probe-spacing-voxels 32", output)
        self.assertIn("--environment-probe-spacing-voxels 16", output)
        self.assertIn("--environment-lighting-test-scene density-changes", output)
        self.assertIn(
            "[DDGI_LIFECYCLE] group=DENSITY scene=density-changes target=e0 running",
            output,
        )
        self.assertNotIn("--environment-probe-rebuild-spacing-voxels", output)
        self.assertIn("--environment-irradiance-capture-target e0", output)
        self.assertIn("[DDGI_LIFECYCLE] dry-run complete scenarios=3", output)

    def test_rejects_unknown_arguments_without_running_cargo(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            temp_root = Path(directory)
            fake_bin = temp_root / "bin"
            fake_bin.mkdir()
            fake_cargo = fake_bin / "cargo"
            fake_cargo.write_text("#!/usr/bin/env bash\nexit 99\n")
            fake_cargo.chmod(0o755)

            for arguments in (("--verbose",), ("--dry-run", "extra")):
                with self.subTest(arguments=arguments):
                    result = self.run_runner(
                        *arguments,
                        environment={"PATH": f"{fake_bin}:{os.environ['PATH']}"},
                    )
                    self.assertEqual(result.returncode, 2)
                    self.assertIn("usage:", result.stderr)


if __name__ == "__main__":
    unittest.main()
