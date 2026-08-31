from __future__ import annotations

import unittest
import tempfile
from pathlib import Path

import sys


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from ddgi_evidence.model import ScenarioValidation, ValidateScenarioLog  # noqa: E402
from ddgi_evidence.validation import (  # noqa: E402
    RadianceLifecycleError,
    validate_process_evidence,
    validate_scenario_log,
)


VALID_RADIANCE_STREAM = """
[DDGI_ACCEPT][RADIANCE] checkpoint=r1-terminal field_serial=10 geometry_revision=7 radiance_revision=1 spacing_voxels=32 state=Converged update_epoch=8 source_field_serial=9 source_radiance_revision=1 source_state=Converging source_update_epoch=7
[DDGI_ACCEPT][RADIANCE] checkpoint=baseline mutation_frame=None capture_frame=100 active_field_serial=10 active_radiance_revision=1 building_field_serial=0 builder_latched_radiance_revision=None live_radiance_revision=1 latest_radiance_revision=Some(1)
[ENV_IRRADIANCE_CAPTURE] checkpoint target=published build_token_serial=5 generation_token_serial=5 epoch_zero_field_serial=10 field_serial=10 source_field_serial=9 geometry_revision=7 radiance_revision=1 spacing_voxels=32 state=Converged update_epoch=8 publication=Published
[DDGI_ACCEPT][RADIANCE] mutation=r2 after_render_frame=100 first_affected_render_frame=101 expected_radiance_revision=2
[DDGI_ACCEPT][RADIANCE] checkpoint=r2-next-frame mutation_frame=Some(100) capture_frame=101 active_field_serial=10 active_radiance_revision=1 building_field_serial=11 builder_latched_radiance_revision=Some(2) live_radiance_revision=2 latest_radiance_revision=Some(2)
[ENV_IRRADIANCE_CAPTURE] checkpoint target=published build_token_serial=5 generation_token_serial=5 epoch_zero_field_serial=10 field_serial=10 source_field_serial=9 geometry_revision=7 radiance_revision=1 spacing_voxels=32 state=Converged update_epoch=8 publication=Published
[DDGI_ACCEPT][RADIANCE] checkpoint=r2-midflight active_field_serial=10 active_radiance_revision=1 building_field_serial=11 building_radiance_revision=2 building_update_epoch=0 source_field_serial=10 progress=100/1000 old_field_visible=true
[DDGI_ACCEPT][RADIANCE] mutation=r3 frame=102 expected_radiance_revision=3
[DDGI_ACCEPT][RADIANCE] checkpoint=r3-observed latest_radiance_revision=3 inflight_field_serial=11 inflight_radiance_revision=2 field_serial_allocated=false
[DDGI_ACCEPT][RADIANCE] mutation=r4 after_render_frame=103 first_affected_render_frame=104 expected_radiance_revision=4 inflight_field_serial=11 immutable_inflight_radiance_revision=2 latest_coalescing_pending=true
[DDGI_ACCEPT][RADIANCE] checkpoint=r4-next-frame mutation_frame=Some(103) capture_frame=104 active_field_serial=10 active_radiance_revision=1 building_field_serial=11 builder_latched_radiance_revision=Some(2) live_radiance_revision=4 latest_radiance_revision=Some(4)
[ENV_IRRADIANCE_CAPTURE] checkpoint target=published build_token_serial=5 generation_token_serial=5 epoch_zero_field_serial=10 field_serial=10 source_field_serial=9 geometry_revision=7 radiance_revision=1 spacing_voxels=32 state=Converged update_epoch=8 publication=Published
[DDGI][PUBLICATION] serial=11 geometry_revision=7 radiance_revision=2 update_epoch=0 kind=RadianceUpdate latest_transport_revision=Some(4)
[DDGI_ACCEPT][RADIANCE] checkpoint=r4-midflight active_field_serial=11 active_radiance_revision=2 building_field_serial=12 building_radiance_revision=4 building_update_epoch=0 source_field_serial=11 progress=10/1000 r3_coalesced=true old_field_visible=true
[DDGI][PUBLICATION] serial=12 geometry_revision=7 radiance_revision=4 update_epoch=0 kind=RadianceUpdate latest_transport_revision=Some(4)
[DDGI_ACCEPT][RADIANCE] checkpoint=complete field_serial=12 geometry_revision=7 radiance_revision=4 spacing_voxels=32 state=Converging update_epoch=0 source_field_serial=11 source_radiance_revision=2 source_state=Converging source_update_epoch=0
[DDGI_ACCEPT][RADIANCE] complete r3_coalesced=true field_serial_gap_r2_to_r4=1 geometry_unchanged=true spacing_unchanged=true
[DDGI_ACCEPT][RADIANCE] checkpoint=final mutation_frame=None capture_frame=200 active_field_serial=12 active_radiance_revision=4 building_field_serial=0 builder_latched_radiance_revision=None live_radiance_revision=4 latest_radiance_revision=Some(4)
[ENV_IRRADIANCE_CAPTURE] checkpoint target=published build_token_serial=5 generation_token_serial=5 epoch_zero_field_serial=10 field_serial=12 source_field_serial=11 geometry_revision=7 radiance_revision=4 spacing_voxels=32 state=Converging update_epoch=0 publication=Published
""".strip()


