"""Signed launcher releases for self-update.

The launcher accepts an update only if `launcher.json` is signed by the release key whose public half is built into it
(`src-tauri/src/builtin.rs`, `RELEASE_KEY`). Any server may host the update; only a holder of the secret key can make
one. Keep the secret key off the repository and off servers.

    python tools/release.py keygen --out <folder outside the repository>
        -> release_key.secret (32-byte seed, hex) + prints the public key for builtin.rs
    python tools/release.py sign --key <that folder>\\release_key.secret
        [--exe target/release/dungeonborne-launcher.exe] [--version <from Cargo.toml>] [--out dist/launcher]
        -> dist/launcher/dungeonborne-launcher-<version>.exe + launcher.json (copy both to the server's [launcher] dir)

Signed message (UTF-8): "dbl-update/1\\n<version>\\n<sha256 hex>\\n<size>"
"""
import argparse, hashlib, json, re, shutil, sys
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives import serialization

HERE = Path(__file__).resolve().parent.parent


def message(version: str, sha256: str, size: int) -> bytes:
    return f'dbl-update/1\n{version}\n{sha256}\n{size}'.encode()


def public_hex(key: Ed25519PrivateKey) -> str:
    return key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw).hex()


def keygen(out: Path):
    out.mkdir(parents=True, exist_ok=True)
    path = out / 'release_key.secret'
    if path.exists():
        sys.exit(f'{path} already exists — not overwriting (old launchers know only its public key)')
    key = Ed25519PrivateKey.generate()
    seed = key.private_bytes(serialization.Encoding.Raw, serialization.PrivateFormat.Raw, serialization.NoEncryption())
    path.write_text(seed.hex() + '\n', encoding='ascii')
    print(f'secret: {path}')
    print(f'public: {public_hex(key)}   -> src-tauri/src/builtin.rs RELEASE_KEY')


def cargo_version() -> str:
    text = (HERE / 'src-tauri' / 'Cargo.toml').read_text(encoding='utf-8')
    return re.search(r'(?m)^version\s*=\s*"([^"]+)"', text).group(1)


def sign(key_path: Path, exe: Path, version: str, out: Path):
    seed = bytes.fromhex(key_path.read_text(encoding='ascii').strip())
    key = Ed25519PrivateKey.from_private_bytes(seed)
    data = exe.read_bytes()
    sha = hashlib.sha256(data).hexdigest()
    out.mkdir(parents=True, exist_ok=True)
    name = f'dungeonborne-launcher-{version}.exe'
    shutil.copyfile(exe, out / name)
    sig = key.sign(message(version, sha, len(data))).hex()
    meta = {'schema': 'dbllauncher/1', 'version': version, 'file': name, 'size': len(data), 'sha256': sha,
            'signature': sig, 'public_key': public_hex(key)}
    (out / 'launcher.json').write_text(json.dumps(meta, indent=1), encoding='utf-8')
    for old in out.glob('dungeonborne-launcher-*.exe'):
        if old.name != name:
            old.unlink()
    print(f'{out / name}: {len(data) / 1e6:.1f} MB, sha256 {sha[:16]}…, version {version}')
    print(f'public key {meta["public_key"]} — must equal RELEASE_KEY of the launchers that should take it')


def main():
    ap = argparse.ArgumentParser()
    sp = ap.add_subparsers(dest='cmd', required=True)
    k = sp.add_parser('keygen')
    k.add_argument('--out', required=True)
    s = sp.add_parser('sign')
    s.add_argument('--key', required=True)
    s.add_argument('--exe', default=str(HERE / 'target' / 'release' / 'dungeonborne-launcher.exe'))
    s.add_argument('--version', default=None)
    s.add_argument('--out', default=str(HERE / 'dist' / 'launcher'))
    a = ap.parse_args()
    if a.cmd == 'keygen':
        keygen(Path(a.out))
    else:
        sign(Path(a.key), Path(a.exe), a.version or cargo_version(), Path(a.out))


if __name__ == '__main__':
    main()
