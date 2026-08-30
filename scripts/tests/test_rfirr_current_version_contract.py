from __future__ import annotations

import re
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
PRODUCTION_RUNNERS = tuple(sorted(SCRIPTS.glob("check_ddgi*.sh")))


def invalid_expected_versions(source: str) -> list[str]:
    logical_source = source.replace("\\\r\n", " ").replace("\\\n", " ")
    uncommented: list[str] = []
    quote: str | None = None
    escaped = False
    index = 0
    while index < len(logical_source):
        character = logical_source[index]
        if escaped:
            uncommented.append(character)
            escaped = False
        elif character == "\\" and quote != "'":
            uncommented.append(character)
            escaped = True
        elif quote is not None:
            uncommented.append(character)
            if character == quote:
                quote = None
        elif character in {"'", '"'}:
            uncommented.append(character)
            quote = character
        elif character == "#":
            while index < len(logical_source) and logical_source[index] != "\n":
                index += 1
            uncommented.append("\n")
        else:
            uncommented.append(character)
        index += 1

    invalid: list[str] = []
    expectation = re.compile(
        r"--expect-version(?:=([^\s;&|]*)|[ \t]+([^\s;&|]+))?"
    )
    for match in expectation.finditer("".join(uncommented)):
        value = match.group(1) if match.group(1) is not None else match.group(2)
        normalized = value.strip("'\"") if value is not None else None
        if normalized != "current":
            invalid.append(f"--expect-version {normalized or '<missing>'}")
    return invalid


class RfirrCurrentVersionContractTests(unittest.TestCase):
    def test_production_runners_delegate_current_version_to_the_analyzer(self) -> None:
        for runner in PRODUCTION_RUNNERS:
            source = runner.read_text(encoding="utf-8")
            with self.subTest(runner=runner.name):
                self.assertEqual(invalid_expected_versions(source), [])

    def test_version_guard_rejects_non_current_shell_forms(self) -> None:
        production_source = (
            SCRIPTS / "check_ddgi_correctness.sh"
        ).read_text(encoding="utf-8")
        mutations = (
            "--expect-version 9",
            "--expect-version=9",
            "--expect-version \\" "\n  9",
            "--expect-version=\n9",
            "--expect-version # value removed\ncurrent",
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                mutated_source = production_source.replace(
                    "--expect-version current", mutation, 1
                )
                self.assertNotEqual(
                    invalid_expected_versions(mutated_source), []
                )

    def test_version_guard_accepts_current_and_ignores_comments(self) -> None:
        source = (
            "analyze --expect-version current\n"
            "analyze --expect-version=current\n"
            "analyze --expect-version \\" "\n  current\n"
            "# analyze --expect-version 9\n"
        )
        self.assertEqual(invalid_expected_versions(source), [])

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
