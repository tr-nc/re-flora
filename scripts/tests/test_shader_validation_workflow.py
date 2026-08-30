from __future__ import annotations

import ast
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/shader-validation.yml"
OWNER_PATHS = (
    ".github/workflows/shader-validation.yml",
    "Cargo.lock",
    "Cargo.toml",
    "config/ddgi_convergence_acceptance.toml",
    "docs/ddgi_convergence_calibration.md",
    "docs/ddgi_indirect_transport_spec.md",
    "docs/ddgi_migration_plan.md",
    "docs/ddgi_transport_acceptance.md",
    "docs/global_illumination_approaches_research.md",
    "scripts/check_ddgi_local_terrain_convergence.sh",
    "scripts/check_ddgi_transport_acceptance.sh",
    "scripts/check_latest_run_log.py",
    "scripts/perf_suite.py",
    "scripts/runtime_log_diagnostics.py",
    "scripts/summarize_ddgi_convergence.py",
    "scripts/tests/test_check_ddgi_transport_acceptance.py",
    "scripts/tests/test_ddgi_convergence_capsule_source.py",
    "scripts/tests/test_ddgi_capture_process_integration.py",
    "scripts/tests/test_ddgi_indirect_transport_spec.py",
    "scripts/tests/test_summarize_ddgi_convergence.py",
    "scripts/tests/test_shader_validation_workflow.py",
    "scripts/tests/test_validate_capture_process_evidence.py",
    "scripts/validate_capture_process_evidence.py",
    "src/ddgi/mod.rs",
    "src/ddgi/resources.rs",
    "src/ddgi/runtime.rs",
    "src/tracer/mod.rs",
)


