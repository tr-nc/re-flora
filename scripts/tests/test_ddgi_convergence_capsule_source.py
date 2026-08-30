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
    runtime = production_tokens(sources[RUNTIME])
    tracer = production_tokens(sources[TRACER])

    module = find_sequence(runtime, ("mod", MODULE, "{"))
    if any(token.value == "pub" for token in runtime[max(0, module - 5) : module]):
        raise AssertionError("convergence evidence module must remain private")
    module_open = module + 2
    module_close = matching(runtime, module_open, "{", "}")

    def struct_position(type_name: str, start: int, end: int) -> int:
        return find_sequence(runtime, ("struct", type_name), start, end)

    def rejects_debug(type_name: str, start: int, end: int) -> None:
        position = struct_position(type_name, start, end)
        declaration_start = max(
            (
                index + 1
                for index in range(start, position)
                if runtime[index].value in (";", "}")
            ),
            default=start,
        )
        if any(
            token.value == "Debug"
            for token in runtime[declaration_start:position]
        ):
            raise AssertionError(f"{type_name} must not derive Debug")
        for index in range(start + 1, end):
            if runtime[index].value != type_name or runtime[index - 1].value != "for":
                continue
            impl = max(
                (
                    position
                    for position in range(start, index)
                    if runtime[position].value == "impl"
                ),
                default=start,
            )
            if any(token.value == "Debug" for token in runtime[impl:index]):
                raise AssertionError(f"{type_name} must not expose Debug")

    rejects_debug("DdgiBatchCompletion", 0, module)
    for type_name in ("Pending", "Evidence"):
        rejects_debug(type_name, module_open, module_close)
    if any(token.value in ("Debug", "Display") for token in runtime[module_open:module_close]):
        raise AssertionError("private evidence representation must not expose formatting traits")

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
    sinks = [
        index
        for index in range(module_open, module_close - 4)
        if tuple(token.value for token in runtime[index : index + 5])
        == ("log", ":", ":", "debug", "!")
    ]
    if len(sinks) != 1 or not emit_open < sinks[0] < emit_close:
        raise AssertionError("private emitter must own the child module's unique log sink")
    find_sequence(
        runtime,
        ("for", "line", "in", "self", ".", "0", ".", "lines", "(", ")", "{"),
        emit_open,
        emit_close,
    )
    find_sequence(
        runtime,
        ("log", ":", ":", "debug", "!", "(", "target", ":", "TARGET"),
        emit_open,
        emit_close,
    )
    if not any(
        token.kind == "STRING" and token.value == "{line}"
        for token in runtime[sinks[0] : emit_close]
    ):
        raise AssertionError("private log sink must emit each exact evidence line")

    complete = find_sequence(runtime, ("fn", "complete_pending_batch", "("))
    complete_open = next(
        index for index in range(complete, len(runtime)) if runtime[index].value == "{"
    )
    complete_close = matching(runtime, complete_open, "{", "}")
    forbidden = {COMMIT, "emit"}
    if any(token.value in forbidden for token in runtime[complete_open:complete_close]):
        raise AssertionError("completion may prepare evidence but cannot commit or emit it")

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

    dot_calls = [
        (index, matching(tracer, index + 1, "(", ")"))
        for index in range(batch_open + 2, batch_close - 1)
        if tracer[index].value == COMMIT
        and tracer[index - 1].value == "."
        and tracer[index - 2].value == binding
        and tracer[index + 1].value == "("
    ]
    ufcs_calls = []
    for index in range(batch_open, batch_close - 9):
        if tuple(token.value for token in tracer[index : index + 6]) == (
            "DdgiBatchCompletion",
            ":",
            ":",
            COMMIT,
            "(",
            "&",
        ):
            call_close = matching(tracer, index + 4, "(", ")")
            arguments = tuple(token.value for token in tracer[index + 5 : call_close])
            if arguments == ("&", "mut", binding):
                ufcs_calls.append((index + 3, call_close))
    calls = dot_calls + ufcs_calls
    if len(calls) != 1:
        raise AssertionError("Tracer must commit its bound completion exactly once")
    commit, call_close = calls[0]

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
            "debug-completion": sources[RUNTIME].replace(
                "pub(crate) struct DdgiBatchCompletion {",
                "#[derive(Debug)]\npub(crate) struct DdgiBatchCompletion {",
                1,
            ),
            "debug-pending": sources[RUNTIME].replace(
                "pub(super) struct Pending", "#[derive(Debug)]\n    pub(super) struct Pending", 1
            ),
            "debug-evidence": sources[RUNTIME].replace(
                "    struct Evidence {", "    #[derive(Debug)]\n    struct Evidence {", 1
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
        }
        for name, runtime in boundary_mutations.items():
            with self.subTest(name=name):
                mutated = dict(sources)
                mutated[RUNTIME] = runtime
                with self.assertRaises(AssertionError):
                    audit_convergence_evidence(mutated)


if __name__ == "__main__":
    unittest.main()
