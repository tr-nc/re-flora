import math
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
            "static const float2 DITHER_HASH_SCALE = float2(0.06711056, 0.00583715);",
            dither_source,
        )
        self.assertIn(
            "static const float DITHER_HASH_MULTIPLIER = 52.9829189;",
            dither_source,
        )
        self.assertIn(
            "static const float DITHER_HASH_PHASE = 0.125;",
            dither_source,
        )
        self.assertIn("float hashInput = frac(dot(float2(screenSpaceUv), DITHER_HASH_SCALE)", dither_source)
        self.assertIn("float noise = frac(DITHER_HASH_MULTIPLIER * hashInput);", dither_source)
        self.assertIn("return (noise - 0.5) * OUTPUT_QUANTIZATION_STEP;", dither_source)
        self.assertIn("float ditherOffset = getDitherOffset(uvi);", post_processing_source)
        self.assertIn("finalColor += ditherOffset.xxx;", post_processing_source)

    def test_hash_sample_is_centered_and_bounded_to_half_an_output_step(self) -> None:
        normalized_offsets = []
        for y in range(162):
            for x in range(288):
                hash_input = (0.06711056 * x + 0.00583715 * y + 0.125) % 1.0
                noise = (52.9829189 * hash_input) % 1.0
                normalized_offsets.append(noise - 0.5)

        self.assertLess(abs(math.fsum(normalized_offsets) / len(normalized_offsets)), 0.001)
        self.assertGreaterEqual(min(normalized_offsets), -0.5)
        self.assertLessEqual(max(normalized_offsets), 0.5)


if __name__ == "__main__":
    unittest.main()
