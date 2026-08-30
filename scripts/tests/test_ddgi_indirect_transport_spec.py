from __future__ import annotations

import unittest
from pathlib import Path


SPEC = Path(__file__).resolve().parents[2] / "docs" / "ddgi_indirect_transport_spec.md"
ACCEPTANCE = Path(__file__).resolve().parents[2] / "docs" / "ddgi_transport_acceptance.md"
MIGRATION = Path(__file__).resolve().parents[2] / "docs" / "ddgi_migration_plan.md"


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

    def test_acceptance_e63_observations_are_explicitly_historical(self) -> None:
        lines = ACCEPTANCE.read_text(encoding="utf-8").splitlines()
        e63_lines = [index for index, line in enumerate(lines) if "e63" in line]

        self.assertGreater(len(e63_lines), 0)
        for index in e63_lines:
            context = " ".join(lines[max(0, index - 1) : index + 2]).lower()
            self.assertIn("historical", context, lines[index])

    def test_migration_e63_observations_are_explicitly_historical(self) -> None:
        lines = MIGRATION.read_text(encoding="utf-8").splitlines()
        e63_lines = [index for index, line in enumerate(lines) if "e63" in line]

        self.assertGreater(len(e63_lines), 0)
        for index in e63_lines:
            context = " ".join(lines[max(0, index - 1) : index + 2]).lower()
            self.assertIn("historical 64-epoch", context, lines[index])


if __name__ == "__main__":
    unittest.main()
