"""The current butterfly authoring pipeline must not depend on retired experiments."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "experiments/butterfly-method-comparison/blender-v5"
spec = importlib.util.spec_from_file_location("butterfly_palette", SOURCE / "palette.py")
assert spec is not None and spec.loader is not None
palette = importlib.util.module_from_spec(spec)
spec.loader.exec_module(palette)


class ButterflyPaletteTests(unittest.TestCase):
    def test_current_runner_help_works_without_retired_versions_or_blender(
        self,
    ) -> None:
        result = subprocess.run(
            [sys.executable, str(SOURCE / "run.py"), "--help"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertIn("--rebuild", result.stdout)

    def test_current_atlases_keep_their_palette_and_decoded_pixels(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            for size in (12, 16):
                with Image.open(SOURCE / f"atlas-{size}-rgba.png") as original:
                    for name, colors in (
                        ("blue", palette.BLUE),
                        ("gray", palette.GRAY),
                    ):
                        source = original.convert("RGBA")
                        if name == "gray":
                            gray = source.convert("L")
                            source = Image.merge(
                                "RGBA", (gray, gray, gray, source.getchannel("A"))
                            )
                        output = Path(directory) / "indexed.png"
                        palette.save_indexed(palette.indexed(source, colors), output)
                        with (
                            Image.open(output) as actual,
                            Image.open(
                                SOURCE / f"atlas-{size}-{name}-indexed.png"
                            ) as expected,
                        ):
                            self.assertEqual(actual.mode, "P")
                            colors_read = actual.getpalette()
                            assert colors_read is not None
                            self.assertEqual(len(colors_read), 15)
                            self.assertEqual(
                                actual.convert("RGBA").tobytes(),
                                expected.convert("RGBA").tobytes(),
                            )

    def test_alpha_threshold_and_transparent_index_are_unchanged(self) -> None:
        source = Image.new("RGBA", (3, 1))
        source.putdata([(80, 190, 220, 127), (80, 190, 220, 128), (205, 246, 248, 255)])
        actual = palette.indexed(source, palette.BLUE)
        self.assertEqual(list(actual.getdata()), [0, 3, 4])
        self.assertEqual(actual.info["transparency"], 0)


if __name__ == "__main__":
    unittest.main()
