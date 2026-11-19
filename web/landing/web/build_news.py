"""Site news from the season's patch notes (the same file as the "Changelog" tab in the launcher).

    python build_news.py                      # seasons/<active>/patch_notes.toml -> news.js
    python build_news.py path/to/patch_notes.toml

Writes news.js: window.NEWS = [{version, date, ru: {title, new, fixed, balance}, en: {...}}], newest first, at most
MAX entries. English — from [update.lang.en]; whatever is missing there falls back to Russian (as in the launcher).
"""
import json
import sys
import tomllib
from pathlib import Path

SITE = Path(__file__).resolve().parent
REPO = SITE.parents[2] / "content"  # content/seasons workspace
MAX = 6
KINDS = ("new", "fixed", "balance")


def source() -> Path:
    if len(sys.argv) > 1:
        return Path(sys.argv[1])
    season = (REPO / "seasons" / "active.txt").read_text(encoding="utf-8").split()[0]
    return REPO / "seasons" / season / "patch_notes.toml"


def main() -> None:
    path = source()
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    out = []
    for u in data.get("update", [])[:MAX]:
        ru = {"title": u["title"], **{k: u.get(k, []) for k in KINDS}}
        en_src = u.get("lang", {}).get("en", {})
        en = {k: en_src.get(k, ru[k]) for k in ("title", *KINDS)}
        out.append({"version": u.get("version", u["date"]), "date": u["date"], "ru": ru, "en": en})
    js = (
        f"// Built by build_news.py from {path.relative_to(REPO).as_posix()} — do not edit by hand.\n"
        f"window.NEWS = {json.dumps(out, ensure_ascii=False, indent=1)};\n"
    )
    (SITE / "news.js").write_bytes(js.encode("utf-8"))
    print(f"news.js: {len(out)} entries from {path}")


if __name__ == "__main__":
    main()
