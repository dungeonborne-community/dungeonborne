"""Builds client mod packages (dbmod/1) from a working game install, as described in tools/mods.json.

Each package is a deterministic zip (fixed timestamps, sorted entries): `mod.json` + `Paks/<file>` + `ue4ss/<Mod>/...`.
A package gets a new version only when its content changes; unchanged packages keep their version and zip, so the
launcher does not download them again. Output (default launcher/dist/mods):
  <id>-<version>.zip      packages the server hands out
  index.json              state of the last build (versions, hashes) — keep it next to the zips
  manifest_mods.json      ready `mods[]` for the server manifest (url is root-relative: /mods/<file>)

    python -I tools/make_mods.py [--src <client install>] [--out dist\\mods] [--spec tools\\mods.json] [--exe <Dungeonborne.exe>]

07.11: the source is the build from the backups repository (`cargo xtask mods build --profile=client` -> backups\\out\\mods\\client,
laid out as in an install, docs/MODS_DESIGN.md §11). It has no game exe: the hash is taken from --exe (default — the exe
in --src, as before for a working install).
    python -I tools/make_mods.py --src ..\\backups\\out\\mods\\client --exe <client install>\\Dungeonborne\\Binaries\\Win64\\Dungeonborne.exe
"""
import argparse, hashlib, io, json, re, sys, time, zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent


def default_client():
    """Client to build from — config/local.toml [games] client (tools/py/wspaths.py); none — None (--src needed)."""
    sys.path.insert(0, str(HERE.parents[1] / 'tools' / 'py'))
    import wspaths
    v = wspaths.local('games', 'client')
    return str(wspaths.resolve(v)) if v else None
PAK_EXTS = ('pak', 'ucas', 'utoc', 'sig')
# Logs, dumps, backups; enabled.txt is written by the launcher itself
JUNK = re.compile(r'(\.bak[^/]*$|\.bak_[^/]*$|\.log$|\.dmp$|(^|/)enabled\.txt$)', re.I)
ZIP_TIME = (1980, 1, 1, 0, 0, 0)


