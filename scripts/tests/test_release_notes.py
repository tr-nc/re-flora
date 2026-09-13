import argparse
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import package_release
import release_notes


def document(version="1.2.3"):
    return (f"# Re: Flora {version}\n\n## What's new\n"
            "- Fallen fruit now settles more steadily on the ground.\n\n"
            "## Known limitations\nThis is still an evolving prototype.\n\n"
            "## How to play\nDownload the package for your platform and extract it.\n")


class PlayerReleaseNotesTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("config", "user.name", "Release fixture")
        (self.root / "seed").write_text("fixture")
        self.git("add", "seed")
        self.git("commit", "-qm", "seed")

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, check=True, capture_output=True)

    def write_notes(self, text=None):
        path=self.root / "docs/releases/1.2.3.md"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text or document())
        return path

    def commit_notes(self):
        self.write_notes()
        self.git("add", "docs")
        self.git("commit", "-qm", "document player changes")

    def test_uncommitted_file_cannot_satisfy_release(self):
        self.write_notes()
        with self.assertRaisesRegex(release_notes.ReleaseNotesError, "missing committed player notes"):
            release_notes.committed_notes(self.root, "1.2.3")

    def test_worktree_edits_cannot_change_shipped_notes(self):
        self.commit_notes()
        self.write_notes(document().replace("steadily", "perfectly"))
        self.assertEqual(release_notes.committed_notes(self.root, "1.2.3"), document())

    def test_wrong_version_and_placeholder_are_rejected(self):
        for text in [document("1.2.4"),document().replace("Fallen fruit", "TODO"),document().replace("This is still an evolving prototype.", "")]:
            with self.subTest(text=text), self.assertRaises(release_notes.ReleaseNotesError):
                release_notes.validate_notes(text,"1.2.3")

    def test_unsafe_version_rejected_before_git_or_output(self):
        for version in ["../../x", "1.2.3\nother=bad", "$(echo bad)", ""]:
            with self.assertRaises(release_notes.ReleaseNotesError):
                release_notes.committed_notes(self.root,version)

    def test_each_platform_archive_contains_exact_committed_notes(self):
        self.commit_notes()
        for directory in package_release.PACKAGE_DIRS:
            (self.root/directory).mkdir()
        target=self.root/'target/release'
        target.mkdir(parents=True)
        (target/package_release.binary_name()).write_bytes(b'fixture binary, not executable')
        for channel in ['windows','macos','fedora']:
            args=argparse.Namespace(version='1.2.3',channel=channel,dist_dir='dist',target_dir='target')
            with patch.object(package_release,'repo_root',return_value=self.root), patch.object(package_release,'copy_macos_vulkan_runtime',return_value=[]), patch.object(package_release,'fix_unix_runtime_paths'), patch.object(package_release,'write_build_info'):
                archive=package_release.package(args)
            with zipfile.ZipFile(archive) as z:
                self.assertEqual(z.read(f're-flora-1.2.3-{channel}/RELEASE_NOTES.md').decode(),document())

    def test_missing_notes_do_not_delete_existing_staging_directory(self):
        stage=self.root/'dist/stage/re-flora-1.2.3-windows'
        stage.mkdir(parents=True)
        (stage/'keep').write_text('keep')
        args=argparse.Namespace(version='1.2.3',channel='windows',dist_dir='dist',target_dir='target')
        with patch.object(package_release,'repo_root',return_value=self.root), self.assertRaises(release_notes.ReleaseNotesError):
            package_release.package(args)
        self.assertTrue((stage/'keep').exists())


if __name__=='__main__':
    unittest.main()
