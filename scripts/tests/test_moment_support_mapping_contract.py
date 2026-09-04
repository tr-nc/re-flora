from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import analyze_environment_irradiance_capture as analyzer  # noqa: E402


class MomentSupportStaticMappingContractTests(unittest.TestCase):
    def test_rust_cli_analyzer_and_slang_share_id_and_channels(self) -> None:
        ddgi_rust = (ROOT / "src/ddgi/mod.rs").read_text(encoding="utf-8")
        cli_rust = (ROOT / "src/cli.rs").read_text(encoding="utf-8")
        tracer = (ROOT / "shader/slang/tracer.slang").read_text(encoding="utf-8")

        self.assertRegex(ddgi_rust, r"\bMomentSupport\s*=\s*22,")
        self.assertIn('"moment-support" => Some(Self::MomentSupport)', ddgi_rust)
        self.assertIn('Self::MomentSupport => "moment-support"', ddgi_rust)
        self.assertIn(
            'parse(&["re-flora", "--ddgi-debug-view", "moment-support"])',
            cli_rust,
        )
        self.assertEqual(analyzer.DEBUG_VIEW_LABELS[22], "moment-support")
        self.assertRegex(
            tracer,
            r"static const uint DDGI_DEBUG_MOMENT_SUPPORT\s*=\s*22u;",
        )

        branch = re.search(
            r"if \(view == DDGI_DEBUG_MOMENT_SUPPORT\)\s*"
            r"return float3\((.*?)\);",
            tracer,
            re.DOTALL,
        )
        self.assertIsNotNone(branch)
        channels = [channel.strip() for channel in branch.group(1).split(",")]
        self.assertEqual(
            channels,
            [
                "momentResult.accumulated_base_weight",
                "momentResult.accumulated_weight",
                "momentResult.dominant_probe_weight",
            ],
        )


if __name__ == "__main__":
    unittest.main()
