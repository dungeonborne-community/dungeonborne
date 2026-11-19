"""Workspace paths for the python tools (docs/WORKSPACE_PLAN.md).

Before 14.11 the tools were run from the backups root and took data by paths like `balance/item_type.json`,
`seasons/<id>/...`, `ue4ss/...`. Now the data is split across members; W() maps the old path to the new one:

    W("balance/item_type.json")  -> <workspace>/content/balance/tables/item_type.json
    W("seasons") / season        -> <workspace>/content/seasons/<season>

ROOT — workspace root (the folder with config/workspace.toml), HOME — DB_HOME (the DB_HOME variable, else local.toml [home] path,
else ../home next to the workspace).
"""
import os
import re
from pathlib import Path

ROOT = next(p for p in Path(__file__).resolve().parents if (p / "config" / "workspace.toml").is_file())

# old path from the backups root -> path from the workspace root (long prefixes before short ones)
LAYOUT = [
    ("balance_csv", "content/balance/csv"),
    ("balance_kit", "content/balance/kit"),
    ("balance_pak", "game/paks/balance"),
    ("balance", "content/balance/tables"),
    ("seasons", "content/seasons"),
    ("loot", "content/loot"),
    ("names", "content/names"),
    ("match_checks.toml", "content/match_checks.toml"),
    ("mods/catalog.toml", "game/catalog/catalog.toml"),
    ("ue4ss", "game/ue4ss"),
    ("ExcelConfigjson", "core/backend/ExcelConfigjson"),
    ("ExcelConfig", "core/backend/ExcelConfig"),
    ("assets", "core/backend/assets"),
    ("rpc_dump", "core/backend/rpc_dump"),
    ("src", "core/backend/src"),
]


def W(old: str) -> Path:
    """Path from the backups root (as before 14.11) -> absolute path in the workspace."""
    s = str(old).replace("\\", "/")
    for a, b in LAYOUT:
        if s == a or s.startswith(a + "/"):
            return ROOT / (b + s[len(a):])
    return ROOT / s


def local(section: str, key: str):
    """Value 'string' from config/local.toml ([section] key = '…'), else None."""
    path = ROOT / "config" / "local.toml"
    if not path.is_file():
        return None
    text = path.read_text(encoding="utf-8")
    sec = re.search(r"(?ms)^\[" + re.escape(section) + r"\]\s*$(.*?)(?=^\[|\Z)", text)
    if not sec:
        return None
    m = re.search(r"(?m)^\s*" + re.escape(key) + r"\s*=\s*['\"]([^'\"]*)['\"]", sec.group(1))
    return m.group(1) if m else None


HOME = Path(os.environ.get("DB_HOME") or local("home", "path") or ROOT.parent / "home")


def resolve(p: str) -> Path:
    """'home:<path>' — inside DB_HOME, else the path as is."""
    return HOME / p[5:] if p.startswith("home:") else Path(p)


# stand work folder (DbTools: run/, logs/, runs/) — local.toml [stand] work
STAND = resolve(local("stand", "work") or "home:runs/stand")
