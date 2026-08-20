#!/usr/bin/env python3

import unittest

import release_tag


class ReleaseVersionTests(unittest.TestCase):
    def test_bump_patch_increments_only_patch(self) -> None:
        self.assertEqual(release_tag.bump_patch_version("1.2.3"), "1.2.4")

    def test_bump_minor_increments_minor_and_resets_patch(self) -> None:
        self.assertEqual(release_tag.bump_minor_version("1.2.9"), "1.3.0")

    def test_bump_minor_rejects_prerelease_version(self) -> None:
        with self.assertRaisesRegex(release_tag.ReleaseTagError, "plain X.Y.Z"):
            release_tag.bump_minor_version("1.2.3-rc.1")


if __name__ == "__main__":
    unittest.main()
