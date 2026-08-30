import ast
from pathlib import Path
import re
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
SOURCE_OWNERS = (
    "src/app/mod.rs",
    "src/app/core/mod.rs",
    "src/app/core/lighting_mode_acceptance.rs",
    "src/tracer/mod.rs",
    "src/tracer/buffer_updater.rs",
    "src/environment_lighting.rs",
)
CONCRETE_ROUTES = tuple(path for path in ROUTES if path != "src/**") + SOURCE_OWNERS


def mapping_key(stripped: str) -> str | None:
    if not stripped.endswith(":"):
        return None
    raw = stripped[:-1].strip()
    if not raw:
        return None
    if raw[:1] in ('"', "'"):
        try:
            value = ast.literal_eval(raw)
        except (SyntaxError, ValueError):
            return None
        return value if isinstance(value, str) else None
    return raw if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_-]*", raw) else None


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
        key = mapping_key(stripped)
        if indent == 0:
            in_on = key == "on"
            in_event = False
            in_paths = False
            continue
        if not in_on:
            continue
        if indent == 2 and key is not None:
            in_event = key == event
            in_paths = False
            continue
        if not in_event:
            continue
        if indent == 4 and key is not None:
            in_paths = key == "paths"
            continue
        if in_paths and indent == 6 and stripped.startswith("- "):
            value = stripped[2:].strip()
            paths.append(ast.literal_eval(value) if value[:1] in ('"', "'") else value)
        elif in_paths and indent <= 4:
            in_paths = False
    return paths


def github_path_matches(path: str, pattern: str) -> bool:
    if any(token in pattern for token in ("?", "[", "]")) or re.search(
        r"[!+*@?]\(", pattern
    ):
        return False
    regex = ""
    index = 0
    while index < len(pattern):
        if pattern.startswith("**", index):
            index += 2
            if index < len(pattern) and pattern[index] == "/":
                regex += "(?:.*/)?"
                index += 1
            else:
                regex += ".*"
        elif pattern[index] == "*":
            regex += "[^/]*"
            index += 1
        else:
            regex += re.escape(pattern[index])
            index += 1
    return re.fullmatch(regex, path) is not None


def required_paths_are_routed(paths: list[str], required_paths: tuple[str, ...]) -> bool:
    normalized: list[tuple[bool, str]] = []
    for ordered_pattern in paths:
        excluded = ordered_pattern.startswith("!")
        pattern = ordered_pattern[1:] if excluded else ordered_pattern
        if any(token in pattern for token in ("?", "[", "]")) or re.search(
            r"[!+*@?]\(", pattern
        ):
            return False
        normalized.append((excluded, pattern))
    for required_path in required_paths:
        included = False
        for excluded, pattern in normalized:
            if github_path_matches(required_path, pattern):
                included = not excluded
        if not included:
            return False
    return True


def source_owners_are_routed(paths: list[str]) -> bool:
    return "src/**" in paths and required_paths_are_routed(paths, SOURCE_OWNERS)


