"""Builds the client runtime into launcher/bundle/client.zip: from a working game install, or (--upstream) from the
pinned upstream releases in config/tools.lock (Goldberg, UE4SS — `cargo xtask setup --apply --tool=goldberg,ue4ss`)
plus cacert.pem of your game reference. 19.11: --upstream gives the same steam_api64.dll, UE4SS.dll, dwmapi.dll and
mod scripts as the shipped bundle (sha256); cacert.pem is the game's own, add your backend CA with --ca.

The set is embedded in the launcher exe (src-tauri/src/bundle.rs) and laid out in the game folder before launch. Since 04.11 it is
only the runtime shared by all servers: the UE4SS core (dwmapi.dll, UE4SS.dll, settings) with the built-in mods
(BPModLoaderMod, BPML_GenericFunctions, Keybinds, shared without typings), Goldberg steam_api64.dll + steam_appid.txt,
cacert.pem with the backend CA. The server's paks and Lua mods are dbmod/1 packages (tools/make_mods.py); the launcher installs them for the game session.

Everything that earlier versions of the set put into the game and this one no longer does goes into `retired` (path -> sha256 of those versions):
the launcher deletes such a file only if it is unchanged (sha256 matches) — the player's own files are not touched.

    python tools/make_client_bundle.py [--src <client install>] [--version 2026.11.03]
    python tools/make_client_bundle.py --upstream [--ca <backend CA .pem>]
"""
import argparse, hashlib, io, json, re, sys, time, zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE.parents[1] / 'tools' / 'py'))
import wspaths  # noqa: E402


def default_client():
    """Client to build from — config/local.toml [games] client (tools/py/wspaths.py); none — None (--src needed)."""
    v = wspaths.local('games', 'client')
    return str(wspaths.resolve(v)) if v else None


def game_appid() -> str:
    """Steam app of the game — config/game.toml, the first [[build]] app."""
    m = re.search(r'(?m)^app\s*=\s*"(\d+)"', (wspaths.ROOT / 'config' / 'game.toml').read_text(encoding='utf-8'))
    if not m:
        sys.exit('config/game.toml: no app')
    return m.group(1)


def reference_win64() -> Path:
    """Win64 of the newest game reference (local.toml [game] original, else home/game/original/<build>)."""
    v = wspaths.local('game', 'original')
    root = wspaths.resolve(v) if v else wspaths.HOME / 'game' / 'original'
    builds = sorted((d for d in root.glob('*') if (d / 'Dungeonborne' / 'Binaries' / 'Win64' / 'cacert.pem').is_file()),
                    key=lambda d: d.name)
    if not builds:
        sys.exit(f'{root}: no game reference (docs/GAME_DATA.md: cargo ws game original ...)')
    return builds[-1] / 'Dungeonborne' / 'Binaries' / 'Win64'
OUT = HERE / 'bundle' / 'client.zip'

# Built-in UE4SS mods (runtime); the server's mods come as dbmod/1 packages
RUNTIME_MODS = ['BPModLoaderMod', 'BPML_GenericFunctions', 'Keybinds', 'shared']
# typings for the IDE — players do not need them
SKIP_MOD_PATH = re.compile(r'^Mods/shared/types/', re.I)
MODS_TXT = 'BPML_GenericFunctions : 1\nBPModLoaderMod : 1\n\n; Built-in keybinds, do not move up!\nKeybinds : 1\n'
MODS_JSON = [{'mod_name': m, 'mod_enabled': True} for m in ('BPML_GenericFunctions', 'BPModLoaderMod', 'Keybinds')]
# In Win64 only these files are ours (the rest is the original game or junk)
WIN64_FILES = ['dwmapi.dll', 'steam_api64.dll', 'steam_appid.txt', 'cacert.pem']
UE4SS_FILES = ['UE4SS.dll', 'UE4SS-settings.ini', 'LICENSE']
# Debug and temporary files
JUNK = re.compile(r'(\.bak[^/]*$|\.bak_[^/]*$|\.log$|\.dmp$|/mods\.txt\.bak)', re.I)

# UE4SS settings of the bundle: the external console and the GUI console off; the UObject array cache on (the shipped
# bundle has it; this UE4SS build defaults to false — found 19.11 comparing --upstream with the shipped bundle)
SETTINGS_INI = {'ConsoleEnabled': '0', 'GuiConsoleEnabled': '0', 'GuiConsoleVisible': '0',
                'bUseUObjectArrayCache': 'true'}


def ini_consoles_off(data: bytes) -> bytes:
    text = data.decode('utf-8-sig')
    for k, v in SETTINGS_INI.items():
        # [^\r\n]*: keep the file's CRLF (`.*$` ate the \r of the changed lines)
        text, n = re.subn(rf'(?m)^{k}[ \t]*=[^\r\n]*', f'{k} = {v}', text)
        assert n == 1, f'UE4SS-settings.ini: {k} occurs {n} times'
    return text.encode('utf-8')


PATCH = {
    'Dungeonborne/Binaries/Win64/ue4ss/UE4SS-settings.ini': ini_consoles_off,
}


def collect(src: Path) -> dict:
    """From a working install: rel path (from the install root, with /) -> bytes"""
    win64 = src / 'Dungeonborne' / 'Binaries' / 'Win64'
    return collect_from({n: (win64 / n).read_bytes() for n in WIN64_FILES}, win64 / 'ue4ss')


