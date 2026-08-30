"""Structural tripwire for the private DDGI convergence-evidence capability.

Rust unit tests inside the owning module prove payload formatting. This test only proves the
cross-module ownership and sequencing facts that Python can establish honestly.
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
RUNTIME = "src/ddgi/runtime.rs"
TRACER = "src/tracer/mod.rs"
MODULE = "convergence_evidence"
COMMIT = "commit_convergence_evidence"


@dataclass(frozen=True)
class RustToken:
    kind: str
    value: str


def rust_tokens(source: str) -> list[RustToken]:
    tokens: list[RustToken] = []
    index = 0
    while index < len(source):
        if source[index].isspace():
            index += 1
            continue
        if source.startswith("//", index):
            newline = source.find("\n", index + 2)
            index = len(source) if newline < 0 else newline + 1
            continue
        if source.startswith("/*", index):
            depth = 1
            index += 2
            while depth:
                if index >= len(source):
                    raise AssertionError("unterminated Rust block comment")
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            continue

        raw_string = False
        for prefix in ("br", "rb", "cr", "r"):
            if not source.startswith(prefix, index):
                continue
            cursor = index + len(prefix)
            hash_count = 0
            while cursor < len(source) and source[cursor] == "#":
                hash_count += 1
                cursor += 1
            if cursor >= len(source) or source[cursor] != '"':
                continue
            terminator = '"' + "#" * hash_count
            end = source.find(terminator, cursor + 1)
            if end < 0:
                raise AssertionError("unterminated Rust raw string")
            tokens.append(RustToken("STRING", source[cursor + 1 : end]))
            index = end + len(terminator)
            raw_string = True
            break
        if raw_string:
            continue

        string_prefix = next(
            (prefix for prefix in ("b", "c", "") if source.startswith(prefix + '"', index)),
            None,
        )
        if string_prefix is not None:
            cursor = index + len(string_prefix) + 1
            value: list[str] = []
            while cursor < len(source):
                if source[cursor] == "\\":
                    value.append(source[cursor : cursor + 2])
                    cursor += 2
                elif source[cursor] == '"':
                    break
                else:
                    value.append(source[cursor])
                    cursor += 1
            if cursor >= len(source):
                raise AssertionError("unterminated Rust string")
            tokens.append(RustToken("STRING", "".join(value)))
            index = cursor + 1
            continue

        char_prefix = "b" if source.startswith("b'", index) else ""
        if source.startswith(char_prefix + "'", index):
            cursor = index + len(char_prefix) + 1
            cursor += 2 if cursor < len(source) and source[cursor] == "\\" else 1
            if cursor < len(source) and source[cursor] == "'":
                tokens.append(RustToken("CHAR", ""))
                index = cursor + 1
                continue

        if source.startswith("r#", index) and index + 2 < len(source):
            cursor = index + 2
            if source[cursor].isalpha() or source[cursor] == "_":
                cursor += 1
                while cursor < len(source) and (
                    source[cursor].isalnum() or source[cursor] == "_"
                ):
                    cursor += 1
                tokens.append(RustToken("IDENT", source[index + 2 : cursor]))
                index = cursor
                continue
        if source[index].isalpha() or source[index] == "_":
            cursor = index + 1
            while cursor < len(source) and (
                source[cursor].isalnum() or source[cursor] == "_"
            ):
                cursor += 1
            tokens.append(RustToken("IDENT", source[index:cursor]))
            index = cursor
            continue

        tokens.append(RustToken("PUNCT", source[index]))
        index += 1
    return tokens


def matching(tokens: list[RustToken], opening: int, left: str, right: str) -> int:
    depth = 1
    for index in range(opening + 1, len(tokens)):
        if tokens[index].value == left:
            depth += 1
        elif tokens[index].value == right:
            depth -= 1
            if depth == 0:
                return index
    raise AssertionError(f"unmatched Rust {left}")


def production_tokens(source: str) -> list[RustToken]:
    tokens = rust_tokens(source)
    kept: list[RustToken] = []
    index = 0
    brace_depth = 0
    cfg_test_mod = ("#", "[", "cfg", "(", "test", ")", "]", "mod")
    while index < len(tokens):
        values = tuple(token.value for token in tokens[index : index + len(cfg_test_mod)])
        if values == cfg_test_mod:
            opening = next(
                position
                for position in range(index + len(cfg_test_mod), len(tokens))
                if tokens[position].value == "{"
            )
            index = matching(tokens, opening, "{", "}") + 1
            continue
        token = tokens[index]
        kept.append(token)
        if token.value == "{":
            brace_depth += 1
        elif token.value == "}":
            brace_depth -= 1
        index += 1
    return kept


def find_sequence(tokens: list[RustToken], values: tuple[str, ...], start=0, end=None) -> int:
    end = len(tokens) if end is None else end
    for index in range(start, end - len(values) + 1):
        if tuple(token.value for token in tokens[index : index + len(values)]) == values:
            return index
    raise AssertionError(f"missing Rust token sequence: {' '.join(values)}")


def audit_convergence_evidence(sources: dict[str, str]) -> None:
    token_sets = {path: production_tokens(source) for path, source in sources.items()}
    runtime = token_sets[RUNTIME]
    tracer = token_sets[TRACER]

    module = find_sequence(runtime, ("mod", MODULE, "{"))
    if any(token.value == "pub" for token in runtime[max(0, module - 5) : module]):
        raise AssertionError("convergence evidence module must remain private")
    module_open = module + 2
    module_close = matching(runtime, module_open, "{", "}")

    opaque_assertions: list[tuple[int, tuple[str, ...]]] = []
    for index, token in enumerate(runtime):
        if token.value != "assert_not_impl_any":
            continue
        if tuple(item.value for item in runtime[index + 1 : index + 3]) != ("!", "("):
            continue
        close = matching(runtime, index + 2, "(", ")")
        opaque_assertions.append(
            (index, tuple(item.value for item in runtime[index + 3 : close]))
        )
    expected_opaque_payloads = [
        (
            "DdgiBatchCompletion", ":",
            ":", ":", "core", ":", ":", "fmt", ":", ":", "Debug", ",",
            ":", ":", "core", ":", ":", "fmt", ":", ":", "Display",
        ),
        (
            "Pending", ":",
            ":", ":", "core", ":", ":", "fmt", ":", ":", "Debug", ",",
            ":", ":", "core", ":", ":", "fmt", ":", ":", "Display",
        ),
        (
            "Evidence", ":",
            ":", ":", "core", ":", ":", "fmt", ":", ":", "Debug", ",",
            ":", ":", "core", ":", ":", "fmt", ":", ":", "Display",
        ),
    ]
    if [payload for _, payload in opaque_assertions] != expected_opaque_payloads:
        raise AssertionError(
            "production rustc must own the exact Debug/Display assertions for the opaque types"
        )

    def declaration_end(type_name: str, start: int, end: int) -> int:
        declaration = find_sequence(runtime, ("struct", type_name), start, end)
        opening = next(
            index
            for index in range(declaration + 2, end)
            if runtime[index].value in ("{", "(")
        )
        closing = matching(
            runtime,
            opening,
            runtime[opening].value,
            "}" if runtime[opening].value == "{" else ")",
        )
        if runtime[opening].value == "(":
            if runtime[closing + 1].value != ";":
                raise AssertionError(f"{type_name} tuple struct must end with a semicolon")
            return closing + 1
        return closing

    assertion_positions = [position for position, _ in opaque_assertions]
    expected_positions = [
        declaration_end("DdgiBatchCompletion", 0, module) + 1,
        declaration_end("Pending", module_open, module_close) + 1,
        declaration_end("Evidence", module_open, module_close) + 1,
    ]
    for assertion, expected in zip(assertion_positions, expected_positions, strict=True):
        prefix = tuple(token.value for token in runtime[expected : expected + 6])
        if assertion != expected + 5 or prefix != (
            ":", ":", "static_assertions", ":", ":", "assert_not_impl_any"
        ):
            raise AssertionError(
                "each negative assertion must be an unconfigured direct item adjacent to its struct"
            )

    evidence_field = find_sequence(
        runtime,
        ("pending_convergence_evidence", ":", "Option", "<", MODULE, ":", ":", "Pending", ">"),
        0,
        module,
    )
    field_start = max(
        index
        for index in range(evidence_field)
        if runtime[index].value in ("{", ",")
    )
    if any(token.value == "pub" for token in runtime[field_start:evidence_field]):
        raise AssertionError("completion must hold the pending capability privately")
    impl_start = find_sequence(
        runtime,
        ("impl", "super", ":", ":", "DdgiBatchCompletion", "{"),
        module_open,
        module_close,
    )
    impl_open = impl_start + 5
    impl_close = matching(runtime, impl_open, "{", "}")
    definition = find_sequence(runtime, ("fn", COMMIT, "("), impl_open, impl_close)
    if not any(token.value == "pub" for token in runtime[impl_open:definition]):
        raise AssertionError("Tracer needs one narrow crate-visible commit capability")
    commit_open = next(
        index for index in range(definition, impl_close) if runtime[index].value == "{"
    )
    commit_close = matching(runtime, commit_open, "{", "}")
    commit_body = [token.value for token in runtime[commit_open + 1 : commit_close]]
    binding = commit_body[4] if len(commit_body) > 4 else ""
    expected_commit_body = [
        "if",
        "let",
        "Some",
        "(",
        binding,
        ")",
        "=",
        "self",
        ".",
        "pending_convergence_evidence",
        ".",
        "take",
        "(",
        ")",
        "{",
        binding,
        ".",
        "emit",
        "(",
        ")",
        ";",
        "}",
    ]
    if not binding or commit_body != expected_commit_body:
        raise AssertionError(
            "commit must take and emit the same private pending capability exactly once"
        )

    pending_impl = find_sequence(runtime, ("impl", "Pending", "{"), module_open, module_close)
    pending_impl_open = pending_impl + 2
    pending_impl_close = matching(runtime, pending_impl_open, "{", "}")
    emit = find_sequence(runtime, ("fn", "emit", "("), pending_impl_open, pending_impl_close)
    if any(token.value == "pub" for token in runtime[pending_impl_open:emit]):
        raise AssertionError("Pending::emit must remain child-private")
    emit_open = next(
        index for index in range(emit, pending_impl_close) if runtime[index].value == "{"
    )
    emit_close = matching(runtime, emit_open, "{", "}")
    emit_body = [token.value for token in runtime[emit_open + 1 : emit_close]]
    expected_emit_body = [
        "if",
        "log",
        ":",
        ":",
        "log_enabled",
        "!",
        "(",
        "target",
        ":",
        "TARGET",
        ",",
        "log",
        ":",
        ":",
        "Level",
        ":",
        ":",
        "Debug",
        ")",
        "{",
        "for",
        "line",
        "in",
        "self",
        ".",
        "0",
        ".",
        "lines",
        "(",
        ")",
        "{",
        "log",
        ":",
        ":",
        "debug",
        "!",
        "(",
        "target",
        ":",
        "TARGET",
        ",",
        "{line}",
        ")",
        ";",
        "}",
        "}",
    ]
    if emit_body != expected_emit_body:
        raise AssertionError("private emitter must match the canonical debug-gated sink body")

    target_literal = "re_flora::ddgi_convergence_evidence"
    target_literals = [
        (path, index)
        for path, tokens in token_sets.items()
        for index, token in enumerate(tokens)
        if token.kind == "STRING" and token.value == target_literal
    ]
    if len(target_literals) != 1 or target_literals[0][0] != RUNTIME:
        raise AssertionError("private child module must uniquely own the convergence log target")
    if not module_open < target_literals[0][1] < module_close:
        raise AssertionError("convergence log target escaped the private child module")

    marker_literals = [
        (path, index)
        for path, tokens in token_sets.items()
        for index, token in enumerate(tokens)
        if token.kind == "STRING" and "[DDGI_CONVERGENCE_EVIDENCE]" in token.value
    ]
    if len(marker_literals) != 2 or any(path != RUNTIME for path, _ in marker_literals):
        raise AssertionError("private child module must uniquely own both evidence markers")
    if any(not module_open < index < module_close for _, index in marker_literals):
        raise AssertionError("convergence evidence marker escaped the private child module")

    targeted_sinks = []
    for path, tokens in token_sets.items():
        for index in range(len(tokens) - 5):
            if (
                tokens[index].value == "log"
                and tokens[index + 1].value == ":"
                and tokens[index + 2].value == ":"
                and tokens[index + 3].value in ("trace", "debug", "info", "warn", "error")
                and tokens[index + 4].value == "!"
                and tokens[index + 5].value == "("
            ):
                close = matching(tokens, index + 5, "(", ")")
                arguments = tokens[index + 6 : close]
                has_target = any(
                    arguments[position].value == "target"
                    and arguments[position + 1].value == ":"
                    and arguments[position + 2].value == "TARGET"
                    for position in range(len(arguments) - 2)
                ) or any(
                    item.kind == "STRING" and item.value == target_literal
                    for item in arguments
                )
                if has_target:
                    targeted_sinks.append((path, index, tokens[index + 3].value))
    if targeted_sinks != [(RUNTIME, find_sequence(runtime, ("log", ":", ":", "debug", "!"), emit_open, emit_close), "debug")]:
        raise AssertionError("convergence target must have one canonical private debug sink")

    complete = find_sequence(runtime, ("fn", "complete_pending_batch", "("))
    complete_open = next(
        index for index in range(complete, len(runtime)) if runtime[index].value == "{"
    )
    complete_close = matching(runtime, complete_open, "{", "}")
    forbidden = {COMMIT, "emit"}
    if any(token.value in forbidden for token in runtime[complete_open:complete_close]):
        raise AssertionError("completion may prepare evidence but cannot commit or emit it")

    commit_calls = []
    for path, tokens in token_sets.items():
        for index in range(2, len(tokens) - 1):
            if tokens[index].value != COMMIT or tokens[index + 1].value != "(":
                continue
            if tokens[index - 1].value == ".":
                commit_calls.append((path, index, "member", tokens[index - 2].value))
            elif tokens[index - 1].value == ":" and tokens[index - 2].value == ":":
                close = matching(tokens, index + 1, "(", ")")
                arguments = tuple(item.value for item in tokens[index + 2 : close])
                target = tokens[index - 3].value if index >= 3 else ""
                commit_calls.append((path, index, "ufcs", (target, arguments)))
    if len(commit_calls) != 1 or commit_calls[0][0] != TRACER:
        raise AssertionError("commit capability must have one global Tracer call")

    pending = find_sequence(
        tracer,
        ("ddgi_trace_stats_readback_pending", ".", "take", "(", ")", "{"),
    )
    batch_open = pending + 5
    batch_close = matching(tracer, batch_open, "{", "}")
    candidates = []
    for index in range(batch_open, batch_close - 3):
        if tuple(token.value for token in tracer[index : index + 2]) != ("let", "mut"):
            continue
        if tracer[index + 2].kind != "IDENT" or tracer[index + 3].value != "=":
            continue
        stack = []
        statement_end = None
        pairs = {"(": ")", "[": "]", "{": "}"}
        for position in range(index + 4, batch_close):
            value = tracer[position].value
            if value in pairs:
                stack.append(pairs[value])
            elif stack and value == stack[-1]:
                stack.pop()
            elif value == ";" and not stack:
                statement_end = position
                break
        if statement_end is None:
            raise AssertionError("runtime completion binding has no statement terminator")
        if any(
            token.value == "complete_pending_batch"
            for token in tracer[index + 4 : statement_end]
        ):
            candidates.append((tracer[index + 2].value, statement_end))
    if len(candidates) != 1:
        raise AssertionError("batch block must bind exactly one runtime completion")
    binding, assignment_end = candidates[0]

    _, commit, call_kind, receiver = commit_calls[0]
    if call_kind == "member":
        if receiver != binding:
            raise AssertionError("Tracer committed a different completion receiver")
        call_close = matching(tracer, commit + 1, "(", ")")
    else:
        target, arguments = receiver
        if target != "DdgiBatchCompletion" or arguments != ("&", "mut", binding):
            raise AssertionError("Tracer UFCS commit must consume its bound completion")
        call_close = matching(tracer, commit + 1, "(", ")")
    if not batch_open < commit < batch_close:
        raise AssertionError("canonical commit must remain in the batch block")

    local_light = find_sequence(
        tracer,
        ("self", ".", "observe_ddgi_local_light_gpu_evidence", "("),
        assignment_end,
        batch_close,
    )
    local_close = matching(tracer, local_light + 3, "(", ")")
    if tracer[local_close + 1].value != "?" or commit <= local_close + 1:
        raise AssertionError("commit must follow successful local-light evidence")
    if tracer[call_close + 1].value != ";" or call_close + 2 != batch_close:
        raise AssertionError("commit must be the batch block's final executable statement")


def production_sources() -> dict[str, str]:
    return {
        path: (ROOT / path).read_text(encoding="utf-8")
        for path in (RUNTIME, TRACER)
    }


class DdgiConvergenceCapsuleSourceTests(unittest.TestCase):
    def test_private_runtime_capability_and_tracer_form_one_transaction(self) -> None:
        audit_convergence_evidence(production_sources())

    def test_rustc_negative_trait_assertions_are_exact_owner_contracts(self) -> None:
        sources = production_sources()
        assertions = {
            "completion": "DdgiBatchCompletion: ::core::fmt::Debug, ::core::fmt::Display",
            "pending": "Pending: ::core::fmt::Debug, ::core::fmt::Display",
            "evidence": "Evidence: ::core::fmt::Debug, ::core::fmt::Display",
        }
        mutations = {
            "missing-completion": sources[RUNTIME].replace(assertions["completion"], "", 1),
            "wrong-pending-target": sources[RUNTIME].replace(
                assertions["pending"],
                "Prepared: ::core::fmt::Debug, ::core::fmt::Display",
                1,
            ),
            "missing-evidence-display": sources[RUNTIME].replace(
                assertions["evidence"],
                "Evidence: ::core::fmt::Debug",
                1,
            ),
            "wrong-completion-trait": sources[RUNTIME].replace(
                assertions["completion"],
                "DdgiBatchCompletion: ::core::fmt::Debug, Clone",
                1,
            ),
            "relative-macro-crate": sources[RUNTIME].replace(
                "::static_assertions::assert_not_impl_any!(",
                "static_assertions::assert_not_impl_any!(",
                1,
            ),
            "relative-trait-path": sources[RUNTIME].replace(
                "::core::fmt::Debug, ::core::fmt::Display",
                "Debug, Display",
                1,
            ),
            "cfg-test-completion": sources[RUNTIME].replace(
                "::static_assertions::assert_not_impl_any!(\n"
                "    DdgiBatchCompletion: ::core::fmt::Debug, ::core::fmt::Display\n"
                ");",
                "#[cfg(test)]\n"
                "::static_assertions::assert_not_impl_any!(\n"
                "    DdgiBatchCompletion: ::core::fmt::Debug, ::core::fmt::Display\n"
                ");",
                1,
            ),
            "nested-cfg-any-pending": sources[RUNTIME].replace(
                "::static_assertions::assert_not_impl_any!(\n"
                "        Pending: ::core::fmt::Debug, ::core::fmt::Display\n"
                "    );",
                "#[cfg(any())]\n"
                "    mod disabled_seal {\n"
                "        ::static_assertions::assert_not_impl_any!(\n"
                "            super::Pending: ::core::fmt::Debug, ::core::fmt::Display\n"
                "        );\n"
                "    }",
                1,
            ),
            "nested-test-module-evidence": sources[RUNTIME].replace(
                "::static_assertions::assert_not_impl_any!(\n"
                "        Evidence: ::core::fmt::Debug, ::core::fmt::Display\n"
                "    );",
                "#[cfg(test)]\n"
                "    mod seal_tests {\n"
                "        ::static_assertions::assert_not_impl_any!(\n"
                "            super::Evidence: ::core::fmt::Debug, ::core::fmt::Display\n"
                "        );\n"
                "    }",
                1,
            ),
        }
        for name, runtime in mutations.items():
            with self.subTest(name=name):
                self.assertNotEqual(runtime, sources[RUNTIME])
                mutated = dict(sources)
                mutated[RUNTIME] = runtime
                with self.assertRaises(AssertionError):
                    audit_convergence_evidence(mutated)

        generic_wrapper = dict(sources)
        generic_wrapper[RUNTIME] += (
            "\nstruct DiagnosticWrapper<T>(T);\n"
            "impl<T> std::fmt::Debug for DiagnosticWrapper<T> {\n"
            "    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { todo!() }\n"
            "}\n"
        )
        audit_convergence_evidence(generic_wrapper)

    def test_comments_unrelated_names_and_bad_commit_order_do_not_fool_the_seam(self) -> None:
        sources = production_sources()
        call = "completion.commit_convergence_evidence();"
        mutations = {
            "line-comment": sources[TRACER].replace(call, f"// {call}", 1),
            "unrelated-name": sources[TRACER].replace(call, "other.commit_convergence_evidence();", 1),
            "early-call": sources[TRACER]
            .replace(call, "", 1)
            .replace(
                "self.observe_ddgi_local_light_gpu_evidence(",
                f"{call}\n                self.observe_ddgi_local_light_gpu_evidence(",
                1,
            ),
            "late-bail": sources[TRACER].replace(call, f'{call}\n            bail!("late");', 1),
            "late-return": sources[TRACER].replace(call, f'{call}\n            return Err(anyhow!("late"));', 1),
        }
        for name, tracer in mutations.items():
            with self.subTest(name=name):
                mutated = dict(sources)
                mutated[TRACER] = tracer
                with self.assertRaises(AssertionError):
                    audit_convergence_evidence(mutated)

        noisy = dict(sources)
        noisy[TRACER] += "\nimpl Unrelated { fn commit_convergence_evidence(&self) {} }\n"
        audit_convergence_evidence(noisy)

        renamed = dict(sources)
        renamed[TRACER] = renamed[TRACER].replace("completion", "batch_completion")
        audit_convergence_evidence(renamed)

        ufcs = dict(sources)
        ufcs[TRACER] = ufcs[TRACER].replace(
            call,
            "DdgiBatchCompletion::commit_convergence_evidence(&mut completion);",
            1,
        )
        audit_convergence_evidence(ufcs)

        boundary_mutations = {
            "visible-child-module": sources[RUNTIME].replace(
                "mod convergence_evidence {", "pub(crate) mod convergence_evidence {", 1
            ),
            "visible-completion-capability": sources[RUNTIME].replace(
                "    pending_convergence_evidence: Option<convergence_evidence::Pending>,",
                "    pub(crate) pending_convergence_evidence: Option<convergence_evidence::Pending>,",
                1,
            ),
            "visible-emitter": sources[RUNTIME].replace(
                "        fn emit(self)", "        pub(super) fn emit(self)", 1
            ),
            "direct-completion-emission": sources[RUNTIME].replace(
                "        let after = self.volumes().builder().status();",
                "        pending.emit();\n        let after = self.volumes().builder().status();",
                1,
            ),
            "empty-commit": sources[RUNTIME].replace(
                "            if let Some(pending) = self.pending_convergence_evidence.take() {\n"
                "                pending.emit();\n"
                "            }",
                "",
                1,
            ),
            "discarded-take": sources[RUNTIME].replace(
                "            if let Some(pending) = self.pending_convergence_evidence.take() {",
                "            let _ = self.pending_convergence_evidence.take();\n"
                "            if let Some(pending) = self.pending_convergence_evidence.take() {",
                1,
            ),
            "different-emit-receiver": sources[RUNTIME].replace(
                "                pending.emit();", "                decoy.emit();", 1
            ),
            "missing-emit": sources[RUNTIME].replace("                pending.emit();", "", 1),
            "parent-helper-early-member-commit": sources[RUNTIME]
            + "\nfn parent_commit(mut alias: DdgiBatchCompletion) {\n"
            "    alias.commit_convergence_evidence();\n"
            "}\n",
            "parent-helper-early-ufcs-commit": sources[RUNTIME]
            + "\nfn parent_commit(mut alias: DdgiBatchCompletion) {\n"
            "    DdgiBatchCompletion::commit_convergence_evidence(&mut alias);\n"
            "}\n",
            "extra-target-outside-child": sources[RUNTIME]
            + '\nconst EXTRA_TARGET: &str = "re_flora::ddgi_convergence_evidence";\n',
            "extra-marker-outside-child": sources[RUNTIME]
            + '\nconst EXTRA_MARKER: &str = "[DDGI_CONVERGENCE_EVIDENCE] decoy";\n',
        }
        for name, runtime in boundary_mutations.items():
            with self.subTest(name=name):
                mutated = dict(sources)
                mutated[RUNTIME] = runtime
                with self.assertRaises(AssertionError):
                    audit_convergence_evidence(mutated)

        emitter_mutations = {
            "disabled-gate": sources[RUNTIME].replace(
                "if log::log_enabled!(target: TARGET, log::Level::Debug) {", "if false {", 1
            ),
            "outer-loop": sources[RUNTIME].replace(
                "            if log::log_enabled!(target: TARGET, log::Level::Debug) {",
                "            for _ in 0..2 {\n"
                "                if log::log_enabled!(target: TARGET, log::Level::Debug) {",
                1,
            ).replace(
                "                }\n            }\n        }\n    }\n\n    impl Evidence",
                "                }\n                }\n            }\n        }\n    }\n\n    impl Evidence",
                1,
            ),
            "zero-sink": sources[RUNTIME].replace(
                '                    log::debug!(target: TARGET, "{line}");',
                "                    let _ = line;",
                1,
            ),
            "double-sink": sources[RUNTIME].replace(
                '                    log::debug!(target: TARGET, "{line}");',
                '                    log::debug!(target: TARGET, "{line}");\n'
                '                    log::debug!(target: TARGET, "{line}");',
                1,
            ),
            "second-target-sink": sources[RUNTIME].replace(
                '                    log::debug!(target: TARGET, "{line}");',
                '                    log::debug!(target: TARGET, "{line}");\n'
                '                    log::info!(target: TARGET, "{line}");',
                1,
            ),
        }
        for name, runtime in emitter_mutations.items():
            with self.subTest(name=name):
                self.assertNotEqual(runtime, sources[RUNTIME])
                mutated = dict(sources)
                mutated[RUNTIME] = runtime
                with self.assertRaises(AssertionError):
                    audit_convergence_evidence(mutated)


if __name__ == "__main__":
    unittest.main()
