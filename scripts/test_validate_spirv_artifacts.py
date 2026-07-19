#!/usr/bin/env python3
"""Deterministic tests for generated SPIR-V artifact inventory validation."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import validate_spirv_artifacts


class ValidateSpirvArtifactsTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary_directory.cleanup)
        self.output = Path(self.temporary_directory.name) / "out"
        self.root = self.output / "precompiled-shaders"
        self.root.mkdir(parents=True)

    def write_generated_inventory(self, artifact_paths: list[str]) -> None:
        lines = [
            f'include_bytes!(concat!(env!("OUT_DIR"), "/precompiled-shaders/{path}"))'
            for path in artifact_paths
        ]
        (self.output / "precompiled_shaders.rs").write_text("\n".join(lines), encoding="utf-8")

    def write_artifact(self, relative_path: str) -> None:
        artifact = self.root / relative_path
        artifact.parent.mkdir(parents=True, exist_ok=True)
        artifact.write_bytes(b"SPIR-V")

    def test_accepts_exact_generated_inventory_without_fixed_count(self) -> None:
        artifacts = [
            "shader/first.comp.reflection.spv",
            "shader/first.comp.optimized.spv",
            "shader/new.frag.reflection.spv",
            "shader/new.frag.optimized.spv",
        ]
        self.write_generated_inventory(artifacts)
        for artifact in artifacts:
            self.write_artifact(artifact)

        validated = validate_spirv_artifacts.validated_artifact_paths(self.root)

        self.assertEqual(
            [path.relative_to(self.root).as_posix() for path in validated], sorted(artifacts)
        )

    def test_reports_missing_and_unexpected_artifacts(self) -> None:
        self.write_generated_inventory(
            ["shader/tree.vert.reflection.spv", "shader/tree.vert.optimized.spv"]
        )
        self.write_artifact("shader/tree.vert.reflection.spv")
        self.write_artifact("shader/stale.vert.optimized.spv")

        with self.assertRaisesRegex(
            RuntimeError, r"(?s)missing \(1\).+unexpected \(1\)"
        ):
            validate_spirv_artifacts.validated_artifact_paths(self.root)

    def test_rejects_generated_output_without_artifact_references(self) -> None:
        self.write_generated_inventory([])

        with self.assertRaisesRegex(RuntimeError, "no SPIR-V artifacts referenced"):
            validate_spirv_artifacts.validated_artifact_paths(self.root)


if __name__ == "__main__":
    unittest.main()
