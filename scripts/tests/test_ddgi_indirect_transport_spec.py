from __future__ import annotations

import unittest
from pathlib import Path


SPEC = Path(__file__).resolve().parents[2] / "docs" / "ddgi_indirect_transport_spec.md"


class DdgiIndirectTransportSpecTests(unittest.TestCase):
    def test_normative_backstop_is_128_epochs_and_e63_is_only_historical(self) -> None:
        text = SPEC.read_text(encoding="utf-8")
        self.assertIn(
            "Epoch 127 is the hard finite backstop (128 complete temporal samples)", text
        )
        self.assertNotIn(
            "Epoch 63 is a hard finite backstop (64 complete temporal samples)", text
        )
        self.assertIn("Under the historical 64-epoch policy", text)
        self.assertIn("not the current\nsample-budget contract", text)


if __name__ == "__main__":
    unittest.main()
