from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
FIXTURES = Path(__file__).with_name("fixtures")
CURRENT_ANALYZER = SCRIPTS / "analyze_current_environment_irradiance_capture.py"
COMPATIBILITY_ANALYZER = SCRIPTS / "analyze_environment_irradiance_capture.py"
PRODUCTION_RUNNERS = tuple(sorted(SCRIPTS.glob("check_ddgi*.sh")))
sys.path.insert(0, str(SCRIPTS))

from rfirr_production_runner_contract import (  # noqa: E402
    RUNNER_INVOCATION_INVENTORY,
    RUNNER_PRODUCTION_DEPENDENCIES,
    production_evidence_dependencies,
    production_runner_invocation_failures,
)
from shader_validation_workflow_contract import REQUIRED_OWNER_PATHS  # noqa: E402


class RfirrCurrentVersionContractTests(unittest.TestCase):
    def run_analyzer(
        self, analyzer: Path, capture: Path, *arguments: str
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(analyzer), str(capture), *arguments],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_production_runners_have_direct_current_schema_invocations(self) -> None:
        for runner in PRODUCTION_RUNNERS:
            source = runner.read_text(encoding="utf-8")
            with self.subTest(runner=runner.name):
                self.assertEqual(
                    production_runner_invocation_failures(runner.name, source), []
                )

        self.assertEqual(
            set(RUNNER_INVOCATION_INVENTORY),
            {runner.name for runner in PRODUCTION_RUNNERS},
        )

    def test_runner_policy_and_root_authorities_are_readonly(self) -> None:
        for runner in PRODUCTION_RUNNERS:
            source = runner.read_text(encoding="utf-8")
            with self.subTest(runner=runner.name):
                self.assertEqual(
                    source.count(
                        'readonly repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"'
                    ),
                    1,
                )
                self.assertEqual(source.count("readonly dry_run"), 1)

    def test_state_and_root_reassignment_or_unset_are_rejected(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutations = (
            source.replace("readonly dry_run", "dry_run=false", 1),
            source + "\ndry_run=false\n",
            source + "\nunset dry_run\n",
            source.replace("readonly repo_root=", "repo_root=", 1),
            source.replace(
                'execute_analysis() {',
                'saved_repo_root="$repo_root"\n'
                'repo_root=/tmp/reviewer\n'
                'repo_root="$saved_repo_root"\n\n'
                'execute_analysis() {',
                1,
            ),
            source + "\nunset repo_root\n",
        )
        for mutated in mutations:
            with self.subTest():
                failures = production_runner_invocation_failures(
                    runner_name, mutated
                )
                self.assertTrue(
                    any("immutable runner authority" in failure for failure in failures),
                    failures,
                )

    def test_literal_authority_names_do_not_create_shell_authority(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        for mutated in (
            source + "\nprintf '%s' dry_run\n",
            source + "\n# dry_run and repo_root are immutable\n",
            source + "\n# ${dry_run:-false} and $repo_root are literals\n",
            source + "\nprintf '%s' '${dry_run:-false} $repo_root'\n",
            source + "\nprintf '%s' \\$dry_run \\$repo_root\n",
            source + '\nprintf \'%s\' "dry_run and repo_root are immutable"\n',
        ):
            with self.subTest():
                self.assertEqual(
                    production_runner_invocation_failures(runner_name, mutated),
                    [],
                )

    def test_canonical_parameter_expansion_guard_is_accepted(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutated = source.replace(
            "    if $dry_run; then\n"
            '        print_command "${command[@]}"',
            "    if ${dry_run:-false}; then\n"
            '        print_command "${command[@]}"',
            1,
        )
        self.assertEqual(
            production_runner_invocation_failures(runner_name, mutated), []
        )

    def test_parameter_expansion_base_identifier_is_exact(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        anchor = '        if ! analyze_current_capture "${final_analysis[@]}"; then'
        call = 'analyze_current_capture "${final_analysis[@]}"'
        for expansion in (
            "${dry_run:-false}",
            "${#dry_run}",
            "${!dry_run}",
            "${dry_run[0]:-false}",
        ):
            with self.subTest(expansion=expansion):
                mutated = source.replace(
                    anchor,
                    f"        if {expansion}; then\n"
                    f"            {call}\n"
                    "        elif false; then",
                    1,
                )
                failures = production_runner_invocation_failures(
                    runner_name, mutated
                )
                self.assertTrue(
                    any("controlled by dry_run" in failure for failure in failures),
                    failures,
                )

        dry_runner = source.replace(
            anchor,
            "        if ${dry_runner:-false}; then\n"
            f"            {call}\n"
            "        elif false; then",
            1,
        )
        failures = production_runner_invocation_failures(runner_name, dry_runner)
        self.assertFalse(
            any("controlled by dry_run" in failure for failure in failures),
            failures,
        )

    def test_comment_quotes_cannot_mask_a_dry_run_controlled_analysis_call(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        anchor = '        if ! analyze_current_capture "${final_analysis[@]}"; then'
        for apostrophe_line in (
            "        # reviewer single quote: '",
            '        printf \'%s\\n\' "reviewer\'s quoted prose"',
        ):
            with self.subTest(apostrophe_line=apostrophe_line):
                mutated = source.replace(
                    anchor,
                    f"{apostrophe_line}\n"
                    "        if $dry_run; then\n"
                    '            analyze_current_capture "${final_analysis[@]}"\n'
                    "        elif false; then\n"
                    f"{apostrophe_line}\n"
                    "        fi",
                    1,
                )

                failures = production_runner_invocation_failures(
                    runner_name, mutated
                )

                self.assertTrue(
                    any(
                        "controlled by dry_run" in failure
                        for failure in failures
                    ),
                    failures,
                )

    def test_comments_assignments_and_unused_functions_are_not_invocations(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        direct_call = (
            '    "$repo_root/scripts/'
            'analyze_current_environment_irradiance_capture.py" "$@"'
        )
        mutations = (
            source.replace(direct_call, f"    # {direct_call.strip()}", 1),
            source.replace(
                direct_call,
                '    unused_analyzer="$repo_root/scripts/'
                'analyze_current_environment_irradiance_capture.py"',
                1,
            ),
            source.replace('analyze_current_capture "', 'true "'),
        )
        for mutated in mutations:
            with self.subTest():
                self.assertNotEqual(
                    production_runner_invocation_failures(runner_name, mutated), []
                )

    def test_every_inventory_rejects_deleted_or_replaced_analysis_branches(self) -> None:
        for runner in PRODUCTION_RUNNERS:
            source = runner.read_text(encoding="utf-8")
            invocation = next(
                line
                for line in source.splitlines()
                if "analyze_current_capture" in line
                and "()" not in line
                and "analyze_current_environment" not in line
            )
            for replacement in (f"# {invocation}", invocation.replace("analyze_current_capture", "true", 1)):
                with self.subTest(runner=runner.name, replacement=replacement.strip()):
                    mutated = source.replace(invocation, replacement, 1)
                    self.assertNotEqual(
                        production_runner_invocation_failures(runner.name, mutated), []
                    )

    def test_unused_helper_call_cannot_replace_a_real_inventory_branch(self) -> None:
        runner_name = "check_ddgi_runtime_terrain_edits.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        real_call = "        analyze_current_capture"
        mutated = source.replace(real_call, "        true", 1)
        mutated += "\nunused_analysis() {\n    analyze_current_capture \"$@\"\n}\n"

        self.assertNotEqual(
            production_runner_invocation_failures(runner_name, mutated), []
        )

    def test_alternate_or_late_function_overrides_are_rejected(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutations = (
            source + "\nanalyze_current_capture(){ true; }\n",
            source + "\nanalyze_current_capture () { true; }\n",
            source + "\nfunction analyze_current_capture { true; }\n",
            source + "\nfunction analyze_current_capture() { true; }\n",
            source + "\nalias analyze_current_capture=true\n",
            source + "\nanalyze_current_capture=true\n",
        )
        for mutated in mutations:
            with self.subTest():
                self.assertNotEqual(
                    production_runner_invocation_failures(runner_name, mutated), []
                )

    def test_canonical_function_accepts_equivalent_bash_definition_forms(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        for header in (
            "analyze_current_capture(){",
            "analyze_current_capture () {",
            "function analyze_current_capture {",
            "function analyze_current_capture() {",
        ):
            with self.subTest(header=header):
                mutated = source.replace("analyze_current_capture() {", header, 1)
                mutated = mutated.replace(
                    '    "$repo_root/scripts/analyze_current_environment_irradiance_capture.py" "$@"',
                    '  "$repo_root/scripts/analyze_current_environment_irradiance_capture.py"   "$@"',
                    1,
                )
                self.assertEqual(
                    production_runner_invocation_failures(runner_name, mutated), []
                )

    def test_dependency_inventory_accepts_braced_repo_root_literals(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutated = source.replace("$repo_root/scripts/", "${repo_root}/scripts/")
        self.assertEqual(
            production_runner_invocation_failures(runner_name, mutated), []
        )

    def test_scope_inventory_accepts_equivalent_helper_definition_forms(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        for function_name in ("execute_analysis", "run_analysis"):
            canonical = f"{function_name}() {{"
            for header in (
                f"{function_name}(){{",
                f"{function_name} () {{",
                f"function {function_name} {{",
                f"function {function_name}() {{",
            ):
                with self.subTest(function=function_name, header=header):
                    mutated = source.replace(canonical, header, 1)
                    self.assertEqual(
                        production_runner_invocation_failures(
                            runner_name, mutated
                        ),
                        [],
                    )

    def test_hidden_or_unknown_analyzer_occurrences_are_rejected(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        anchor = '        if ! analyze_current_capture "${final_analysis[@]}"; then'
        hidden = source.replace(
            anchor,
            '        if $dry_run; then\n'
            '            analyze_current_capture>/dev/null 2>&1\n'
            f'        fi\n{anchor}',
            1,
        )
        hidden_failures = production_runner_invocation_failures(
            runner_name, hidden
        )
        self.assertTrue(
            any("controlled by dry_run" in failure for failure in hidden_failures),
            hidden_failures,
        )
        unknown_mutations = (
            source + "\n# analyze_current_capture hidden occurrence\n",
            source + '\nunused="analyze_current_capture"\n',
        )
        for mutated in unknown_mutations:
            with self.subTest():
                failures = production_runner_invocation_failures(
                    runner_name, mutated
                )
                self.assertTrue(
                    any("unclassified analyzer occurrence" in failure for failure in failures),
                    failures,
                )

    def test_transport_analysis_pipeline_has_exactly_one_sink(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutations = (
            source.replace("    local sink=(cat)", '    local sink=(tee "$json")', 1),
            source.replace(
                '        sink=(/usr/bin/env tee "$json")', "        sink=(cat)", 1
            ),
            source.replace(
                '        sink=(/usr/bin/env tee "$json")',
                '        sink=(/usr/bin/env tee "$capture")',
                1,
            ),
            source.replace(
                '    analyze_current_capture "$@" | "${sink[@]}"',
                '    analyze_current_capture "$@" | tee /dev/null | "${sink[@]}"',
                1,
            ),
        )
        for mutated in mutations:
            with self.subTest():
                failures = production_runner_invocation_failures(runner_name, mutated)
                self.assertTrue(
                    any("exact analyzer-to-sink policy" in failure for failure in failures),
                    failures,
                )

    def test_transport_sink_policy_accepts_equivalent_whitespace(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutated = source.replace("    local sink=(cat)", "  local   sink=( cat )", 1)
        mutated = mutated.replace(
            '        sink=(/usr/bin/env tee "$json")',
            '      sink=( /usr/bin/env   tee   "$json" )',
            1,
        )
        mutated = mutated.replace(
            '    analyze_current_capture "$@" | "${sink[@]}"',
            '  analyze_current_capture   "$@"   |   "${sink[@]}"',
            1,
        )
        self.assertEqual(
            production_runner_invocation_failures(runner_name, mutated), []
        )

    def test_cargo_command_array_launch_must_keep_its_non_dry_policy(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutated = source.replace(
            '            if $dry_run; then\n'
            '                print_command "${command[@]}" --ddgi-debug-view "$view" \\\n'
            '                    --environment-irradiance-capture "$path"\n'
            '                continue',
            '            if $dry_run; then\n'
            '                print_command "${command[@]}" --ddgi-debug-view "$view" \\\n'
            '                    --environment-irradiance-capture "$path"\n'
            '                "${command[@]}" || true\n'
            '                continue',
            1,
        )
        failures = production_runner_invocation_failures(runner_name, mutated)
        self.assertTrue(
            any("unauthorized process launch" in failure for failure in failures),
            failures,
        )

    def test_process_launch_inventory_rejects_direct_app_commands(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutations = (
            source + "\nre-flora --version\n",
            source + '\n"$repo_root/target/release/re-flora" --version\n',
            source + '\n"/tmp/reviewer/re-flora" --version\n',
            source + '\n"/tmp/reviewer/cargo" --version\n',
            source + "\ncargo --version\n",
            source + "\ncommand_not_really cargo --version\n",
            source + "\ncargo() { true; }\n",
            source + "\nalias cargo=true\n",
            source + "\ncommand() { true; }\n",
            source
            + "\ncommand() {\n"
            + '    if [[ "$1" == tee ]]; then shift; cat; else "$@"; fi\n'
            + "}\n",
            source + "\ntee() { cat; }\n",
            source + "\npython3() { true; }\n",
            source + "\ntee /tmp/output\n",
        )
        for mutated in mutations:
            with self.subTest():
                failures = production_runner_invocation_failures(
                    runner_name, mutated
                )
                self.assertTrue(
                    any("unauthorized process launch" in failure for failure in failures),
                    failures,
                )

    def test_process_authority_names_in_comments_are_ignored(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutated = source + "\n# cargo and re-flora launch authority\n"
        self.assertEqual(
            production_runner_invocation_failures(runner_name, mutated), []
        )

    def test_process_names_in_shell_literals_are_not_launches(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        for literal in (
            ":;# cargo and re-flora are not commands",
            "printf '%s' 'cargo and re-flora are not commands'",
            'printf \'%s\' "cargo and re-flora are not commands"',
            "printf '%s\\n' '\ncargo and re-flora are still data\n'",
        ):
            with self.subTest(literal=literal):
                self.assertEqual(
                    production_runner_invocation_failures(
                        runner_name, source + "\n" + literal + "\n"
                    ),
                    [],
                )

    def test_control_statement_assignments_cannot_mutate_runner_authority(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        for assignment in (
            "if dry_run=false; then :; fi",
            "if repo_root=/tmp/reviewer; then :; fi",
            "if dry_run+=false; then :; fi",
            ": \"${dry_run:=false}\"",
            ": \"${repo_root:=/tmp/reviewer}\"",
        ):
            with self.subTest(assignment=assignment):
                mutated = source.replace(
                    "    dry_run=true",
                    "    dry_run=true\n    " + assignment,
                    1,
                )
                failures = production_runner_invocation_failures(
                    runner_name, mutated
                )
                self.assertTrue(
                    any(
                        "immutable runner authority" in failure
                        for failure in failures
                    ),
                    failures,
                )

    def test_comparisons_and_nonassigning_expansions_do_not_mutate_authority(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        for comparison in (
            "if [[ dry_run = false ]]; then :; fi",
            "if [[ $dry_run == false ]]; then :; fi",
            ': "${dry_run:-false}" "${repo_root}/target"',
        ):
            with self.subTest(comparison=comparison):
                mutated = source + "\n" + comparison + "\n"
                self.assertEqual(
                    production_runner_invocation_failures(runner_name, mutated),
                    [],
                )

    def test_dynamic_writer_reviewer_reproductions_cannot_mutate_authority(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        for mutation in (
            "eval 'dry_run=false'",
            "printf -v dry_run false",
            "read dry_run <<< false",
        ):
            with self.subTest(mutation=mutation):
                mutated = source.replace(
                    "    dry_run=true",
                    "    dry_run=true\n    " + mutation,
                    1,
                )
                failures = production_runner_invocation_failures(
                    runner_name, mutated
                )
                self.assertTrue(
                    any(
                        "immutable runner authority" in failure
                        for failure in failures
                    ),
                    failures,
                )

    def test_dynamic_authority_and_code_loading_constructs_fail_closed(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        for mutation in (
            "eval 'printf prose'",
            'source "$repo_root/scripts/helper.sh"',
            '. "$repo_root/scripts/helper.sh"',
            "printf -v repo_root /tmp/reviewer",
            'target=dry_run; printf -v "$target" false',
            "read -r repo_root <<< /tmp/reviewer",
            "read -a dry_run <<< false",
            "readarray -O 0 dry_run <<< false",
            "mapfile -t repo_root <<< /tmp/reviewer",
            "getopts ':x' dry_run",
            "let 'dry_run+=1'",
            "declare -rn ref=dry_run",
            "typeset -n ref=repo_root",
            "local -n ref=dry_run",
            "builtin eval 'dry_run=false'",
            "command source /tmp/reviewer.sh",
            "bash -c 'dry_run=false'",
            "/usr/bin/env bash -c 'dry_run=false'",
        ):
            with self.subTest(mutation=mutation):
                mutated = source.replace(
                    "    dry_run=true",
                    "    dry_run=true\n    " + mutation,
                    1,
                )
                failures = production_runner_invocation_failures(
                    runner_name, mutated
                )
                self.assertTrue(
                    any(
                        "immutable runner authority" in failure
                        for failure in failures
                    ),
                    failures,
                )

    def test_dynamic_construct_names_as_data_and_safe_targets_are_allowed(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        for statement in (
            "# eval 'dry_run=false'; source /tmp/reviewer.sh",
            "printf '%s' 'eval source . printf -v dry_run read repo_root'",
            'printf \'%s\' "eval and source are prose"',
            "message='eval dry_run=false; read repo_root'",
            "printf -v message prose",
            "read -r message <<< prose",
            "read -p dry_run message <<< prose",
            "read -u repo_root message <<< prose",
            "readarray messages <<< prose",
            "mapfile -C dry_run messages <<< prose",
            "getopts dry_run option_name",
            "let 'message=1' 'dry_run==false'",
        ):
            with self.subTest(statement=statement):
                self.assertEqual(
                    production_runner_invocation_failures(
                        runner_name, source + "\n" + statement + "\n"
                    ),
                    [],
                )

    def test_transport_normalization_python_is_env_owned(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        self.assertEqual(source.count("/usr/bin/env python3"), 2)
        mutated = source.replace("/usr/bin/env python3", "python3", 1)
        failures = production_runner_invocation_failures(runner_name, mutated)
        self.assertTrue(
            any("unauthorized process launch" in failure for failure in failures),
            failures,
        )

    def test_runner_cargo_and_decision_tee_have_no_bare_launches(self) -> None:
        for runner in PRODUCTION_RUNNERS:
            source = runner.read_text(encoding="utf-8")
            with self.subTest(runner=runner.name, tool="cargo"):
                mutated = source.replace("/usr/bin/env cargo", "cargo", 1)
                failures = production_runner_invocation_failures(
                    runner.name, mutated
                )
                self.assertTrue(
                    any("unauthorized process launch" in failure for failure in failures),
                    failures,
                )
            if "/usr/bin/env tee" in source:
                with self.subTest(runner=runner.name, tool="tee"):
                    mutated = source.replace("/usr/bin/env tee", "tee", 1)
                    failures = production_runner_invocation_failures(
                        runner.name, mutated
                    )
                    self.assertTrue(
                        any(
                            "unauthorized process launch" in failure
                            or "exact analyzer-to-sink policy" in failure
                            for failure in failures
                        ),
                        failures,
                    )

    def test_indented_function_closing_brace_ends_scope(self) -> None:
        runner_name = "check_ddgi_inflight_terrain_edits.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutated = source.replace(
            '    echo "[DDGI_INFLIGHT_EDIT] PASS spacing=$spacing repeat=$repeat active revision skipped $obsolete_revision and reached $replacement_revision"\n'
            "    fi\n"
            "}\n\n"
            'for spacing in "${spacings[@]}"; do',
            '    echo "[DDGI_INFLIGHT_EDIT] PASS spacing=$spacing repeat=$repeat active revision skipped $obsolete_revision and reached $replacement_revision"\n'
            "    fi\n"
            "    }\n\n"
            'for spacing in "${spacings[@]}"; do',
            1,
        )
        self.assertEqual(
            production_runner_invocation_failures(runner_name, mutated), []
        )

    def test_conditional_closing_fi_accepts_trailing_comment(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutated = source.replace(
            '    /usr/bin/env cargo build --release --manifest-path "$repo_root/Cargo.toml"\nfi',
            '    /usr/bin/env cargo build --release --manifest-path "$repo_root/Cargo.toml"\n'
            "fi # production-only build",
            1,
        )
        self.assertEqual(
            production_runner_invocation_failures(runner_name, mutated), []
        )

    def test_runner_dependency_inventory_closes_over_all_production_helpers(self) -> None:
        sources = {
            runner.name: runner.read_text(encoding="utf-8")
            for runner in PRODUCTION_RUNNERS
        }
        self.assertEqual(set(RUNNER_PRODUCTION_DEPENDENCIES), set(sources))
        dependencies = production_evidence_dependencies(sources)
        self.assertIn("scripts/check_ddgi_sky_normalization_evidence.py", dependencies)
        self.assertTrue(dependencies.issubset(REQUIRED_OWNER_PATHS))

        for runner_name, source in sources.items():
            with self.subTest(runner=runner_name):
                self.assertEqual(
                    production_runner_invocation_failures(runner_name, source), []
                )

    def test_current_schema_entry_accepts_current_and_rejects_v9(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            current = root / "current.rfirr"
            current.write_bytes(
                bytes.fromhex((FIXTURES / "ddgi_filter_evidence_v10.hex").read_text())
            )
            historical = root / "historical.rfirr"
            historical.write_bytes(
                bytes.fromhex((FIXTURES / "ddgi_filter_evidence_v9.hex").read_text())
            )

            current_result = self.run_analyzer(CURRENT_ANALYZER, current)
            historical_result = self.run_analyzer(CURRENT_ANALYZER, historical)

        self.assertEqual(current_result.returncode, 0, current_result.stderr)
        self.assertEqual(json.loads(current_result.stdout)["capture"]["version"], 10)
        self.assertEqual(historical_result.returncode, 1, historical_result.stderr)
        self.assertIn(
            "version: expected 10, got 9",
            json.loads(historical_result.stdout)["validation_failures"],
        )

    def test_current_schema_entry_has_no_numeric_or_dynamic_version_surface(self) -> None:
        fixture = bytes.fromhex(
            (FIXTURES / "ddgi_filter_evidence_v9.hex").read_text()
        )
        with tempfile.TemporaryDirectory() as directory:
            capture = Path(directory) / "historical.rfirr"
            capture.write_bytes(fixture)
            mutations = (
                ("--expect-version", "9"),
                ("--expect-version=9",),
                # Escaped and dynamically expanded shell spellings produce this argv.
                ("--expect-version", "current"),
            )
            for arguments in mutations:
                with self.subTest(arguments=arguments):
                    result = self.run_analyzer(CURRENT_ANALYZER, capture, *arguments)
                    self.assertEqual(result.returncode, 2)
                    self.assertIn("unrecognized arguments", result.stderr)

    def test_shell_escaping_and_expansion_cannot_select_a_schema(self) -> None:
        fixture = bytes.fromhex(
            (FIXTURES / "ddgi_filter_evidence_v9.hex").read_text()
        )
        with tempfile.TemporaryDirectory() as directory:
            capture = Path(directory) / "historical.rfirr"
            capture.write_bytes(fixture)
            commands = (
                '"$1" "$2" --expect\\-version 9',
                'flag=--expect-version; "$1" "$2" "$flag" 9',
            )
            for command in commands:
                with self.subTest(command=command):
                    result = subprocess.run(
                        [
                            "bash",
                            "-c",
                            command,
                            "production-current-schema-test",
                            str(CURRENT_ANALYZER),
                            str(capture),
                        ],
                        check=False,
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(result.returncode, 2)
                    self.assertIn("unrecognized arguments", result.stderr)

    def test_compatibility_entry_retains_explicit_numeric_decode(self) -> None:
        fixture = bytes.fromhex(
            (FIXTURES / "ddgi_filter_evidence_v9.hex").read_text()
        )
        with tempfile.TemporaryDirectory() as directory:
            capture = Path(directory) / "historical.rfirr"
            capture.write_bytes(fixture)
            result = self.run_analyzer(
                COMPATIBILITY_ANALYZER, capture, "--expect-version", "9"
            )

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_python_consumers_do_not_own_a_numeric_current_version(self) -> None:
        for consumer_name in (
            "validate_ddgi_radiance_lifecycle.py",
            "summarize_ddgi_convergence.py",
        ):
            source = (SCRIPTS / consumer_name).read_text(encoding="utf-8")
            with self.subTest(consumer=consumer_name):
                self.assertNotRegex(source, r"\.version\s*(?:==|!=)\s*\d+")


if __name__ == "__main__":
    unittest.main()
