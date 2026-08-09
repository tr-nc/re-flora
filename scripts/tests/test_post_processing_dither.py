from fractions import Fraction
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
DITHER_SHADER = ROOT / "shader/slang/dither.slang"
POST_PROCESSING_SHADER = ROOT / "shader/slang/post_processing.slang"


class PostProcessingDitherTests(unittest.TestCase):
    def test_shader_uses_scalar_output_offset_at_final_write(self) -> None:
        dither_source = DITHER_SHADER.read_text(encoding="utf-8")
        post_processing_source = POST_PROCESSING_SHADER.read_text(encoding="utf-8")

        self.assertIn(
            "static const float OUTPUT_QUANTIZATION_STEP = 1.0 / 255.0;",
            dither_source,
        )
        self.assertIn(
            "static const float2 DITHER_GRID = float2(1.0 / 16.0, 10.0 / 36.0);",
            dither_source,
        )
        self.assertIn(
            "static const float DITHER_PHASE = 0.25 + 1.0 / 288.0;",
            dither_source,
        )
        self.assertIn("return (gridPosition - 0.5) * OUTPUT_QUANTIZATION_STEP;", dither_source)
        self.assertIn("float ditherOffset = getDitherOffset(uvi);", post_processing_source)
        self.assertIn("finalColor += ditherOffset.xxx;", post_processing_source)

    def test_declared_period_is_zero_mean_and_below_half_an_output_step(self) -> None:
        normalized_offsets = [
            (Fraction(x, 16) + Fraction(5 * y, 18) + Fraction(73, 288)) % 1
            - Fraction(1, 2)
            for y in range(18)
            for x in range(16)
        ]

        self.assertEqual(sum(normalized_offsets), 0)
        self.assertEqual(min(normalized_offsets), -Fraction(143, 288))
        self.assertEqual(max(normalized_offsets), Fraction(143, 288))
        self.assertLess(max(abs(offset) for offset in normalized_offsets), Fraction(1, 2))


if __name__ == "__main__":
    unittest.main()
