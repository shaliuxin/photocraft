#!/usr/bin/env python3
"""Check the packaged native ABIs and ensure the HAP contains no web runtime snapshot."""
import argparse
import json
import struct
import zipfile
from pathlib import Path

NATIVE_PAGES = ['pages/Index', 'pages/SubWindow']

# e_machine values from the ELF header. Keep in step with the triples in scripts/build_native.sh
# and the mapping in entry/src/main/cpp/CMakeLists.txt.
ELF_MACHINE = {'arm64-v8a': 183, 'x86_64': 62, 'armeabi-v7a': 40}
DEFAULT_ABIS = ['arm64-v8a', 'x86_64']


def verify_file_open(config: dict, utds: dict) -> None:
    bundle = config['app']['bundleName']
    ability = next(a for a in config['module']['abilities'] if a['name'] == 'EntryAbility')
    if not ability.get('exported') or ability.get('launchType') != 'singleton':
        raise ValueError('file opening requires an exported singleton EntryAbility')
    uris = [uri for skill in ability['skills']
            if 'ohos.want.action.viewData' in skill.get('actions', [])
            for uri in skill.get('uris', [])]
    if not uris or any(u.get('scheme') != 'file' or not u.get('type')
                       or u.get('linkFeature') != 'FileOpen' for u in uris):
        raise ValueError('file opening requires file/type/FileOpen declarations')
    types = {u['type'] for u in uris}
    required = {'general.png', 'general.jpeg', 'com.adobe.photoshop-image',
                *(bundle + '.' + name for name in ('pcraft', 'psb', 'webp', 'heif'))}
    if not required <= types:
        raise ValueError(f'missing file open types: {required - types}')
    declarations = utds['UniformDataTypeDeclarations']
    for declaration in declarations:
        type_id = declaration['TypeId']
        if not type_id.startswith(bundle + '.') or type_id not in types:
            raise ValueError(f'unregistered or incorrectly namespaced UTD: {type_id}')
    declared = {d['TypeId'] for d in declarations}
    if any(t.startswith(bundle + '.') and t not in declared for t in types):
        raise ValueError('custom file open type has no UTD declaration')
    extensions = {ext for d in declarations for ext in d['FilenameExtensions']}
    if not {'.pcraft', '.psb', '.psdt', '.webp', '.heic'} <= extensions:
        raise ValueError('missing custom file extensions')


def verify(path: Path, expected_bundle: str | None = None,
           abis: list[str] | None = None) -> None:
    expected = list(abis or DEFAULT_ABIS)
    unknown = [abi for abi in expected if abi not in ELF_MACHINE]
    if unknown:
        raise ValueError(f'unknown ABIs: {unknown}')
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        for abi in expected:
            name = f'libs/{abi}/libphotocraft.so'
            if name not in names:
                raise ValueError(f'missing {name}')
            data = archive.read(name)
            # 0x02 selects ELF64; every ABI here is 64-bit.
            if data[:5] != b'\x7fELF\x02' or struct.unpack_from('<H', data, 18)[0] != ELF_MACHINE[abi]:
                raise ValueError(f'incorrect ELF architecture: {name}')
        packaged = {name.split('/')[1] for name in names if name.startswith('libs/') and name.count('/') >= 2}
        extra = sorted(packaged - set(expected))
        if extra:
            raise ValueError(f'unexpected ABIs in HAP: {extra}')
        forbidden = [name for name in names if name.lower().endswith(('.wasm', '.html', '.js'))]
        if forbidden:
            raise ValueError(f'web assets remain: {forbidden}')
        pages = json.loads(archive.read('resources/base/profile/main_pages.json'))
        if pages.get('src') != NATIVE_PAGES:
            raise ValueError(f'non-native pages remain: {pages}')
        config = json.loads(archive.read('module.json'))
        if expected_bundle is not None and config['app']['bundleName'] != expected_bundle:
            raise ValueError(f"unexpected bundle: {config['app']['bundleName']}")
        verify_file_open(config,
                         json.loads(archive.read('resources/rawfile/arkdata/utd/utd.json5')))
    print(f'Native HAP verified: {path} (ABIs: {", ".join(expected)})')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('hap', type=Path)
    parser.add_argument('--expected-bundle')
    parser.add_argument('--abis', nargs='+', metavar='ABI',
                        help=f'ABIs the HAP must contain (default: {" ".join(DEFAULT_ABIS)})')
    args = parser.parse_args()
    verify(args.hap, args.expected_bundle, args.abis)
