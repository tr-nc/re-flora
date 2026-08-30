from __future__ import annotations

from dataclasses import replace
import math
import struct
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import analyze_environment_irradiance_capture as analyzer  # noqa: E402
import validate_ddgi_radiance_lifecycle as validator  # noqa: E402


class ValidateDdgiRadianceLifecycleTests(unittest.TestCase):
    def test_lifecycle_validator_requires_v10_owner_evidence(self) -> None:
        fixture_hex = (
            Path(__file__).with_name("fixtures") / "ddgi_filter_evidence_v10.hex"
        ).read_text()
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "rust-producer-v10.rfirr"
            capture_path.write_bytes(bytes.fromhex(fixture_hex))
            capture = analyzer.load_capture(capture_path)

        failures: list[str] = []
        validator.require_v10_capture(capture, "golden", failures)
        self.assertEqual(failures, [])

        failures = []
        validator.require_v10_capture(replace(capture, version=9), "old", failures)
        self.assertEqual(failures, ["old: capture is not v10"])

    def test_required_planes_reject_a_nan_at_the_same_outside_roi_pixel(self) -> None:
        finite_plane = b"".join(
            struct.pack("<4f", *pixel)
            for pixel in ((0.25, 0.5, 0.75, 1.0), (10.0, 20.0, 30.0, 0.0))
        )
        nonfinite_plane = finite_plane[:16] + struct.pack(
            "<4f", math.nan, 20.0, 30.0, 0.0
        )
        plane_fields = (
            "payload",
            "world_payload",
            "direct_light_payload",
            "terrain_shadow_receiver_payload",
            "direct_sun_shadow_payload",
        )
        base = analyzer.Capture(
            path=Path("outside-roi.rfirr"),
            version=10,
            width=2,
            height=1,
            backend=1,
            spacing_voxels=16,
            debug_view=0,
            payload=finite_plane,
            world_payload=finite_plane,
            direct_light_payload=finite_plane,
            terrain_shadow_receiver_payload=finite_plane,
            direct_sun_shadow_payload=finite_plane,
            plane_count=5,
        )
        for plane_field in plane_fields:
            with self.subTest(plane=plane_field):
                failures: list[str] = []
                validator.require_required_planes_finite(
                    replace(base, **{plane_field: nonfinite_plane}),
                    "baseline",
                    failures,
                )
                self.assertEqual(
                    failures,
                    ["baseline: required capture planes contain non-finite values"],
                )


if __name__ == "__main__":
    unittest.main()
