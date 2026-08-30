from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
VALIDATOR = SCRIPTS / "validate_capture_process_evidence.py"


class ValidateCaptureProcessEvidenceTests(unittest.TestCase):
    @staticmethod
    def logged(module: str, payload: str, *, level: str = "INFO") -> str:
        return f"[12:34:56.789 {level} {module}] {payload}\n"

    def validate(
        self,
        *,
        console_extra: str = "",
        run_log_extra: str = "",
        publication_revision: int = 2,
        initialization_revision: int = 2,
        build_revision: int = 2,
        verified_publication_revision: int = 2,
        order: tuple[str, ...] = ("publication", "initialization", "verification"),
        duplicate_marker: bool = False,
        include_startup: bool = True,
        run_log_name: str = "run.log",
        truncate_run_log_after_marker: bool = False,
        preserve_run_log: bool = False,
        invalid_preserve_target: bool = False,
        binding_style: str = "canonical",
        event_module_overrides: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            run_log = (root / run_log_name).resolve()
            marker_payload = f"[RUN_LOG] path={run_log}"
            marker = {
                "canonical": self.logged("re_flora", marker_payload),
                "raw": marker_payload + "\n",
                "wrong-level": self.logged("re_flora", marker_payload, level="DEBUG"),
                "wrong-module": self.logged("re_flora::run_log_binding", marker_payload),
                "prefix": self.logged("re_flora", "[FAKE] " + marker_payload),
                "suffix": self.logged("re_flora", marker_payload + " trailing-junk"),
            }[binding_style]
            modules = {
                "publication": "re_flora::app::core::environment_lighting_test_scene",
                "initialization": "re_flora::tracer",
                "verification": "re_flora::app::core::environment_lighting_test_scene",
                "saved": "re_flora::app::core::environment_irradiance_capture",
                "complete": "re_flora::app::core",
            }
            modules.update(event_module_overrides or {})
            events = {
                "publication": self.logged(
                    modules["publication"],
                    "[ENV_LIGHT_TEST] static terrain ready case=sealed "
                    f"terrain_revision={publication_revision} settling_frames=2",
                ),
                "initialization": self.logged(
                    modules["initialization"],
                    "[DDGI] initialization requested "
                    f"terrain_revision={initialization_revision} spacing_voxels=32",
                ),
                "verification": self.logged(
                    modules["verification"],
                    "[ENV_LIGHT_TEST] first DDGI build verified build_token_serial=1 "
                    f"geometry_revision={build_revision} "
                    f"visible_terrain_publication_revision={verified_publication_revision}",
                ),
            }
            startup = "".join(events[name] for name in order) if include_startup else ""
            capture_events = (
                self.logged(
                    modules["saved"],
                    "[ENV_IRRADIANCE_CAPTURE] saved path=/tmp/capture.rfirr",
                )
                + self.logged(
                    modules["complete"],
                    "[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run",
                )
            )
            body = startup + capture_events
            run_log.write_text(
                marker
                + ("" if truncate_run_log_after_marker else body)
                + run_log_extra,
                encoding="utf-8",
            )
            console = root / "console.log"
            console.write_text(
                marker + (marker if duplicate_marker else "") + body + console_extra,
                encoding="utf-8",
            )
            preserved = root / "preserved" / "bound.run.log"
            if invalid_preserve_target:
                preserved.parent.write_text("not a directory", encoding="utf-8")
            result = subprocess.run(
                [
                    "python3",
                    str(VALIDATOR),
                    *(["--require-test-scene-startup"] if include_startup else []),
                    *(
                        ["--preserve-run-log", str(preserved)]
                        if preserve_run_log
                        else []
                    ),
                    str(console),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            if preserve_run_log and result.returncode == 0:
                self.assertEqual(preserved.read_bytes(), run_log.read_bytes())
                self.assertIn(f"preserved_run_log={preserved}", result.stdout)
            return result

    def test_accepts_one_clean_process_bound_log_with_ordered_identity(self) -> None:
        result = self.validate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("status=PASS", result.stdout)

    def test_accepts_a_canonical_process_bound_log_path_with_spaces(self) -> None:
        result = self.validate(run_log_name="run evidence.log")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_nonproduction_binding_logger_lines(self) -> None:
        for style in ("raw", "wrong-level", "wrong-module", "prefix", "suffix"):
            with self.subTest(style=style):
                result = self.validate(binding_style=style)
                self.assertEqual(result.returncode, 1)
                self.assertIn("process-bound", result.stderr)

    def test_rejects_capture_and_startup_events_from_the_wrong_module(self) -> None:
        for event in ("publication", "initialization", "verification", "saved", "complete"):
            with self.subTest(event=event):
                result = self.validate(
                    event_module_overrides={event: "attacker::forged_evidence"}
                )
                self.assertEqual(result.returncode, 1)

    def test_preserves_the_exact_bound_run_log_with_the_capture_artifacts(self) -> None:
        result = self.validate(preserve_run_log=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_fails_closed_when_the_bound_run_log_cannot_be_preserved(self) -> None:
        result = self.validate(
            preserve_run_log=True,
            invalid_preserve_target=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("cannot preserve bound run log", result.stderr)

    def test_generic_binding_does_not_require_test_scene_startup(self) -> None:
        result = self.validate(include_startup=False)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_a_run_log_truncated_after_its_binding_marker(self) -> None:
        result = self.validate(truncate_run_log_after_marker=True)
        self.assertEqual(result.returncode, 1)
        self.assertIn("run log capture saved event", result.stderr)

    def test_rejects_dirty_console_and_dirty_run_log(self) -> None:
        for kwargs in (
            {"console_extra": "VUID-test\n"},
            {"run_log_extra": "stale readback detected\n"},
        ):
            with self.subTest(kwargs=kwargs):
                result = self.validate(**kwargs)
                self.assertEqual(result.returncode, 1)
                self.assertIn("fatal or validation diagnostic", result.stderr)

    def test_rejects_case_and_whitespace_mutations_of_device_lost(self) -> None:
        for diagnostic in ("device lost", "DeViCe\tLoSt", "DEVICE   LOST"):
            for source in ("console_extra", "run_log_extra"):
                with self.subTest(diagnostic=diagnostic, source=source):
                    result = self.validate(**{source: diagnostic + "\n"})
                    self.assertEqual(result.returncode, 1)
                    self.assertIn("fatal or validation diagnostic", result.stderr)

    def test_accepts_benign_error_counters_and_validation_layer_status(self) -> None:
        benign = "errors=0\nvalidation layers enabled\n"
        for source in ("console_extra", "run_log_extra"):
            with self.subTest(source=source):
                result = self.validate(**{source: benign})
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_accepts_only_the_known_cosmetic_color_scheme_portal_timeout(self) -> None:
        benign = self.logged(
            "sctk_adwaita::config",
            "XDG Settings Portal did not return response in time: "
            "timeout: 100ms, key: color-scheme",
            level="ERROR",
        )
        for source in ("console_extra", "run_log_extra"):
            with self.subTest(source=source):
                result = self.validate(**{source: benign})
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_portal_timeout_exception_fails_closed_on_identity_or_payload_drift(self) -> None:
        mutations = (
            self.logged(
                "sctk_adwaita::other",
                "XDG Settings Portal did not return response in time: "
                "timeout: 100ms, key: color-scheme",
                level="ERROR",
            ),
            self.logged(
                "sctk_adwaita::config",
                "XDG Settings Portal did not return response in time: "
                "timeout: 100ms, key: cursor-theme",
                level="ERROR",
            ),
            self.logged(
                "sctk_adwaita::config",
                "XDG Settings Portal did not return response in time: "
                "timeout: 100ms, key: color-scheme VUID-injected",
                level="ERROR",
            ),
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                result = self.validate(console_extra=mutation)
                self.assertEqual(result.returncode, 1)
                self.assertIn("fatal or validation diagnostic", result.stderr)

    def test_rejects_canonical_fatal_diagnostic_forms(self) -> None:
        for diagnostic in (
            "ERROR renderer failed",
            "ERROR_DEVICE_LOST",
            "VK_ERROR_DEVICE_LOST",
            "VK_ERROR_VALIDATION_FAILED_EXT",
            "ERROR_OUT_OF_DEVICE_MEMORY",
            "validation error: bad descriptor",
            "validation failure: bad descriptor",
            "destroyed descriptor set",
            "panic in worker",
            "thread panicked",
            "VUID-vkCmdDraw-test",
            "stale readback",
        ):
            with self.subTest(diagnostic=diagnostic):
                result = self.validate(console_extra=diagnostic + "\n")
                self.assertEqual(result.returncode, 1)

    def test_rejects_missing_or_duplicate_process_binding(self) -> None:
        duplicate = self.validate(duplicate_marker=True)
        self.assertEqual(duplicate.returncode, 1)
        self.assertIn("found 2", duplicate.stderr)

        with tempfile.TemporaryDirectory() as directory:
            console = Path(directory) / "console.log"
            console.write_text("clean but unbound\n", encoding="utf-8")
            missing = subprocess.run(
                ["python3", str(VALIDATOR), str(console)],
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertEqual(missing.returncode, 1)
        self.assertIn("found 0", missing.stderr)

    def test_rejects_revision_or_event_order_drift(self) -> None:
        mismatch = self.validate(build_revision=1)
        self.assertEqual(mismatch.returncode, 1)
        self.assertIn("revisions differ", mismatch.stderr)

        reordered = self.validate(
            order=("initialization", "publication", "verification")
        )
        self.assertEqual(reordered.returncode, 1)
        self.assertIn("must precede", reordered.stderr)


if __name__ == "__main__":
    unittest.main()
