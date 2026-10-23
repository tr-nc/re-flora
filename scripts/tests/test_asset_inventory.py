from __future__ import annotations

import csv
import hashlib
import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import asset_inventory as ledger


def group(pattern: str = "assets/*", name: str = "fixture") -> ledger.Group:
    return {
        "id": name,
        "name": name,
        "patterns": [pattern],
        "exclude": [],
        "usage": "运行时加载",
        "decision": "待许可确认",
        "source": "测试来源",
        "gap": "待补",
        "evidence": [],
    }


class AssetInventoryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def write(self, path: str, data: bytes = b"fixture") -> None:
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)

    def test_scope_includes_sidecars_and_embedded_media_but_not_plain_code(
        self,
    ) -> None:
        self.write("docs/font.html", b"url(data:font/ttf;base64,AA==)")
        self.write("docs/plain.html", b"<p>plain text</p>")
        cases = {
            "assets/models/recipe.mjs": "asset-source/data",
            "assets/models/model.json": "asset-source/data",
            "assets/sfx/provenance.json": "bundle-note",
            "assets/font/OFL.txt": "bundle-note",
            "assets/sfx/SHA256SUMS": "bundle-note",
            "docs/evidence/view.PNG": "png",
            "docs/paper.pdf": "pdf",
            "experiments/source.blend": "blend",
            "crates/audio/table.petalhrtf": "petalhrtf",
            "docs/font.html": "embedded-media-container",
            "docs/plain.html": None,
            "src/renderer.rs": None,
            "config/gui.toml": None,
            "docs/README.md": None,
        }
        for path, expected in cases.items():
            with self.subTest(path=path):
                self.assertEqual(ledger.asset_kind(self.root, path), expected)

    def test_csv_roundtrip_hashes_duplicates_notes_and_packaging_are_distinct(
        self,
    ) -> None:
        sound = 'assets/sfx/叶片, "soft".wav'
        paths = [sound, "assets/sfx/README.md", "docs/reference.wav"]
        for path in paths:
            self.write(path, b"same content")
        rows = ledger.inventory(
            self.root, paths + [sound], [group(), group("docs/*", "docs")]
        )
        self.assertEqual(len(rows), 3)
        self.assertEqual(rows, list(csv.DictReader(io.StringIO(ledger.csv_text(rows)))))
        by_path = {row["path"]: row for row in rows}
        self.assertEqual(
            by_path[sound]["sha256"], hashlib.sha256(b"same content").hexdigest()
        )
        self.assertEqual(by_path[sound]["bytes"], "12")
        self.assertEqual(by_path[sound]["packaged_loose"], "yes")
        self.assertEqual(by_path["docs/reference.wav"]["packaged_loose"], "no")
        self.assertEqual(
            by_path["assets/sfx/README.md"]["usage"], "随包来源/许可说明，不加载"
        )
        # Equal bytes at different paths remain separate; one duplicate path is counted once.
        self.assertEqual(len({row["sha256"] for row in rows}), 1)

    def test_overlapping_rules_fail_and_exclusion_resolves_the_ambiguity(self) -> None:
        broad = group("assets/*", "broad")
        narrow = group("assets/font/*", "font")
        with self.assertRaisesRegex(ValueError, "Overlapping"):
            ledger.matching_group("assets/font/font.ttf", [broad, narrow])
        broad["exclude"] = ["assets/font/*"]
        self.assertEqual(
            ledger.matching_group("assets/font/font.ttf", [broad, narrow]), narrow
        )
        self.assertIsNone(
            ledger.matching_group("docs/unclassified.wav", [broad, narrow])
        )

    def test_git_discovery_excludes_untracked_and_ignored_files(self) -> None:
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        self.write(".gitignore", b"target/\n")
        self.write("assets/tracked space.PNG")
        self.write("assets/untracked.wav")
        self.write("target/capture.png")
        subprocess.run(
            ["git", "add", ".gitignore", "assets/tracked space.PNG"],
            cwd=self.root,
            check=True,
        )
        paths = ledger.tracked_paths(self.root)
        self.assertEqual(paths, [".gitignore", "assets/tracked space.PNG"])
        self.assertEqual(len(ledger.inventory(self.root, paths, [group()])), 1)

    def test_check_detects_stale_bytes_without_writing_and_unknown_assets_fail(
        self,
    ) -> None:
        path = "assets/sfx/one.wav"
        self.write(path)
        catalog: ledger.Catalog = {
            "reviewed_revision": "fixture-revision",
            "reviewed_on": "2026-10-23",
            "groups": [group()],
        }
        self.write(str(ledger.CATALOG), json.dumps(catalog).encode())
        with (
            patch.object(ledger, "ROOT", self.root),
            patch.object(ledger, "tracked_paths", return_value=[path]),
        ):
            with patch.object(sys, "argv", ["asset_inventory.py"]):
                self.assertEqual(ledger.main(), 0)
            csv_path = self.root / "docs/assets/inventory.csv"
            original = csv_path.read_bytes()
            with patch.object(sys, "argv", ["asset_inventory.py", "--check"]):
                self.assertEqual(ledger.main(), 0)
                self.write(path, b"changed")
                self.assertEqual(ledger.main(), 1)
                self.assertEqual(csv_path.read_bytes(), original)
            self.write("docs/unknown.wav")
            with (
                patch.object(
                    ledger, "tracked_paths", return_value=[path, "docs/unknown.wav"]
                ),
                patch.object(sys, "argv", ["asset_inventory.py"]),
            ):
                self.assertEqual(ledger.main(), 1)
                self.assertIn("UNREVIEWED", csv_path.read_text())

    def test_markdown_report_escapes_pipes_and_links_space_containing_evidence(
        self,
    ) -> None:
        item = group()
        item["source"] = "a | b"
        item["evidence"] = ["assets/sfx/with spaces/README.md"]
        catalog: ledger.Catalog = {
            "reviewed_revision": "fixture",
            "reviewed_on": "2026-10-23",
            "groups": [item],
        }
        output = ledger.report([], catalog)
        self.assertIn("a \\| b", output)
        self.assertIn("assets/sfx/with%20spaces/README.md", output)


if __name__ == "__main__":
    unittest.main()
