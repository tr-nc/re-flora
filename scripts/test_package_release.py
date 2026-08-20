#!/usr/bin/env python3

from __future__ import annotations

import struct
import tempfile
import unittest
from pathlib import Path

import package_release


def write_fake_pe(path: Path, machine: int) -> None:
    pe_offset = 0x80
    data = bytearray(pe_offset + 6)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, pe_offset)
    data[pe_offset : pe_offset + 4] = b"PE\0\0"
    struct.pack_into("<H", data, pe_offset + 4, machine)
    path.write_bytes(data)


class PackageRuntimeTests(unittest.TestCase):
    def test_windows_runtime_candidates_match_executable_architecture(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "re-flora.exe"
            x86_dll = root / "windows-x86" / "phonon.dll"
            x64_dll = root / "windows-x64" / "phonon.dll"
            x86_dll.parent.mkdir()
            x64_dll.parent.mkdir()
            write_fake_pe(executable, 0x8664)
            write_fake_pe(x86_dll, 0x014C)
            write_fake_pe(x64_dll, 0x8664)

            compatible = package_release.compatible_runtime_candidates(
                [x86_dll, x64_dll], executable, "windows", "Steam Audio"
            )

            self.assertEqual(compatible, [x64_dll])

    def test_windows_packaging_rejects_only_wrong_architecture_dlls(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "re-flora.exe"
            x86_dll = root / "phonon.dll"
            write_fake_pe(executable, 0x8664)
            write_fake_pe(x86_dll, 0x014C)

            with self.assertRaisesRegex(FileNotFoundError, "PE machine 0x8664"):
                package_release.compatible_runtime_candidates(
                    [x86_dll], executable, "windows", "Steam Audio"
                )


if __name__ == "__main__":
    unittest.main()
