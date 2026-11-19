"""Packs the UI Sandbox kit for designers: ui/ (current Windows 11 UI + sandbox + contract) and examples/<name>/ from UIs/.
python tools/pack_ui_sandbox.py  ->  dist/ui-sandbox/dungeonborne-ui-sandbox-<date>-<hash8>.zip (deterministic content)."""
import hashlib, io, time, zipfile
from pathlib import Path

root = Path(__file__).resolve().parent.parent
UI_FILES = ["index.html", "app.js", "i18n.js", "input.css", "styles.css", "api.d.ts", "README.md", "Trunk.toml", "dev/mock-tauri.js"]

files = {}
for rel in UI_FILES:
    files[f"ui/{rel}"] = (root / "ui" / rel).read_bytes()
for d in sorted(p for p in (root / "UIs").iterdir() if p.is_dir()):
    for rel in UI_FILES:
        if (d / rel).exists():
            files[f"examples/{d.name}/{rel}"] = (d / rel).read_bytes()
files["examples/README.md"] = (root / "UIs" / "README.md").read_bytes()

h = hashlib.sha256()
for k in sorted(files):
    h.update(k.encode())
    h.update(files[k])
name = f"dungeonborne-ui-sandbox-{time.strftime('%Y%m%d')}-{h.hexdigest()[:8]}"
out = root / "dist" / "ui-sandbox"
out.mkdir(parents=True, exist_ok=True)
for old in out.glob("*.zip"):
    old.unlink()
buf = io.BytesIO()
with zipfile.ZipFile(buf, "w") as z:
    for k in sorted(files):
        z.writestr(zipfile.ZipInfo(f"{name}/{k}", (2026, 1, 1, 0, 0, 0)), files[k], compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
p = out / f"{name}.zip"
p.write_bytes(buf.getvalue())
print(p, f"{len(buf.getvalue()) / 1024:.0f} KB,", len(files), "files")
