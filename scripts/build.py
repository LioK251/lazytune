"""Build with optional local service credentials without printing them."""
import os
import subprocess
import sys
from pathlib import Path

root = Path(__file__).resolve().parents[1]
env = os.environ.copy()
env.pop('NO_COLOR', None)
secrets = root / '.env_lastfm'
if secrets.exists():
    labels = {'api key': 'LAZYTUNE_LASTFM_KEY', 'shared secret': 'LAZYTUNE_LASTFM_SECRET'}
    for line in secrets.read_text(encoding='utf-8-sig').splitlines():
        parts = line.split('\t', 1)
        if len(parts) == 2 and parts[0].strip().lower() in labels:
            env[labels[parts[0].strip().lower()]] = parts[1].strip()
trunk = root / '.tools' / 'bin' / ('trunk.exe' if os.name == 'nt' else 'trunk')
release = '--release' if '--release' in sys.argv else None
if '--desktop-only' not in sys.argv:
    subprocess.run([str(trunk) if trunk.exists() else 'trunk', 'build', *([release] if release else [])], cwd=root, env=env, check=True)
subprocess.run(['cargo', 'build', '-p', 'lazytune', '--locked', *([release] if release else [])], cwd=root, env=env, check=True)
