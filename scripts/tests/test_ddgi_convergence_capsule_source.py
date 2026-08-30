from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class DdgiConvergenceCapsuleSourceTests(unittest.TestCase):
    def test_tracer_cannot_reassociate_validation_count_from_completion_status(self) -> None:
        tracer = (ROOT / "src" / "tracer" / "mod.rs").read_text(encoding="utf-8")
        start = tracer.index("if let Some(publication) = completion.validated_publication")
        end = tracer.index("let lighting = self.ddgi_runtime.lighting_diagnostics()", start)
        evidence_scope = tracer[start:end]

        self.assertRegex(
            evidence_scope,
            re.compile(r"publication\s*\.consecutive_below_threshold\(\)"),
        )
        self.assertNotIn("completion.status", evidence_scope)
        self.assertNotIn("status.consecutive_below_threshold", evidence_scope)
        self.assertNotIn("completion.status.consecutive_below_threshold", evidence_scope)

    def test_capsule_privately_owns_the_count_from_both_typed_outcomes(self) -> None:
        runtime = (ROOT / "src" / "ddgi" / "runtime.rs").read_text(encoding="utf-8")
        capsule_start = runtime.index("pub(crate) struct DdgiValidatedPublication")
        capsule_end = runtime.index("impl DdgiBatchCompletion", capsule_start)
        capsule_scope = runtime[capsule_start:capsule_end]

        self.assertIn("consecutive_below_threshold: u32", capsule_scope)
        self.assertIn("fn consecutive_below_threshold(self) -> u32", capsule_scope)
        self.assertNotIn("pub consecutive_below_threshold", capsule_scope)


if __name__ == "__main__":
    unittest.main()
