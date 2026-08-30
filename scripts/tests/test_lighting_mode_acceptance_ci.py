import ast
from pathlib import Path
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = REPO_ROOT / ".github" / "workflows" / "shader-validation.yml"
ROUTES = (
    "scripts/analyze_lighting_mode_acceptance.py",
    "scripts/check_lighting_mode_acceptance.sh",
    "scripts/check_lighting_mode_acceptance_source_contract.py",
    "scripts/tests/test_analyze_lighting_mode_acceptance.py",
    "scripts/tests/test_check_lighting_mode_acceptance.py",
    "scripts/tests/test_lighting_mode_acceptance_ci.py",
    "scripts/tests/test_lighting_mode_acceptance_source_contract.py",
    "docs/lighting_mode_acceptance.md",
    "src/**",
)


def event_paths(workflow: str, event: str) -> list[str]:
    """Parse the top-level on.<event>.paths sequence used by this workflow."""
    lines = workflow.splitlines()
    in_on = False
    in_event = False
    in_paths = False
    paths: list[str] = []
    for line in lines:
        stripped = line.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        indent = len(line) - len(stripped)
        if indent == 0:
            in_on = stripped == "on:"
            in_event = False
            in_paths = False
            continue
        if not in_on:
            continue
        if indent == 2 and stripped.endswith(":"):
            in_event = stripped == f"{event}:"
            in_paths = False
            continue
        if not in_event:
            continue
        if indent == 4 and stripped.endswith(":"):
            in_paths = stripped == "paths:"
            continue
        if in_paths and indent == 6 and stripped.startswith("- "):
            value = stripped[2:].strip()
            paths.append(ast.literal_eval(value) if value[:1] in ('"', "'") else value)
        elif in_paths and indent <= 4:
            in_paths = False
    return paths


class LightingModeAcceptanceCiTests(unittest.TestCase):
    def test_pull_request_routes_all_e2_sources_to_cpu_contract_gates(self) -> None:
        workflow = WORKFLOW.read_text()
        pull_request = event_paths(workflow, "pull_request")
        for path in ROUTES:
            self.assertIn(path, pull_request, path)

    def test_push_routes_all_e2_sources_to_cpu_contract_gates(self) -> None:
        workflow = WORKFLOW.read_text()
        push = event_paths(workflow, "push")
        for path in ROUTES:
            self.assertIn(path, push, path)

    def test_commented_src_route_is_not_an_event_path_item(self) -> None:
        workflow = WORKFLOW.read_text()
        for event in ("pull_request", "push"):
            with self.subTest(event=event):
                marker = '      - "src/**"'
                event_start = workflow.index(f"  {event}:")
                mutated = workflow[:event_start] + workflow[event_start:].replace(
                    marker, '      # - "src/**"', 1
                )
                self.assertNotIn("src/**", event_paths(mutated, event))

    def test_shader_validation_runs_cpu_contract_gates(self) -> None:
        workflow = WORKFLOW.read_text()
        for redundant_rust_route in (
            "src/app/mod.rs",
            "src/app/core/lighting_mode_acceptance.rs",
            "src/app/core/mod.rs",
            "src/tracer/buffer_updater.rs",
            "src/tracer/mod.rs",
        ):
            self.assertNotIn(f'- "{redundant_rust_route}"', workflow)
        self.assertIn(
            "python3 -m unittest scripts.tests.test_analyze_lighting_mode_acceptance",
            workflow,
        )
        self.assertIn(
            "python3 -m unittest scripts.tests.test_check_lighting_mode_acceptance",
            workflow,
        )
        self.assertIn(
            "python3 -m unittest scripts.tests.test_lighting_mode_acceptance_source_contract",
            workflow,
        )
        self.assertIn("cargo test --locked lighting_mode_acceptance", workflow)
        self.assertIn(
            "cargo test --locked startup_log_tests::run_log_binding_marker_uses_the_existing_absolute_path",
            workflow,
        )


if __name__ == "__main__":
    unittest.main()
