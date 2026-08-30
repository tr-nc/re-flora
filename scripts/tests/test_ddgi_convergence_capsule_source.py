from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class DdgiConvergenceCapsuleSourceTests(unittest.TestCase):
    def test_runtime_status_cannot_expose_a_second_validation_count_source(self) -> None:
        runtime = (ROOT / "src" / "ddgi" / "runtime.rs").read_text(encoding="utf-8")
        status = re.search(
            r"pub struct DdgiRuntimeVolumeStatus\s*\{(?P<body>.*?)\n\}",
            runtime,
            re.DOTALL,
        )

        self.assertIsNotNone(status)
        self.assertNotIn("consecutive_below_threshold", status.group("body"))

    def test_validated_publication_privately_owns_the_validation_count(self) -> None:
        runtime = (ROOT / "src" / "ddgi" / "runtime.rs").read_text(encoding="utf-8")
        capsule = re.search(
            r"pub\(crate\) struct DdgiValidatedPublication\s*\{(?P<body>.*?)\n\}",
            runtime,
            re.DOTALL,
        )

        self.assertIsNotNone(capsule)
        body = capsule.group("body")
        self.assertRegex(body, r"(?m)^\s*consecutive_below_threshold: u32,")
        self.assertNotRegex(
            body,
            r"(?m)^\s*pub(?:\(crate\)|\(super\))?\s+consecutive_below_threshold:",
        )


if __name__ == "__main__":
    unittest.main()
