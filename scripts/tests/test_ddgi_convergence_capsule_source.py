from __future__ import annotations

from dataclasses import dataclass
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
RUNTIME = "src/ddgi/runtime.rs"
TRACER = "src/tracer/mod.rs"
COMMIT = "commit_convergence_evidence"
VALIDATION_MARKER = "[DDGI_CONVERGENCE_EVIDENCE] full-atlas validated"
TERMINAL_MARKER = "[DDGI_CONVERGENCE_EVIDENCE] terminal"


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
            (
                prefix
                for prefix in ("b", "c", "")
                if source.startswith(prefix + '"', index)
            ),
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


def production_tokens(source: str) -> list[RustToken]:
    tokens = rust_tokens(source)
    kept: list[RustToken] = []
    index = 0
    brace_depth = 0
    cfg_test_mod = ("#", "[", "cfg", "(", "test", ")", "]", "mod")
    while index < len(tokens):
        values = tuple(token.value for token in tokens[index : index + len(cfg_test_mod)])
        if brace_depth == 0 and values == cfg_test_mod:
            opening = next(
                position
                for position in range(index + len(cfg_test_mod), len(tokens))
                if tokens[position].value == "{"
            )
            depth = 1
            cursor = opening + 1
            while depth:
                if tokens[cursor].value == "{":
                    depth += 1
                elif tokens[cursor].value == "}":
                    depth -= 1
                cursor += 1
            index = cursor
            continue
        token = tokens[index]
        kept.append(token)
        if token.value == "{":
            brace_depth += 1
        elif token.value == "}":
            brace_depth -= 1
        index += 1
    return kept


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


def audit_convergence_evidence(sources: dict[str, str]) -> None:
    token_sets = {path: production_tokens(source) for path, source in sources.items()}
    for marker in (VALIDATION_MARKER, TERMINAL_MARKER):
        owners = [
            path
            for path, tokens in token_sets.items()
            for token in tokens
            if token.kind == "STRING" and token.value.startswith(marker)
        ]
        if owners != [RUNTIME]:
            raise AssertionError("runtime formatter must uniquely own evidence marker literals")

    occurrences = [
        (path, index)
        for path, tokens in token_sets.items()
        for index, token in enumerate(tokens)
        if token.kind == "IDENT" and token.value == COMMIT
    ]
    if len(occurrences) != 2:
        raise AssertionError("commit capability must have one definition and one call")
    runtime_occurrences = [index for path, index in occurrences if path == RUNTIME]
    tracer_occurrences = [index for path, index in occurrences if path == TRACER]
    if len(runtime_occurrences) != 1 or len(tracer_occurrences) != 1:
        raise AssertionError("commit capability ownership is split across the wrong modules")

    runtime = token_sets[RUNTIME]
    definition = runtime_occurrences[0]
    if runtime[definition - 1].value != "fn":
        raise AssertionError("runtime must own the commit capability definition")
    capsule = next(
        index
        for index, token in enumerate(runtime)
        if token.value == "DdgiValidatedPublication" and runtime[index - 1].value == "struct"
    )
    capsule_open = next(
        index for index in range(capsule, len(runtime)) if runtime[index].value == "{"
    )
    capsule_close = matching(runtime, capsule_open, "{", "}")
    if any(token.value == "pub" for token in runtime[capsule_open + 1 : capsule_close]):
        raise AssertionError("validated publication capsule fields must remain private")

    completion_struct = next(
        index
        for index, token in enumerate(runtime)
        if token.value == "DdgiBatchCompletion" and runtime[index - 1].value == "struct"
    )
    completion_open = next(
        index
        for index in range(completion_struct, len(runtime))
        if runtime[index].value == "{"
    )
    completion_close = matching(runtime, completion_open, "{", "}")
    evidence_field = next(
        index
        for index in range(completion_open, completion_close)
        if runtime[index].value == "validated_publication"
    )
    field_start = max(
        index
        for index in range(completion_open, evidence_field)
        if runtime[index].value in ("{", ",")
    )
    if any(
        token.value == "pub" for token in runtime[field_start + 1 : evidence_field]
    ):
        raise AssertionError("completion evidence capability must remain private")

    commit_open = next(
        index for index in range(definition, len(runtime)) if runtime[index].value == "{"
    )
    commit_close = matching(runtime, commit_open, "{", "}")
    commit_values = [token.value for token in runtime[commit_open:commit_close]]
    if not any(
        commit_values[index : index + 6]
        == ["self", ".", "validated_publication", ".", "take", "("]
        for index in range(len(commit_values) - 5)
    ):
        raise AssertionError("commit capability must consume its private evidence once")

    complete_pending = next(
        index
        for index, token in enumerate(runtime)
        if token.value == "complete_pending_batch" and runtime[index - 1].value == "fn"
    )
    complete_open = next(
        index for index in range(complete_pending, len(runtime)) if runtime[index].value == "{"
    )
    complete_close = matching(runtime, complete_open, "{", "}")
    if any(
        token.kind == "IDENT" and token.value == COMMIT
        for token in runtime[complete_open + 1 : complete_close]
    ):
        raise AssertionError("runtime completion must return, not commit, evidence")

    tracer = token_sets[TRACER]
    commit = tracer_occurrences[0]
    if tracer[commit - 1].value != "." or tracer[commit + 1].value != "(":
        raise AssertionError("Tracer must invoke the narrow commit capability as one method call")
    commit_close = matching(tracer, commit + 1, "(", ")")
    local_light = next(
        index
        for index, token in enumerate(tracer)
        if token.value == "observe_ddgi_local_light_gpu_evidence"
        and tracer[index - 1].value == "."
    )
    local_open = next(
        index for index in range(local_light, len(tracer)) if tracer[index].value == "("
    )
    local_close = matching(tracer, local_open, "(", ")")
    if tracer[local_close + 1].value != "?" or commit <= local_close + 1:
        raise AssertionError("commit must follow successful local-light evidence")

    pending = max(
        index
        for index in range(local_light)
        if tracer[index].value == "ddgi_trace_stats_readback_pending"
    )
    batch_open = next(
        index for index in range(pending, local_light) if tracer[index].value == "{"
    )
    batch_close = matching(tracer, batch_open, "{", "}")
    if not batch_open < commit < batch_close:
        raise AssertionError("commit must remain in the batch completion block")
    if any(token.value == "?" for token in tracer[commit_close + 1 : batch_close]):
        raise AssertionError("batch evidence work must not fail after commit")