class ShaderValidationWorkflowTests(unittest.TestCase):
    @staticmethod
    def yaml_sequence(workflow: str, path: tuple[str, ...]) -> list[str]:
        lines = workflow.splitlines()
        start = 0
        end = len(lines)
        for depth, key in enumerate(path):
            indent = depth * 2
            header = " " * indent + key + ":"
            match = next(
                index
                for index in range(start, end)
                if lines[index].split("#", 1)[0].rstrip() == header
            )
            start = match + 1
            end = next(
                (
                    index
                    for index in range(start, end)
                    if (content := lines[index].split("#", 1)[0].rstrip())
                    and len(content) - len(content.lstrip()) <= indent
                ),
                end,
            )
        item_indent = " " * (len(path) * 2)
        values = []
        for line in lines[start:end]:
            content = line.split("#", 1)[0].rstrip()
            if content.startswith(item_indent + "- "):
                value = content[len(item_indent) + 2 :]
                if value.startswith(('"', "'")):
                    value = ast.literal_eval(value)
                values.append(value)
        return values

    @staticmethod
    def path_pattern(pattern: str) -> re.Pattern[str]:
        if any(character in pattern for character in "?[]{}\\") or re.search(
            r"(?:@|\+|\?|\*|!)\(", pattern
        ):
            raise ValueError(f"unsupported workflow path syntax: {pattern}")
        expression = []
        index = 0
        while index < len(pattern):
            if pattern.startswith("**", index):
                expression.append(".*")
                index += 2
            elif pattern[index] == "*":
                expression.append("[^/]*")
                index += 1
            else:
                expression.append(re.escape(pattern[index]))
                index += 1
        return re.compile("^" + "".join(expression) + "$")

    @classmethod
    def path_is_included(cls, patterns: list[str], path: str) -> bool:
        included = False
        for pattern in patterns:
            if pattern.startswith("!("):
                raise ValueError(f"unsupported workflow path syntax: {pattern}")
            excluded = pattern.startswith("!")
            candidate = pattern[1:] if excluded else pattern
            if not candidate:
                raise ValueError("empty workflow path pattern")
            if cls.path_pattern(candidate).fullmatch(path):
                included = not excluded
        return included

    @staticmethod
    def append_event_path(workflow: str, event: str, pattern: str, quote: str) -> str:
        event_start = workflow.index(f"  {event}:\n")
        paths_start = workflow.index("    paths:\n", event_start)
        next_boundary = re.search(
            r"(?m)^(?:  [A-Za-z_][^:]*:|[^\s#])", workflow[paths_start + 1 :]
        )
        if next_boundary is None:
            raise AssertionError(f"{event} paths have no enclosing block boundary")
        end = paths_start + 1 + next_boundary.start()
        return workflow[:end] + f"      - {quote}{pattern}{quote}\n" + workflow[end:]

    def test_policy_inputs_and_targeted_rust_gates_are_continuous(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")

        self.assertEqual(workflow.count('      - "src/environment_lighting.rs"'), 2)
        self.assertEqual(workflow.count('      - "src/ddgi/**"'), 2)
        for path in (
            "config/camera_snapshots.toml",
            "config/ddgi_convergence_acceptance.toml",
            "docs/ddgi_convergence_calibration.md",
            "docs/ddgi_indirect_transport_spec.md",
            "docs/ddgi_transport_acceptance.md",
            "scripts/analyze_bd2_blue_voxel.py",
            "scripts/check_bd2_blue_voxel.sh",
            "scripts/check_ddgi_sky_normalization_evidence.py",
            "scripts/check_latest_run_log.py",
            "scripts/perf_suite.py",
            "scripts/runtime_log_diagnostics.py",
            "scripts/summarize_ddgi_convergence.py",
            "scripts/validate_capture_process_evidence.py",
            "src/app/core/mod.rs",
            "src/app/core/environment_lighting_test_scene.rs",
            "src/app/core/environment_irradiance_capture.rs",
            "src/app/core/loading.rs",
            "src/app/core/visible_terrain.rs",
            "src/main.rs",
            "src/run_log.rs",
            "src/tracer/mod.rs",
        ):
            self.assertEqual(workflow.count(f'      - "{path}"'), 2, path)
        self.assertIn('      - "shader/**"', workflow)
        self.assertIn('      - "scripts/check_ddgi*.sh"', workflow)
        self.assertIn('      - "scripts/lib/**"', workflow)
        self.assertIn('      - "scripts/tests/**"', workflow)
        self.assertIn("timeout 10m cargo test --locked environment_lighting::tests::", workflow)
        self.assertIn(
            "timeout 10m cargo test --locked environment_lighting_test_scene::tests:: --",
            workflow,
        )
        self.assertIn(
            "--skip app::core::environment_lighting_test_scene::tests::"
            "patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof",
            workflow,
        )
        self.assertIn("timeout 10m cargo test --locked startup_log_tests::", workflow)
        for gate in (
            "ddgi_convergence_evidence_tests::",
            "runtime_convergence_budget_matches_the_acceptance_contract",
            "private_evidence_lines_preserve_exact_count_content_and_order",
            "validation_wire_labels_bind_to_their_distinct_runtime_facts",
        ):
            self.assertIn(f"timeout 10m cargo test --locked {gate}", workflow)
        for gate in (
            "scripts.tests.test_validate_capture_process_evidence",
            "scripts.tests.test_summarize_ddgi_convergence",
            "scripts.tests.test_check_ddgi_transport_acceptance",
            "scripts.tests.test_ddgi_capture_process_integration",
            "scripts.tests.test_ddgi_indirect_transport_spec",
            "scripts.tests.test_runtime_log_diagnostics",
            "scripts.tests.test_check_latest_run_log",
            "scripts.tests.test_perf_suite",
        ):
            self.assertIn(gate, workflow)
        self.assertIn("timeout-minutes: 45", workflow)
        self.assertIn("timeout-minutes: 10", workflow)

    def test_all_convergence_owners_route_through_pull_request_and_push(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        for event in ("pull_request", "push"):
            patterns = self.yaml_sequence(workflow, ("on", event, "paths"))
            future_rust_owner = "src/app/reviewer_fixture.rs"
            self.assertTrue(
                self.path_is_included(patterns, future_rust_owner),
                f"{event} paths do not include future Rust owners through src/**",
            )
            excluded_rust = self.append_event_path(workflow, event, "!src/**", '"')
            self.assertFalse(
                self.path_is_included(
                    self.yaml_sequence(excluded_rust, ("on", event, "paths")),
                    future_rust_owner,
                ),
                f"{event} ignored a trailing all-Rust exclusion",
            )
            for owner in OWNER_PATHS:
                self.assertTrue(
                    self.path_is_included(patterns, owner),
                    f"{event} paths do not finally include {owner}",
                )

                for quote in ('"', "'"):
                    excluded = self.append_event_path(workflow, event, f"!{owner}", quote)
                    self.assertFalse(
                        self.path_is_included(
                            self.yaml_sequence(excluded, ("on", event, "paths")),
                            owner,
                        ),
                        f"{event} ignored a trailing {quote}-quoted exclusion for {owner}",
                    )

    def test_workflow_globs_are_slash_aware_and_unsupported_syntax_fails_closed(self) -> None:
        self.assertTrue(self.path_is_included(["src/ddgi/**"], "src/ddgi/runtime.rs"))
        self.assertFalse(self.path_is_included(["src/*"], "src/ddgi/runtime.rs"))
        self.assertTrue(self.path_is_included(["src/**"], "src/ddgi/runtime.rs"))
        unsupported = (
            "src/[dt]*",
            "src/ddgi/?.rs",
            "src/@(ddgi|tracer)/**",
            "src/+(ddgi)/**",
            "src/?(ddgi)/**",
            "src/*(ddgi)/**",
            "!(src/ddgi/**)",
        )
        for pattern in unsupported:
            with self.subTest(pattern=pattern), self.assertRaises(ValueError):
                self.path_is_included([pattern], "src/ddgi/runtime.rs")


if __name__ == "__main__":
    unittest.main()