def sha256(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def collect(src: Path, spec: dict) -> dict:
    """package-relative path -> bytes"""
    win64 = src / 'Dungeonborne' / 'Binaries' / 'Win64'
    paks = src / 'Dungeonborne' / 'Content' / 'Paks'
    files = {}
    for stem in spec.get('paks', []):
        found = [e for e in PAK_EXTS if (paks / f'{stem}.{e}').is_file()]
        if 'pak' not in found:
            sys.exit(f'{paks / stem}.pak: no file')
        for e in found:
            files[f'Paks/{stem}.{e}'] = (paks / f'{stem}.{e}').read_bytes()
    for mod in spec.get('ue4ss', []):
        root = win64 / 'ue4ss' / 'Mods' / mod
        if not root.is_dir():
            sys.exit(f'{root}: no mod folder')
        for f in sorted(root.rglob('*')):
            rel = f'ue4ss/{mod}/{f.relative_to(root).as_posix()}'
            if f.is_file() and not JUNK.search(rel):
                files[rel] = f.read_bytes()
    return files


def mod_json(pid: str, spec: dict, files: dict, exe_sha256: str, version: str) -> dict:
    components = [{'type': 'pak', 'path': f'Paks/{s}.pak', 'priority': spec.get('priority', 0)} for s in spec.get('paks', [])]
    components += [{'type': 'ue4ss', 'path': f'ue4ss/{m}'} for m in spec.get('ue4ss', [])]
    native = any(p.lower().endswith('.dll') for p in files)
    return {
        'schema': 'dbmod/1',
        'id': pid,
        'version': version,
        'name': spec['name'],
        'side': 'client',
        'game': {'exe_sha256': [exe_sha256]},
        'components': components,
        'depends': spec.get('depends', []),
        'conflicts': spec.get('conflicts', []),
        'risk': {'native_code': native, 'network': False},
        'files': {p: sha256(b) for p, b in sorted(files.items())},
    }


def content_key(meta: dict) -> str:
    m = dict(meta)
    m.pop('version')
    return sha256(json.dumps(m, sort_keys=True).encode())


def build_zip(meta: dict, files: dict) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, 'w') as z:
        entries = [('mod.json', json.dumps(meta, indent=1, ensure_ascii=False).encode('utf-8'))]
        entries += sorted(files.items())
        for name, data in entries:
            info = zipfile.ZipInfo(name, ZIP_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            z.writestr(info, data, compresslevel=9)
    return buf.getvalue()


def new_version(prev: str | None) -> str:
    t = time.localtime()
    v = f'{t.tm_year}.{t.tm_mon * 100 + t.tm_mday}.{t.tm_hour * 100 + t.tm_min}'
    if v == prev:  # rebuilt within the same minute
        major, minor, patch = (int(x) for x in v.split('.'))
        v = f'{major}.{minor}.{patch + 1}'
    return v


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--src', default=default_client(),
                    help='game install (default: local.toml [games] client)')
    ap.add_argument('--out', default=str(HERE / 'dist' / 'mods'))
    ap.add_argument('--spec', default=str(HERE / 'tools' / 'mods.json'))
    ap.add_argument('--exe', default=None, help='Dungeonborne.exe for game.exe_sha256 (default — the one in --src)')
    a = ap.parse_args()
    if not a.src:
        sys.exit('no --src and no [games] client in config/local.toml')
    src, out = Path(a.src), Path(a.out)
    exe = Path(a.exe) if a.exe else src / 'Dungeonborne' / 'Binaries' / 'Win64' / 'Dungeonborne.exe'
    if not exe.is_file():
        sys.exit(f'{exe}: no game exe (--exe)')
    exe_sha256 = sha256(exe.read_bytes())
    spec = json.loads(Path(a.spec).read_text(encoding='utf-8'))['packages']
    out.mkdir(parents=True, exist_ok=True)
    index_path = out / 'index.json'
    old = json.loads(index_path.read_text(encoding='utf-8'))['packages'] if index_path.is_file() else {}

    index, manifest = {}, []
    for pid, ps in sorted(spec.items()):
        files = collect(src, ps)
        probe = mod_json(pid, ps, files, exe_sha256, '0.0.0')
        key = content_key(probe)
        prev = old.get(pid)
        if prev and prev['content_key'] == key and (out / prev['file']).is_file():
            entry, state = prev, 'same'
        else:
            version = new_version(prev and prev['version'])
            meta = mod_json(pid, ps, files, exe_sha256, version)
            data = build_zip(meta, files)
            name = f'{pid}-{version}.zip'
            (out / name).write_bytes(data)
            entry = {'version': version, 'file': name, 'sha256': sha256(data), 'size': len(data), 'content_key': key,
                     'native_code': meta['risk']['native_code']}
            state = 'NEW' if not prev else 'updated'
        entry = {**entry, 'required': ps['required'], 'name': ps['name']}
        index[pid] = entry
        manifest.append({'id': pid, 'version': entry['version'], 'sha256': entry['sha256'], 'size': entry['size'],
                         'required': ps['required'], 'url': f"/mods/{entry['file']}"})
        flags = ' native' if entry.get('native_code') else ''
        print(f'{state:8} {pid}@{entry["version"]}  {entry["size"] / 1024:7.1f} KB  {len(files)} files{flags}')

    keep = {e['file'] for e in index.values()}
    for z in out.glob('*.zip'):
        if z.name not in keep:
            z.unlink()
            print(f'removed  {z.name}')
    index_path.write_text(json.dumps({'schema': 'dbmodindex/1', 'source': str(src), 'exe_sha256': exe_sha256,
                                      'packages': index}, indent=1, ensure_ascii=False), encoding='utf-8')
    (out / 'manifest_mods.json').write_text(json.dumps(manifest, indent=1), encoding='utf-8')
    print(f'{out}: {len(index)} packages')


if __name__ == '__main__':
    main()
