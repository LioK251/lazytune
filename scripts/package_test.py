"""The release allowlist must not leak profiles or credentials."""
import hashlib
import tempfile
import unittest
from pathlib import Path
from package import package


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for folder in ['desktop/icons', 'target/release/bundle']:
            (self.root / folder).mkdir(parents=True)
        for path, text in {
            'Cargo.toml': '[workspace.package]\nversion="0.1.0"',
            'desktop/tauri.conf.json': '{"version":"0.1.0"}',
            'desktop/icons/lazytune-1024.png': 'icon',
            'LICENSE': 'license', 'THIRD_PARTY_NOTICES.md': 'notices',
            '.env_lastfm': 'private', 'target/release/profile.json': 'private',
        }.items():
            (self.root / path).write_text(text)

    def test_all_platforms_and_checksums(self):
        for system, machine, platform_name, extensions in [
            ('Windows', 'AMD64', 'windows-x64', ['.exe']),
            ('Darwin', 'arm64', 'macos-arm64', ['.dmg']),
            ('Darwin', 'x86_64', 'macos-x64', ['.dmg']),
            ('Linux', 'x86_64', 'linux-x64', ['.deb', '.rpm', '.AppImage']),
        ]:
            with self.subTest(system=system, machine=machine):
                for extension in extensions:
                    folder = 'target/release' if extension == '.exe' else 'target/release/bundle'
                    (self.root / folder / f'lazytune{extension}').write_bytes(b'app')
                out = package(self.root, system, machine)
                names = {f'lazytune-0.1.0-{platform_name}{ext}' for ext in extensions}
                names |= {'LICENSE', 'THIRD_PARTY_NOTICES.md', 'lazytune-1024.png', f'SHA256SUMS-{platform_name}.txt'}
                self.assertEqual({p.name for p in out.iterdir()}, names)
                for line in (out / f'SHA256SUMS-{platform_name}.txt').read_text().splitlines():
                    digest, name = line.split('  ')
                    self.assertEqual(hashlib.sha256((out / name).read_bytes()).hexdigest(), digest)

    def test_version_mismatch_and_missing_bundles_fail(self):
        with self.assertRaisesRegex(ValueError, 'release artifact'):
            package(self.root, 'Linux', 'x86_64')
        (self.root / 'desktop/tauri.conf.json').write_text('{"version":"0.2.0"}')
        with self.assertRaisesRegex(ValueError, 'versions must match'):
            package(self.root, 'Windows', 'AMD64')

    def test_unexpected_output_fails_without_deleting_it(self):
        (self.root / 'target/release/lazytune.exe').write_bytes(b'app')
        out = package(self.root, 'Windows', 'AMD64')
        secret = out / '.env_lastfm'
        secret.write_text('private')
        with self.assertRaisesRegex(ValueError, 'unexpected files'):
            package(self.root, 'Windows', 'AMD64')
        self.assertTrue(secret.exists())


if __name__ == '__main__':
    unittest.main()
