from __future__ import annotations

import sys
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import check_shader_manifest as manifest  # noqa: E402


class ShaderVariantWrapperTests(unittest.TestCase):
    def test_accepts_a_constrained_compile_time_variant(self) -> None:
        source = (
            "#define RE_FLORA_GLASS_TRANSPORT 1\n"
            "#define RE_FLORA_GLASS_PRIMARY 1\n"
            '#include "tracer.slang"\n'
        )

        self.assertEqual(manifest.variant_wrapper_target(source), "tracer.slang")

    def test_rejects_a_general_textual_include(self) -> None:
        self.assertIsNone(
            manifest.variant_wrapper_target('#include "composition.slang"\n')
        )

    def test_rejects_non_project_defines_and_extra_source(self) -> None:
        self.assertIsNone(
            manifest.variant_wrapper_target(
                "#define THIRD_PARTY_SWITCH 1\n"
                '#include "composition.slang"\n'
            )
        )
        self.assertIsNone(
            manifest.variant_wrapper_target(
                "#define RE_FLORA_GLASS_TRANSPORT 1\n"
                '#include "composition.slang"\n'
                "void unexpected() {}\n"
            )
        )

    def test_rejects_a_variant_target_outside_the_shader_directory(self) -> None:
        self.assertIsNone(
            manifest.variant_wrapper_target(
                "#define RE_FLORA_GLASS_TRANSPORT 1\n"
                '#include "../composition.slang"\n'
            )
        )


if __name__ == "__main__":
    unittest.main()
