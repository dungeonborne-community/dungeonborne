"""Exports texts to Markdown (for reading and editing outside Discord): export/welcome.md, rules.md, about.md, announcements.md.

welcome / rules / about come from server.yaml (English original and translations); with --posted also export/posted.md: what is currently
in 📢・announcements on the server from .env (DISCORD_GUILD_ID): post, rules card and translations from the 🌍 thread.
export/announcements.md is an announcement draft; the script does not touch it.
Links {#channel} and <#id> become «#channel», roles become «@role».

  python export_texts.py            # welcome, rules, about, faq
  python export_texts.py --posted   # plus export of the published posts to export/posted.md
"""
import json
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
OUT = HERE / "export"


def env():
    return {k.strip(): v.strip().strip('"') for k, v in
            (l.split("=", 1) for l in (HERE / ".env").read_text(encoding="utf-8").splitlines() if "=" in l and not l.startswith("#"))}


def api(token):
    h = {"Authorization": "Bot " + token, "User-Agent": "DiscordBot (local, 1)"}

    def get(path):
        for _ in range(5):
            try:
                return json.load(urllib.request.urlopen(urllib.request.Request("https://discord.com/api/v10" + path, headers=h)))
            except urllib.error.HTTPError as e:
                if e.code != 429:
                    raise
                time.sleep(float(json.loads(e.read() or b"{}").get("retry_after", 3)) + 0.5)
        raise SystemExit("Discord rate limit")
    return get


def plain(text, chans, roles):
    text = re.sub(r"\{#([^}]+)\}", lambda m: "#" + m[1], text)
    text = re.sub(r"\{@([^}]+)\}", lambda m: "@" + m[1], text)
    text = re.sub(r"<#(\d+)>", lambda m: "#" + chans.get(m[1], m[1]), text)
    text = re.sub(r"<@&(\d+)>", lambda m: "@" + roles.get(m[1], m[1]), text)
    return text.replace("{server}", "Dungeonborne Community").strip()


def strip_footer(text):
    """Without the service translations line «-# 🌍 …» and the link to the original in copies"""
    return "\n".join(l for l in text.splitlines() if not l.startswith("-# 🌍 ")).strip()


def card(embed, chans, roles):
    out = []
    if embed.get("title"):
        out.append(f"**{embed['title']}**")
    if embed.get("description"):
        out.append(plain(embed["description"], chans, roles))
    for f in embed.get("fields", []):
        out.append(f"**{f['name']}**\n{plain(f['value'], chans, roles)}")
    return "\n".join("> " + l if l else ">" for l in "\n\n".join(out).splitlines())


def from_config(cfg, keys, title, chans, roles):
    langs = cfg.get("languages", {})
    msgs = cfg["messages"]
    parts = [f"# {title}", "", "## English", ""]
    parts += [plain(msgs[k]["content"], chans, roles) + "\n" for k in keys]
    for code, lang in langs.items():
        tr = [msgs[k].get("translations", {}).get(code) for k in keys]
        if any(tr):
            parts += [f"## {lang['name']}", ""]
            parts += [plain(msgs[t]["content"], chans, roles) + "\n" for t in tr if t]
    return "\n".join(parts).rstrip() + "\n"


def main():
    e = env()
    get = api(e["DISCORD_TOKEN"])
    gid = e["DISCORD_GUILD_ID"]
    cfg = yaml.safe_load((HERE / "server.yaml").read_text(encoding="utf-8"))
    channels = get(f"/guilds/{gid}/channels")
    chans = {c["id"]: c["name"] for c in channels}
    roles = {r["id"]: r["name"] for r in get(f"/guilds/{gid}/roles")}
    OUT.mkdir(exist_ok=True)

    files = {
        "welcome.md": (["welcome"], "👋 Welcome"),
        "rules.md": (["rules"], "📜 Rules"),
        "about.md": (["about", "mission"], "🏛️ About"),
        "faq.md": (["faq"], "❓ FAQ"),
    }
    for name, (keys, title) in files.items():
        (OUT / name).write_text(from_config(cfg, keys, title, chans, roles), encoding="utf-8")
        print("export/" + name)
    if "--posted" not in sys.argv:
        return

    ann = next(c for c in channels if c["name"] == "📢・announcements")
    posts = sorted(get(f"/channels/{ann['id']}/messages?limit=100"), key=lambda m: int(m["id"]))
    out = ["# 📢 Announcements", "", f"-# Exported from 📢・announcements, {time.strftime('%Y-%m-%d %H:%M')}. Posts: {len(posts)}.", ""]
    for m in posts:
        if m.get("type") not in (0, 19):
            continue
        who = m["author"].get("global_name") or m["author"]["username"]
        out += ["---", "", f"<!-- {m['timestamp'][:16].replace('T', ' ')} UTC · {who} · https://discord.com/channels/{gid}/{ann['id']}/{m['id']} -->", "",
                "## English", "", plain(strip_footer(m.get("content", "")), chans, roles)]
        for em in m.get("embeds", []):
            if em.get("type") == "rich":
                out += ["", card(em, chans, roles)]
        thread = m.get("thread")
        if thread:
            try:
                tr = sorted(get(f"/channels/{thread['id']}/messages?limit=20"), key=lambda x: int(x["id"]))
            except urllib.error.HTTPError:
                tr = []
            langs = list(cfg.get("languages", {}).values())
            for i, t in enumerate(x for x in tr if x.get("type") in (0, 19) and (x.get("content") or x.get("embeds"))):
                label = langs[i]["name"] if i < len(langs) else f"Translation {i + 1}"
                out += ["", f"## {label}", "", plain(t.get("content", ""), chans, roles)]
                for em in t.get("embeds", []):
                    if em.get("type") == "rich":
                        out += ["", card(em, chans, roles)]
        out.append("")
    (OUT / "posted.md").write_text("\n".join(out).rstrip() + "\n", encoding="utf-8")
    print("export/posted.md")


if __name__ == "__main__":
    main()