class RadianceLifecycleStreamTests(unittest.TestCase):
    def validate(self, text: str) -> dict[str, int]:
        with tempfile.TemporaryDirectory() as temporary:
            console = Path(temporary) / "radiance.console.log"
            console.write_text(text, encoding="utf-8")
            return validate_scenario_log(
                ValidateScenarioLog(
                    ScenarioValidation.RADIANCE_STREAM,
                    console,
                    32,
                )
            )

    def test_valid_stream_returns_the_final_capture_identity(self) -> None:
        facts = self.validate(VALID_RADIANCE_STREAM)
        self.assertEqual(
            facts,
            {
                "field_serial": 12,
                "source_field_serial": 11,
                "geometry_revision": 7,
                "build_token_serial": 5,
            },
        )

    def assert_rejected(self, text: str, message: str) -> None:
        with self.assertRaisesRegex(RadianceLifecycleError, message):
            self.validate(text)

    def test_rejects_out_of_order_and_duplicate_checkpoints(self) -> None:
        lines = VALID_RADIANCE_STREAM.splitlines()
        lines[3], lines[4] = lines[4], lines[3]
        self.assert_rejected("\n".join(lines), "out of order")
        duplicated = VALID_RADIANCE_STREAM.replace(
            lines[0], f"{lines[0]}\n{lines[0]}", 1
        )
        self.assert_rejected(duplicated, "duplicate|out of order")

    def test_rejects_identity_drift_and_obsolete_r3_publication(self) -> None:
        self.assert_rejected(
            VALID_RADIANCE_STREAM.replace(
                "building_field_serial=12 building_radiance_revision=4",
                "building_field_serial=99 building_radiance_revision=4",
            ),
            "field.*drift|r4",
        )
        obsolete = VALID_RADIANCE_STREAM.replace(
            "[DDGI][PUBLICATION] serial=12 geometry_revision=7 radiance_revision=4",
            "[DDGI][PUBLICATION] serial=99 geometry_revision=7 radiance_revision=3 "
            "update_epoch=0 kind=RadianceUpdate\n"
            "[DDGI][PUBLICATION] serial=12 geometry_revision=7 radiance_revision=4",
        )
        self.assert_rejected(obsolete, "obsolete|out of order")

    def test_rejects_direct_sun_next_frame_timing_drift(self) -> None:
        self.assert_rejected(
            VALID_RADIANCE_STREAM.replace(
                "checkpoint=r4-next-frame mutation_frame=Some(103) capture_frame=104",
                "checkpoint=r4-next-frame mutation_frame=Some(103) capture_frame=105",
            ),
            "first rendered frame",
        )


class ProcessEvidenceTests(unittest.TestCase):
    @staticmethod
    def logged(module: str, payload: str) -> str:
        return f"[12:34:56.789 INFO {module}] {payload}\n"

    def fixture(self, root: Path, *, dirty_console: bool = False) -> tuple[Path, Path]:
        run_log = (root / "canonical run.log").resolve()
        marker = self.logged("re_flora", f"[RUN_LOG] path={run_log}")
        events = "".join(
            (
                self.logged(
                    "re_flora::app::core::environment_lighting_test_scene",
                    "[ENV_LIGHT_TEST] static terrain ready case=sealed "
                    "terrain_revision=2 settling_frames=2",
                ),
                self.logged(
                    "re_flora::tracer",
                    "[DDGI] initialization requested terrain_revision=2 "
                    "spacing_voxels=32",
                ),
                self.logged(
                    "re_flora::app::core::environment_lighting_test_scene",
                    "[ENV_LIGHT_TEST] first DDGI build verified "
                    "build_token_serial=1 geometry_revision=2 "
                    "visible_terrain_publication_revision=2",
                ),
                self.logged(
                    "re_flora::app::core::environment_irradiance_capture",
                    "[ENV_IRRADIANCE_CAPTURE] saved path=/tmp/capture.rfirr",
                ),
                self.logged(
                    "re_flora::app::core",
                    "[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run",
                ),
            )
        )
        run_log.write_text(marker + events, encoding="utf-8")
        console = root / "capture.console.log"
        console.write_text(
            marker + events + ("VUID-dirty\n" if dirty_console else ""),
            encoding="utf-8",
        )
        return console, run_log

    def test_process_evidence_binds_clean_console_to_canonical_run_log(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            console, run_log = self.fixture(Path(temporary))
            self.assertEqual(
                validate_process_evidence(
                    console, require_test_scene_startup=True
                ),
                run_log,
            )

    def test_process_evidence_rejects_a_dirty_console(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            console, _ = self.fixture(Path(temporary), dirty_console=True)
            with self.assertRaisesRegex(ValueError, "fatal or validation"):
                validate_process_evidence(
                    console, require_test_scene_startup=True
                )


if __name__ == "__main__":
    unittest.main()
