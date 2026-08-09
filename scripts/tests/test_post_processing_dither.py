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
            "static const uint DITHER_HASH_SEED = 0x12345678u;",
            dither_source,
        )
        self.assertIn(
            "uint hash = pixel.x * 0x9E3779B9u + pixel.y * 0x85EBCA6Bu + DITHER_HASH_SEED;",
            dither_source,
        )
        self.assertIn(
            "hash ^= hash >> 16;",
            dither_source,
        )
        self.assertIn("hash *= 0x7FEB352Du;", dither_source)
        self.assertIn("hash ^= hash >> 15;", dither_source)
        self.assertIn("float noise = float(hash & 0xFFFFu) / 65535.0;", dither_source)
        self.assertIn("return (noise - 0.5) * OUTPUT_QUANTIZATION_STEP;", dither_source)
        self.assertIn("float ditherOffset = getDitherOffset(uvi);", post_processing_source)
        self.assertIn("finalColor += ditherOffset.xxx;", post_processing_source)

    def test_hash_sample_is_centered_and_bounded_to_half_an_output_step(self) -> None:
        normalized_offsets = []
        for y in range(162):
            for x in range(288):
                hash_value = (
                    x * 0x9E3779B9 + y * 0x85EBCA6B + 0x12345678
                ) & 0xFFFFFFFF
                hash_value ^= hash_value >> 16
                hash_value = (hash_value * 0x7FEB352D) & 0xFFFFFFFF
                hash_value ^= hash_value >> 15
                noise = (hash_value & 0xFFFF) / 65535.0
                normalized_offsets.append(noise - 0.5)

        self.assertLess(abs(math.fsum(normalized_offsets) / len(normalized_offsets)), 0.001)
        self.assertGreaterEqual(min(normalized_offsets), -0.5)
        self.assertLessEqual(max(normalized_offsets), 0.5)


if __name__ == "__main__":
    unittest.main()
