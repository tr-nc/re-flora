from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github" / "workflows" / "shader-validation.yml"

REQUIRED_OWNER_PATHS = (
    "src/app/core/environment_irradiance_capture.rs",
    "src/app/core/mod.rs",
    "src/ddgi/**",
    "src/ddgi/resources.rs",
    "src/environment_lighting.rs",
    "src/tracer/**",
    "scripts/validate_ddgi_radiance_lifecycle.py",
    "scripts/summarize_ddgi_convergence.py",
)
REQUIRED_FEDORA_COMMANDS = (
    "cargo test --locked capture_metadata_uses_authoritative_published_terminal_identity",
    "cargo test --locked ddgi::resources::tests::filter_",
    "python3 -m unittest scripts.tests.test_analyze_environment_irradiance_capture.AnalyzeEnvironmentIrradianceCaptureTests.test_rust_producer_v10_golden_decodes_with_exact_filter_witness",
)


def between(source: str, start: str, end: str) -> str:
    return source.split(start, 1)[1].split(end, 1)[0]


def workflow_contract_failures(source: str) -> list[str]:
    failures: list[str] = []
    pull_request = between(source, "  pull_request:\n", "  push:\n")
    push = between(source, "  push:\n", "\npermissions:\n")
    fedora = between(source, "  fedora:\n", "  python-policy-tests:\n")

    for route_name, route in (("pull_request", pull_request), ("push", push)):
        for owner_path in REQUIRED_OWNER_PATHS:
            if f'      - "{owner_path}"' not in route:
                failures.append(f"{route_name} does not route {owner_path}")

    for command in REQUIRED_FEDORA_COMMANDS:
        if command not in fedora:
            failures.append(f"Fedora job does not run {command}")
    return failures


class ShaderValidationWorkflowTests(unittest.TestCase):
    def test_owner_changes_route_to_executable_fedora_evidence_tests(self) -> None:
        self.assertEqual(
            workflow_contract_failures(WORKFLOW.read_text(encoding="utf-8")),
            [],
        )

    def test_contract_detects_removed_route_and_misplaced_fedora_command(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        missing_route = source.replace(
            '      - "src/ddgi/resources.rs"\n', "", 1
        )
        self.assertIn(
            "pull_request does not route src/ddgi/resources.rs",
            workflow_contract_failures(missing_route),
        )

        command = REQUIRED_FEDORA_COMMANDS[-1]
        misplaced_command = source.replace(command, "true", 1)
        self.assertIn(
            f"Fedora job does not run {command}",
            workflow_contract_failures(misplaced_command),
        )


if __name__ == "__main__":
    unittest.main()
