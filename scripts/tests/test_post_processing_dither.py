import math
from pathlib import Path
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[2]
DITHER_SHADER = ROOT / "shader/slang/dither.slang"
POST_PROCESSING_SHADER = ROOT / "shader/slang/post_processing.slang"
GUI_CONFIG = ROOT / "config/gui.toml"
EXTENT_DEPENDENT_RESOURCES = ROOT / "src/tracer/extent_dependent_resources.rs"


class PostProcessingDitherTests(unittest.TestCase):
    def test_linear_scene_color_stays_float_until_srgb_output_blit(self) -> None:
        resources_source = EXTENT_DEPENDENT_RESOURCES.read_text(encoding="utf-8")
        screen_output_factory = resources_source.split(
            "fn create_screen_output_tex", 1
        )[1].split("fn create_screenshot_output_tex", 1)[0]
        post_processing_source = POST_PROCESSING_SHADER.read_text(encoding="utf-8")

        self.assertIn(
            "format: vk::Format::R16G16B16A16_SFLOAT,",
            screen_output_factory,
        )
        self.assertNotIn("vk::Format::R8G8B8A8_UNORM", screen_output_factory)
        self.assertIn('[[vk::image_format("rgba16f")]]', post_processing_source)

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
        self.assertIn(
            "return (noise - 0.5) * (2.0 * strengthLsb * OUTPUT_QUANTIZATION_STEP);",
            dither_source,
        )
        self.assertIn("float dither_strength_lsb;", post_processing_source)
        self.assertIn(
            "float ditherOffset = getDitherOffset(uvi, post_processing_info.dither_strength_lsb);",
            post_processing_source,
        )
        self.assertIn("float3 encodedColor = linearToSrgb(finalColor);", post_processing_source)
        self.assertIn(
            "encodedColor = max(encodedColor + ditherOffset.xxx, float3(0.0));",
            post_processing_source,
        )
        self.assertIn("finalColor = srgbToLinear(encodedColor);", post_processing_source)

    def test_default_strength_is_centered_and_bounded_to_declared_lsb_radius(self) -> None:
        default_strength_lsb = self._dither_parameter()["data"]["value"]
        offsets_lsb = []
        for y in range(162):
            for x in range(288):
                hash_value = (
                    x * 0x9E3779B9 + y * 0x85EBCA6B + 0x12345678
                ) & 0xFFFFFFFF
                hash_value ^= hash_value >> 16
                hash_value = (hash_value * 0x7FEB352D) & 0xFFFFFFFF
                hash_value ^= hash_value >> 15
                noise = (hash_value & 0xFFFF) / 65535.0
                offsets_lsb.append((noise - 0.5) * 2.0 * default_strength_lsb)

        self.assertLess(abs(math.fsum(offsets_lsb) / len(offsets_lsb)), 0.001)
        self.assertGreaterEqual(min(offsets_lsb), -default_strength_lsb)
        self.assertLessEqual(max(offsets_lsb), default_strength_lsb)

    def test_gui_exposes_one_bounded_dither_strength_parameter(self) -> None:
        parameter = self._dither_parameter()

        self.assertEqual(parameter["kind"], "float")
        self.assertEqual(parameter["label"], "Dither Strength (Max 8-bit LSB)")
        self.assertEqual(parameter["data"], {"value": 1, "min": 0, "max": 4})

    def _dither_parameter(self) -> dict:
        with GUI_CONFIG.open("rb") as config_file:
            config = tomllib.load(config_file)

        matches = [
            parameter
            for section in config["section"]
            for parameter in section.get("param", [])
            if parameter["id"] == "dither_strength_lsb"
        ]
        self.assertEqual(len(matches), 1)
        return matches[0]


if __name__ == "__main__":
    unittest.main()