class LightingModeAcceptanceCiTests(unittest.TestCase):
    def test_pull_request_routes_all_e2_sources_to_cpu_contract_gates(self) -> None:
        workflow = WORKFLOW.read_text()
        pull_request = event_paths(workflow, "pull_request")
        for path in ROUTES:
            self.assertIn(path, pull_request, path)
        self.assertTrue(required_paths_are_routed(pull_request, CONCRETE_ROUTES))

    def test_push_routes_all_e2_sources_to_cpu_contract_gates(self) -> None:
        workflow = WORKFLOW.read_text()
        push = event_paths(workflow, "push")
        for path in ROUTES:
            self.assertIn(path, push, path)
        self.assertTrue(required_paths_are_routed(push, CONCRETE_ROUTES))

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

    def test_later_src_exclusion_cannot_remove_e2_owners_from_either_event(self) -> None:
        workflow = WORKFLOW.read_text()
        for event in ("pull_request", "push"):
            with self.subTest(event=event):
                event_start = workflow.index(f"  {event}:")
                marker = '      - "src/**"'
                replacement = marker + '\n      - "!src/**"'
                mutated = workflow[:event_start] + workflow[event_start:].replace(
                    marker, replacement, 1
                )
                self.assertFalse(source_owners_are_routed(event_paths(mutated, event)))

    def test_later_app_module_exclusion_cannot_remove_the_capability_route(self) -> None:
        workflow = WORKFLOW.read_text()
        for event in ("pull_request", "push"):
            with self.subTest(event=event):
                event_start = workflow.index(f"  {event}:")
                marker = '      - "src/**"'
                replacement = marker + '\n      - "!src/app/mod.rs"'
                mutated = workflow[:event_start] + workflow[event_start:].replace(
                    marker, replacement, 1
                )
                paths = event_paths(mutated, event)
                self.assertFalse(source_owners_are_routed(paths))
                self.assertFalse(required_paths_are_routed(paths, CONCRETE_ROUTES))

    def test_later_environment_owner_exclusion_cannot_remove_the_gpu_sink_route(self) -> None:
        workflow = WORKFLOW.read_text()
        for event in ("pull_request", "push"):
            with self.subTest(event=event):
                event_start = workflow.index(f"  {event}:")
                marker = '      - "src/**"'
                replacement = marker + '\n      - "!src/environment_lighting.rs"'
                mutated = workflow[:event_start] + workflow[event_start:].replace(
                    marker, replacement, 1
                )
                paths = event_paths(mutated, event)
                self.assertFalse(source_owners_are_routed(paths))
                self.assertFalse(required_paths_are_routed(paths, CONCRETE_ROUTES))

    def test_later_script_or_doc_exclusion_cannot_remove_e2_routes(self) -> None:
        workflow = WORKFLOW.read_text()
        for event in ("pull_request", "push"):
            for exclusion in ("!scripts/**", "!docs/**"):
                with self.subTest(event=event, exclusion=exclusion):
                    event_start = workflow.index(f"  {event}:")
                    marker = '      - "src/**"'
                    replacement = marker + f'\n      - "{exclusion}"'
                    mutated = workflow[:event_start] + workflow[event_start:].replace(
                        marker, replacement, 1
                    )
                    self.assertFalse(
                        required_paths_are_routed(
                            event_paths(mutated, event), CONCRETE_ROUTES
                        )
                    )

    def test_single_star_cannot_reinclude_nested_owners_after_recursive_exclusion(self) -> None:
        paths = ["src/**", "!src/**", "src/*"]
        for event in ("pull_request", "push"):
            with self.subTest(event=event):
                self.assertFalse(source_owners_are_routed(paths))
                self.assertFalse(required_paths_are_routed(paths, CONCRETE_ROUTES))
        self.assertFalse(github_path_matches("src/app/core/mod.rs", "src/*"))
        self.assertFalse(github_path_matches("src/tracer/mod.rs", "src/*"))

    def test_unsupported_github_glob_syntax_fails_closed(self) -> None:
        for pattern in ("src/?", "src/[ab]/**", "src/@(app|tracer)/**"):
            with self.subTest(pattern=pattern):
                self.assertFalse(github_path_matches("src/app/core/mod.rs", pattern))
                self.assertFalse(source_owners_are_routed(["src/**", pattern]))

    def test_quoted_and_unquoted_event_keys_parse_the_same_paths(self) -> None:
        workflow = WORKFLOW.read_text()
        expected = {
            event: event_paths(workflow, event) for event in ("pull_request", "push")
        }
        variants = (
            workflow.replace("on:", '"on":', 1)
            .replace("  pull_request:", '  "pull_request":', 1)
            .replace("    paths:", '    "paths":', 1),
            workflow.replace("on:", "'on':", 1)
            .replace("  push:", "  'push':", 1)
            .replace("    paths:", "    'paths':", 2),
        )
        for variant in variants:
            for event, paths in expected.items():
                with self.subTest(event=event, prefix=variant[:8]):
                    self.assertEqual(event_paths(variant, event), paths)

    def test_malformed_mapping_keys_fail_closed(self) -> None:
        workflow = WORKFLOW.read_text().replace("on:", '"on:', 1)
        self.assertEqual(event_paths(workflow, "pull_request"), [])
        self.assertEqual(event_paths(workflow, "push"), [])

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
