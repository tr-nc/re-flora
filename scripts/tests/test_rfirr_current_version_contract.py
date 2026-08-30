from __future__ import annotations

import re
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
PRODUCTION_RUNNERS = tuple(sorted(SCRIPTS.glob("check_ddgi*.sh")))


class RfirrCurrentVersionContractTests(unittest.TestCase):
    def test_production_runners_delegate_current_version_to_the_analyzer(self) -> None:
        numeric_expectation = re.compile(r"--expect-version\s+\d+")
        current_callers = 0
        for runner in PRODUCTION_RUNNERS:
            source = runner.read_text(encoding="utf-8")
            with self.subTest(runner=runner.name):
                self.assertIsNone(numeric_expectation.search(source))
            current_callers += source.count("--expect-version current")

        self.assertEqual(current_callers, 13)

    def test_python_consumers_do_not_own_a_numeric_current_version(self) -> None:
        numeric_comparison = re.compile(r"\.version\s*(?:==|!=)\s*\d+")
        for consumer_name in (
            "validate_ddgi_radiance_lifecycle.py",
            "summarize_ddgi_convergence.py",
        ):
            source = (SCRIPTS / consumer_name).read_text(encoding="utf-8")
            with self.subTest(consumer=consumer_name):
                self.assertIsNone(numeric_comparison.search(source))


if __name__ == "__main__":
    unittest.main()
