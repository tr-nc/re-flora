"""Run the real native validation commands against a deterministic app adapter.

No GPU, cargo build, screenshots, or timing in these tests: only the shell gates
are under test. The native app fixtures remain responsible for gameplay evidence.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
COMMANDS = {
    "validate-pavement.sh": "pavement/default.log",
    "validate-pavement-prototype-native.sh": "pavement-prototype-native/seed.log",
    "validate-pavement-pointer.sh": "pavement-pointer/native.log",
    "validate-pavement-continuity.sh": "pavement-continuity/native.log",
    "validate-pavement-recovery.sh": "pavement-recovery/native.log",
    "validate-stone-placement.sh": "stone-placement/default.log",
}


def successful_app_output() -> str:
    lines = [
        "Application exited successfully",
        "seed=verified three_surfaces=true",
        "kind=Dirt atlas_unchanged=true flora_removed=1",
        "restart=verified cells=1",
        "view=bends camera_transient=true",
        "backdrag_then_cancel=atlas_identical redraw_not_required=true sound=muted",
        "smoothing_range=2 sharp_corner=published terrain_preflight=true",
        "restart=verified atlas_crc=1 flora_count=1 authored_identity_and_seed=true",
        "complete=true cases=2 settings_restored=true",
        "complete=true cases=2 single_path_per_drag=true",
        "dirt_candidate=published surface_sampling=retained atlas=unchanged",
        (
            "protected_targets=discarded live_atlas_guard=retained rejected_atlas=exact "
            "clear_target=published fixed_history=valid_only no_retreat=true same_gesture=true"
        ),
        "garden=flat fixture_only=true ordinary_pointer_garden=unchanged",
        "[STONE_PLACEMENT] seed=42 next_seed=43",
        "[STONE_PLACEMENT] seed=43 next_seed=44",
        "restart=verified terrain_bytes=exact rock_and_slab=true dig=verified",
    ]
    for kind in ("Slabs", "Brick", "Dirt"):
        lines.append(
            f"kind={kind} sound=dig_shared release=muted cancel=muted idle_cancel=preserved"
        )
    for kind in ("Slabs", "Brick"):
        lines.extend(
            [
                f"kind={kind} retrace=no_extra_layer",
                (
                    f"kind={kind} published_retracted=true rejected_tail=true stationary=true "
                    "branch=true empty_click_suppressed=true soil_and_completed_path=atlas_identical "
                    "flora_count_and_identity=restored"
                ),
                f"kind={kind} drag=verified window_events=true preview_retained=true along_ground=true",
                (
                    f"kind={kind} red_preview=true contiguous_prefix=true terrain_after_block=unchanged "
                    "held_no_restart=true duplicate_down=ignored release=muted"
                ),
                (
                    f"kind={kind} intent_origin=retained invalid_tail=red paused_atlas=exact "
                    "retry=valid_target no_retreat=true same_gesture=true contiguous_courses=true "
                    "baked_prefix=exact"
                ),
            ]
        )
        for sign in (1, -1):
            for radius, courses in (("0.18", 6), ("0.1", 4)):
                prefix = f"PAVEMENT_BEND_REVIEW kind={kind} sign={sign} radius={radius}"
                lines.append(
                    f"{prefix} courses={courses} complete=true candidate_targets=all_accepted"
                )
                lines.append(f"{prefix} retrace=atlas_identical")
    return "\n".join(lines) + "\n"


class NativeValidationCommandsTests(unittest.TestCase):
    def run_command(
        self,
        command: str,
        output: str,
        *,
        delete_log: bool = False,
        app_exit: int = 0,
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "scripts").mkdir()
            (root / "config").mkdir()
            (root / "config/gui.toml").write_text(
                "# unchanged fixture\n", encoding="utf-8"
            )
            for name in (command, "runtime_log_diagnostics.py"):
                shutil.copy2(SCRIPTS / name, root / "scripts" / name)
            fake_bin = root / "bin"
            fake_bin.mkdir()
            cargo = fake_bin / "cargo"
            cargo.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
            cargo.chmod(0o755)
            app = root / "target/release/re-flora"
            app.parent.mkdir(parents=True)
            app.write_text(
                "#!/usr/bin/env bash\n"
                'printf "%s" "$FIXTURE_OUTPUT"\n'
                'if [[ -n "$FIXTURE_DELETE" ]]; then rm -f -- "$FIXTURE_DELETE"; fi\n'
                'exit "$FIXTURE_EXIT"\n',
                encoding="utf-8",
            )
            app.chmod(0o755)
            return subprocess.run(
                ["bash", str(root / "scripts" / command)],
                cwd=root,
                env={
                    **os.environ,
                    "PATH": str(fake_bin) + os.pathsep + os.environ["PATH"],
                    "FIXTURE_OUTPUT": output,
                    "FIXTURE_DELETE": str(root / "target" / COMMANDS[command])
                    if delete_log
                    else "",
                    "FIXTURE_EXIT": str(app_exit),
                },
                capture_output=True,
                text=True,
                check=False,
            )

    def test_successful_complete_clean_runs_are_accepted(self) -> None:
        for command in COMMANDS:
            with self.subTest(command=command):
                result = self.run_command(command, successful_app_output())
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("Artifacts:", result.stdout)

    def test_fatal_diagnostic_cannot_be_hidden_by_success_markers_and_zero_exit(
        self,
    ) -> None:
        for command in COMMANDS:
            for fatal in (
                " ERROR injected",
                "VUID-injected",
                "thread 'main' panicked",
                "DEVICE_LOST",
            ):
                with self.subTest(command=command, fatal=fatal):
                    result = self.run_command(
                        command, successful_app_output() + fatal + "\n"
                    )
                    self.assertNotEqual(
                        result.returncode, 0, "fatal native run was accepted"
                    )
                    self.assertNotIn("Artifacts:", result.stdout)

    def test_missing_log_incomplete_run_and_process_failure_are_rejected(self) -> None:
        for command in COMMANDS:
            for case in ("missing-log", "incomplete", "process-failure"):
                with self.subTest(command=command, case=case):
                    output = successful_app_output()
                    if case == "incomplete":
                        output = output.replace(
                            "Application exited successfully", "rendering started"
                        )
                    result = self.run_command(
                        command,
                        output,
                        delete_log=case == "missing-log",
                        app_exit=7 if case == "process-failure" else 0,
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertNotIn("Artifacts:", result.stdout)

    def test_unrequested_startup_work_is_rejected_even_without_errors(self) -> None:
        for command, forbidden in (
            ("validate-pavement.sh", "[PAVEMENT] publish"),
            ("validate-stone-placement.sh", "[STONE_PREVIEW]"),
        ):
            with self.subTest(command=command):
                result = self.run_command(
                    command, successful_app_output() + forbidden + "\n"
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn("Artifacts:", result.stdout)


if __name__ == "__main__":
    unittest.main()
