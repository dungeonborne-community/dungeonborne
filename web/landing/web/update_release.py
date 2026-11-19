"""Put a new launcher build on the site: copies the exe into downloads/ and writes the version, size and SHA-256 into index.html.

    python update_release.py <launcher release folder, e.g. apps/launcher/dist/launcher-1.5.3>

Takes launcher.json from the release folder (made by launcher/tools/release.py) and checks the exe hash against it.
"""
import hashlib
import json
import re
import shutil
import sys
from pathlib import Path

SITE = Path(__file__).resolve().parent
TARGET = SITE / "downloads" / "DungeonborneLauncher.exe"


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    rel = Path(sys.argv[1])
    meta = json.loads((rel / "launcher.json").read_text(encoding="utf-8"))
    exe = rel / meta["file"]
    data = exe.read_bytes()
    sha = hashlib.sha256(data).hexdigest()
    if sha != meta["sha256"].lower() or len(data) != meta["size"]:
        sys.exit(f"{exe}: hash or size does not match launcher.json — not deploying")

    TARGET.parent.mkdir(exist_ok=True)
    shutil.copyfile(exe, TARGET)

    page = SITE / "index.html"
    html = page.read_bytes().decode("utf-8")  # no newline translation: the file stays LF
    mb = f"{len(data) / (1024 * 1024):.1f}".replace(".", ",")
    html = re.sub(r'(<span data-release="version">)[^<]*(</span>)', rf"\g<1>{meta['version']}\g<2>", html)
    html = re.sub(r'(<span data-release="size" data-bytes=")\d+(">)[^<]*(</span>)', rf"\g<1>{len(data)}\g<2>{mb}\g<3>", html)
    html = re.sub(r'(<span class="code" data-release="sha256">)[^<]*(</span>)', rf"\g<1>{sha.upper()}\g<2>", html)
    page.write_bytes(html.encode("utf-8"))
    print(f"{meta['version']}: {TARGET.name} {len(data)} bytes, sha256 {sha}")


if __name__ == "__main__":
    main()