def production_sources() -> dict[str, str]:
    return {
        path.relative_to(ROOT).as_posix(): path.read_text(encoding="utf-8")
        for path in (ROOT / "src").rglob("*.rs")
    }


class DdgiConvergenceCapsuleSourceTests(unittest.TestCase):
    def test_runtime_capsule_and_tracer_commit_form_one_evidence_transaction(self) -> None:
        audit_convergence_evidence(production_sources())

    def test_comments_literals_ufcs_and_early_calls_cannot_bypass_the_transaction(self) -> None:
        sources = production_sources()
        call = "completion.commit_convergence_evidence();"
        mutations = {
            "line-comment": sources[TRACER].replace(call, f"// {call}", 1),
            "block-comment": sources[TRACER].replace(call, f"/* {call} */", 1),
            "ufcs-second-call": sources[TRACER].replace(
                call,
                f"{call}\n                DdgiBatchCompletion::{COMMIT}(&mut completion);",
                1,
            ),
            "raw-id-second-call": sources[TRACER].replace(
                call,
                f"{call}\n                completion.r#{COMMIT}();",
                1,
            ),
            "early-call": sources[TRACER]
            .replace(call, "", 1)
            .replace(
                "self.observe_ddgi_local_light_gpu_evidence(",
                f"{call}\n                self.observe_ddgi_local_light_gpu_evidence(",
                1,
            ),
        }
        for name, tracer in mutations.items():
            with self.subTest(name=name):
                mutated = dict(sources)
                mutated[TRACER] = tracer
                with self.assertRaises(AssertionError):
                    audit_convergence_evidence(mutated)

        marker_comment = dict(sources)
        marker_comment[RUNTIME] = (
            sources[RUNTIME].replace(VALIDATION_MARKER, "not-evidence", 1)
            + f"\n// {VALIDATION_MARKER}\n"
        )
        with self.assertRaisesRegex(AssertionError, "marker literals"):
            audit_convergence_evidence(marker_comment)

        noisy = dict(sources)
        noisy[TRACER] += (
            f'\n// {COMMIT} {VALIDATION_MARKER}\n'
            f'const _: &str = "{COMMIT}";\n'
            f'const _: &str = r#"{COMMIT}"#;\n'
            f'const _: &[u8] = b"{COMMIT}";\n'
            f'const _: &[u8] = br#"{COMMIT}"#;\n'
            f'const _: &CStr = c"{COMMIT}";\n'
            f'const _: &CStr = cr#"{COMMIT}"#;\n'
            "const _: char = 'x';\n"
            "const _: u8 = b'x';\n"
        )
        audit_convergence_evidence(noisy)


if __name__ == "__main__":
    unittest.main()
