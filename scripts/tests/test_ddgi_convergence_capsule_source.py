from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
VALIDATION_MARKER = "[DDGI_CONVERGENCE_EVIDENCE] full-atlas validated"
TERMINAL_MARKER = "[DDGI_CONVERGENCE_EVIDENCE] terminal"


def assert_runtime_owns_convergence_evidence(runtime: str, tracer: str) -> None:
    runtime_production = runtime.rsplit("\n#[cfg(test)]\nmod tests", 1)[0]
    tracer_production = tracer
    if (
        runtime_production.count(VALIDATION_MARKER) != 1
        or runtime_production.count(TERMINAL_MARKER) != 1
    ):
        raise AssertionError("runtime must uniquely own both convergence evidence markers")
    if VALIDATION_MARKER in tracer_production or TERMINAL_MARKER in tracer_production:
        raise AssertionError("Tracer must not own convergence evidence markers")

    capsule = re.search(
        r"pub\(crate\) struct DdgiValidatedPublication\s*\{(?P<body>.*?)\n\}",
        runtime_production,
        re.DOTALL,
    )
    if capsule is None:
        raise AssertionError("validated publication capsule is missing")
    for line in capsule.group("body").splitlines():
        if ":" in line and re.match(r"\s*pub(?:\([^)]*\))?\s+", line):
            raise AssertionError("validated publication capsule fields must remain private")

    publication_start = tracer_production.find(
        "if let Some(publication) = completion.validated_publication"
    )
    if publication_start < 0:
        raise AssertionError("Tracer publication diagnostics are missing")
    publication_end = tracer_production.find(
        "let lighting = self.ddgi_runtime.lighting_diagnostics()", publication_start
    )
    if publication_end < 0:
        raise AssertionError("Tracer publication diagnostic seam is malformed")
    publication_scope = tracer_production[publication_start:publication_end]
    if "completion.status" in publication_scope:
        raise AssertionError("public runtime status must not feed convergence evidence")

    completion_start = runtime_production.find("pub(crate) fn complete_pending_batch(")
    completion_end = runtime_production.find("/// Semantic work identity", completion_start)
    completion_scope = runtime_production[completion_start:completion_end]
    emission_call = "completion.emit_convergence_evidence();"
    if completion_scope.count(emission_call) != 1:
        raise AssertionError("runtime must have exactly one successful evidence emission")
    emission = completion_scope.rfind(emission_call)
    result = completion_scope.rfind("Ok(completion)")
    if emission < 0 or result < 0 or emission > result:
        raise AssertionError("runtime must emit evidence at its final successful completion seam")
    if "?" in completion_scope[emission:result]:
        raise AssertionError("fallible completion work must finish before evidence emission")


class DdgiConvergenceCapsuleSourceTests(unittest.TestCase):
    def test_runtime_uniquely_owns_convergence_evidence_emission(self) -> None:
        runtime = (ROOT / "src" / "ddgi" / "runtime.rs").read_text(encoding="utf-8")
        tracer = (ROOT / "src" / "tracer" / "mod.rs").read_text(encoding="utf-8")

        assert_runtime_owns_convergence_evidence(runtime, tracer)
        owners = {
            path.relative_to(ROOT).as_posix()
            for path in (ROOT / "src").rglob("*.rs")
            if any(
                marker in path.read_text(encoding="utf-8")
                for marker in (VALIDATION_MARKER, TERMINAL_MARKER)
            )
        }
        self.assertEqual(owners, {"src/ddgi/runtime.rs"})

    def test_public_status_alias_cannot_restore_tracer_evidence_reconstruction(self) -> None:
        runtime = (ROOT / "src" / "ddgi" / "runtime.rs").read_text(encoding="utf-8")
        tracer = (ROOT / "src" / "tracer" / "mod.rs").read_text(encoding="utf-8")
        mutated_runtime = runtime.replace(
            "pub building_field: Option<DdgiFieldIdentity>,",
            "pub building_field: Option<DdgiFieldIdentity>,\n    pub validation_streak: u32,",
            1,
        )
        mutated_tracer = tracer.replace(
            "if let Some(publication) = completion.validated_publication {",
            "if let Some(publication) = completion.validated_publication {\n"
            "                    let _validation_count = completion.status.validation_streak;",
            1,
        )

        with self.assertRaisesRegex(
            AssertionError, "public runtime status must not feed convergence evidence"
        ):
            assert_runtime_owns_convergence_evidence(mutated_runtime, mutated_tracer)


if __name__ == "__main__":
    unittest.main()