def collect_upstream(ca: Path | None) -> dict:
    """From the tools.lock releases (home/tools/goldberg, home/tools/ue4ss) and the game reference's cacert.pem."""
    gb, ue = wspaths.HOME / 'tools' / 'goldberg', wspaths.HOME / 'tools' / 'ue4ss'
    for f in (gb / 'steam_api64.dll', ue / 'dwmapi.dll', ue / 'ue4ss' / 'UE4SS.dll'):
        if not f.is_file():
            sys.exit(f'{f}: missing - cargo xtask setup --apply --tool=goldberg,ue4ss')
    cacert = (reference_win64() / 'cacert.pem').read_bytes()
    if ca:
        pem = ca.read_bytes()
        if b'-----BEGIN CERTIFICATE-----' not in pem:
            sys.exit(f'{ca}: not a PEM certificate')
        cacert = cacert.rstrip(b'\r\n') + b'\n\n# backend CA\n' + pem.strip() + b'\n'
    win64 = {
        'dwmapi.dll': (ue / 'dwmapi.dll').read_bytes(),
        'steam_api64.dll': (gb / 'steam_api64.dll').read_bytes(),
        'steam_appid.txt': game_appid().encode('ascii'),
        'cacert.pem': cacert,
    }
    return collect_from(win64, ue / 'ue4ss')


def collect_from(win64_files: dict, ue4ss: Path):
    files, dirs = {}, []
    for n in WIN64_FILES:
        files[f'Dungeonborne/Binaries/Win64/{n}'] = win64_files[n]
    for n in UE4SS_FILES:
        files[f'Dungeonborne/Binaries/Win64/ue4ss/{n}'] = (ue4ss / n).read_bytes()
    for mod in RUNTIME_MODS:
        for f in sorted((ue4ss / 'Mods' / mod).rglob('*')):
            inner = f.relative_to(ue4ss).as_posix()
            rel = f'Dungeonborne/Binaries/Win64/ue4ss/{inner}'
            if f.is_file() and not JUNK.search(rel) and not SKIP_MOD_PATH.search(inner):
                files[rel] = f.read_bytes()
    files['Dungeonborne/Binaries/Win64/ue4ss/Mods/mods.txt'] = MODS_TXT.encode('utf-8')
    files['Dungeonborne/Binaries/Win64/ue4ss/Mods/mods.json'] = json.dumps(MODS_JSON, indent=4).encode('utf-8')
    for rel, fn in PATCH.items():
        files[rel] = fn(files[rel])
    return files, dirs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--src', default=default_client(),
                    help='game install (default: local.toml [games] client)')
    ap.add_argument('--upstream', action='store_true',
                    help='from the tools.lock releases (goldberg, ue4ss) and the game reference, not from an install')
    ap.add_argument('--ca', type=Path, help='with --upstream: PEM of your backend CA, appended to cacert.pem')
    ap.add_argument('--version', default=time.strftime('%Y.%m.%d.%H%M'))
    a = ap.parse_args()
    if a.upstream:
        files, dirs = collect_upstream(a.ca)
        source = 'upstream: tools.lock goldberg + ue4ss, cacert.pem of the game reference'
    else:
        if a.ca:
            sys.exit('--ca is for --upstream (an install already has its cacert.pem)')
        if not a.src:
            sys.exit('no --src and no [games] client in config/local.toml (or --upstream)')
        src = Path(a.src)
        if not (src / 'Dungeonborne' / 'Binaries' / 'Win64' / 'Dungeonborne.exe').is_file():
            sys.exit(f'{src}: not a game install')
        files, dirs = collect(src)
        source = str(src)
    # retired: files of earlier set versions that are not in this one (sha256 of all their versions)
    retired = {}
    if OUT.is_file():
        with zipfile.ZipFile(OUT) as old:
            prev = json.loads(old.read('bundle.json'))
        for rel, shas in prev.get('retired', {}).items():
            retired[rel] = list(shas)
        for rel, sha in prev.get('files', {}).items():
            retired.setdefault(rel, [])
            if sha not in retired[rel]:
                retired[rel].append(sha)
    new_hashes = {rel: hashlib.sha256(b).hexdigest() for rel, b in files.items()}
    # what is in the set again is not deleted; but old versions of such a file are "ours" too (they get overwritten)
    retired = {rel: sorted(shas) for rel, shas in sorted(retired.items()) if rel not in new_hashes and shas}
    meta = {
        'schema': 'dblbundle/1',
        'version': a.version,
        'source': source,
        'files': dict(sorted(new_hashes.items())),
        'dirs': dirs,
        'retired': retired,
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        z.writestr('bundle.json', json.dumps(meta, indent=1, ensure_ascii=False))
        for rel, b in sorted(files.items()):
            z.writestr(rel, b)
    OUT.write_bytes(buf.getvalue())
    total = sum(len(b) for b in files.values())
    print(f'{OUT}: {len(files)} files, {total / 1e6:.1f} MB -> zip {OUT.stat().st_size / 1e6:.1f} MB, version {a.version}, '
          f'retired {len(retired)}')


if __name__ == '__main__':
    main()
