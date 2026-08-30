from __future__ import annotations

from dataclasses import replace
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import analyze_environment_irradiance_capture as analyzer  # noqa: E402
import validate_ddgi_radiance_lifecycle as validator  # noqa: E402


class ValidateDdgiRadianceLifecycleTests(unittest.TestCase):
    def test_lifecycle_validator_requires_v9_owner_evidence(self) -> None:
        fixture_hex = (
            Path(__file__).with_name("fixtures") / "ddgi_filter_evidence_v9.hex"
        ).read_text()
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "rust-producer-v9.rfirr"
            capture_path.write_bytes(bytes.fromhex(fixture_hex))
            capture = analyzer.load_capture(capture_path)

        failures: list[str] = []
        validator.require_v9_capture(capture, "golden", failures)
        self.assertEqual(failures, [])

        failures = []
        validator.require_v9_capture(replace(capture, version=8), "old", failures)
        self.assertEqual(failures, ["old: capture is not v9"])


if __name__ == "__main__":
    unittest.main()
