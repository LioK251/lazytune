"""Collect an explicit set of release assets, with platform names and checksums."""
import hashlib
import json
import platform
import shutil
import tomllib
from pathlib import Path


def package(root, system, machine):
    version = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']['package']['version']
    config = json.loads((root / 'desktop/tauri.conf.json').read_text())
    if version != config['version']:
        raise ValueError('Cargo and Tauri versions must match before packaging.')
    os_name = {'Windows': 'windows', 'Darwin': 'macos', 'Linux': 'linux'}[system]
    arch = {'amd64': 'x64', 'x86_64': 'x64', 'aarch64': 'arm64', 'arm64': 'arm64'}[machine.lower()]
    name = f'lazytune-{version}-{os_name}-{arch}'
    release = root / 'target/release'
    extensions = {'Windows': ['.exe'], 'Darwin': ['.dmg'], 'Linux': ['.deb', '.rpm', '.AppImage']}[system]
    files = {}
    for extension in extensions:
        matches = [release / 'lazytune.exe'] if extension == '.exe' else list((release / 'bundle').glob(f'**/*{extension}'))
        if len(matches) != 1 or not matches[0].is_file():
            raise ValueError(f'Expected exactly one {extension} release artifact; build the platform bundles first.')
        files[f'{name}{extension}'] = matches[0]
    for path in ['desktop/icons/lazytune-1024.png', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'artifacts/THIRD_PARTY_LICENSES.html']:
        source = root / path
        if not source.is_file():
            raise ValueError(f'Missing release file: {path}')
        files[source.name] = source
    checksum_name = f'SHA256SUMS-{os_name}-{arch}.txt'
    out = root / 'artifacts/package' / f'{os_name}-{arch}'
    out.mkdir(parents=True, exist_ok=True)
    if any(p.name not in {*files, checksum_name} or not p.is_file() for p in out.iterdir()):
        raise ValueError('Package directory contains unexpected files; use an empty output directory.')
    checksums = []
    for filename, source in sorted(files.items()):
        target = out / filename
        shutil.copy2(source, target)
        with target.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        checksums.append(f'{digest}  {filename}')
    (out / checksum_name).write_text('\n'.join(checksums) + '\n', encoding='utf-8')
    return out


if __name__ == '__main__':
    print(f'Packaged release files in {package(Path(__file__).resolve().parents[1], platform.system(), platform.machine())}')
