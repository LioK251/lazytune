"""Build the isolated native player check using this workspace's existing dependencies."""
import json
import subprocess
import sys
from pathlib import Path


def build():
    root = Path(__file__).resolve().parent.parent
    result = subprocess.run(
        ['cargo', 'build', '-p', 'lazytune', '--locked', '--message-format=json'],
        cwd=root, capture_output=True, text=True, encoding='utf-8',
    )
    if result.returncode:
        raise SystemExit('cargo build failed; run cargo build -p lazytune for diagnostics')
    crates, native_paths = {}, set()
    for line in result.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get('reason') == 'compiler-artifact':
            name = message['target']['name']
            if name in ('wry', 'tao', 'serde_json'):
                libraries = [Path(p) for p in message['filenames'] if p.endswith('.rlib')]
                if libraries:
                    crates[name] = libraries[0]
        elif message.get('reason') == 'build-script-executed':
            native_paths.update(message.get('linked_paths', []))
    missing = {'wry', 'tao', 'serde_json'} - crates.keys()
    if missing:
        raise SystemExit(f'cargo did not report native dependencies: {sorted(missing)}')
    output = root / '.tools/native-player-test'
    output.mkdir(parents=True, exist_ok=True)
    binary = output / ('native_player_test.exe' if sys.platform == 'win32' else 'native_player_test')
    command = ['rustc', str(root / 'scripts/native_player_test.rs'), '--edition=2021',
               '-C', 'opt-level=0', '-o', str(binary)]
    for name, library in crates.items():
        command.extend(['--extern', f'{name}={library}'])
    for directory in sorted({p.parent for p in crates.values()}):
        command.extend(['-L', f'dependency={directory}'])
    for path in sorted(native_paths):
        command.extend(['-L', path])
    subprocess.run(command, cwd=root, check=True)
    print(binary)


if __name__ == '__main__':
    build()
