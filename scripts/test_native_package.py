#!/usr/bin/env python3
"""Verify packaged HAP contents with synthetic ELF headers and real metadata."""
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile

from verify_native import verify

ROOT = Path(__file__).resolve().parents[1]
ARM64 = ('arm64-v8a', 183)
X86_64 = ('x86_64', 62)


class NativePackageTests(unittest.TestCase):
    def package(self, path, libraries, pages=None):
        config = json.loads((ROOT / 'entry/src/main/module.json5').read_text())
        config['app'] = json.loads((ROOT / 'AppScope/app.json5').read_text())['app']
        with zipfile.ZipFile(path, 'w') as archive:
            archive.writestr('module.json', json.dumps(config))
            archive.writestr('resources/base/profile/main_pages.json',
                             json.dumps({'src': pages if pages is not None else
                                         ['pages/Index', 'pages/SubWindow']}))
            archive.writestr('resources/rawfile/arkdata/utd/utd.json5',
                             (ROOT / 'entry/src/main/resources/rawfile/arkdata/utd/utd.json5').read_text())
            for abi, machine in libraries:
                elf = bytearray(64)
                elf[:5] = b'\x7fELF\x02'
                struct.pack_into('<H', elf, 18, machine)
                archive.writestr(f'libs/{abi}/libphotocraft.so', elf)

    def test_both_abis_ship_by_default(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'native.hap'
            self.package(path, [ARM64, X86_64])
            verify(path)
            # The default is what dev.sh packages: one HAP for devices and the x86_64 emulator.
            with self.assertRaisesRegex(ValueError, 'missing libs/x86_64'):
                self.package(path, [ARM64])
                verify(path)

    def test_single_abi_package_passes_when_that_abi_is_expected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'native.hap'
            self.package(path, [ARM64])
            verify(path, abis=['arm64-v8a'])

    def test_missing_wrong_or_extra_architecture_fails(self):
        for libraries in [[], [('arm64-v8a', 62)], [X86_64],
                          [ARM64, ('armeabi-v7a', 40)]]:
            with self.subTest(libraries=libraries), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / 'native.hap'
                self.package(path, libraries)
                with self.assertRaises(ValueError):
                    verify(path)

    def test_extra_runtime_library_architecture_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'native.hap'
            self.package(path, [ARM64])
            with zipfile.ZipFile(path, 'a') as archive:
                archive.writestr('libs/x86_64/libc++_shared.so', b'foreign runtime')
            with self.assertRaisesRegex(ValueError, 'unexpected ABIs'):
                verify(path, abis=['arm64-v8a'])

    def test_expected_bundle_is_checked(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'native.hap'
            self.package(path, [ARM64])
            bundle = json.loads((ROOT / 'AppScope/app.json5').read_text())['app']['bundleName']
            verify(path, expected_bundle=bundle, abis=['arm64-v8a'])
            with self.assertRaisesRegex(ValueError, 'unexpected bundle'):
                verify(path, expected_bundle='invalid.bundle', abis=['arm64-v8a'])

    def test_unexpected_or_missing_native_page_fails(self):
        for pages in [['pages/Index'],
                      ['pages/Index', 'pages/SubWindow', 'pages/Web']]:
            with self.subTest(pages=pages), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / 'native.hap'
                self.package(path, [ARM64], pages=pages)
                with self.assertRaisesRegex(ValueError, 'non-native pages remain'):
                    verify(path, abis=['arm64-v8a'])


if __name__ == '__main__':
    unittest.main()
