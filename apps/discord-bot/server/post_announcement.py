"""Publishes an announcement from a draft file, the same way /announce does: post, rules card, «🌍 …» thread with translations,
language links and buttons, delivery to the announcement channel's followers.

Draft (drafts/*.md): sections «## ENGLISH», «## RUSSIAN», «## 中文» (or other languages from bot.toml languages),
each with a ```…``` block. Rules go after the ---rules--- line: a first line «### Title» becomes the card title.

    python post_announcement.py drafts/clotho-announcement.md --channel "📢・announcements" --ping "🔔 Events"            # check only
    python post_announcement.py drafts/clotho-announcement.md --channel "📢・announcements" --ping "🔔 Events" --publish  # publish

Server: DISCORD_GUILD_ID from .env. The production server only with --guild and the owner's explicit "yes" to this exact text.

--as-owner — post as the server owner: via a channel webhook with the owner's name and avatar (the Discord API gives them to the bot).
--as-project — post as the project: the «Dungeonborne Community» webhook with the assets/project-avatar.png logo (no specific person).
No personal token is needed or used: automating a personal account (self-bot) is against Discord's rules.
Channel and role can be given as part of the name: --channel announcements --ping Events.

--delete-after 7d — delete the post (together with the translations thread) after a delay: 30m, 12h, 7d (1m to 365d).
The bot does the deleting: the script drops a request into its queue bot/data/expire_queue, and the bot picks it up within 30 s.
"""
import argparse
import json
import re
import sys
import time
import tomllib
from pathlib import Path

import requests

HERE = Path(__file__).resolve().parent
API = "https://discord.com/api/v10"
RULES_MARK = "---rules---"
MAX_POST = 1800
SECTION_LANG = {"ENGLISH": "en", "RUSSIAN": "ru", "中文": "zh"}


def env():
    return dict(re.findall(r'^(\w+)\s*=\s*"?([^"\n]*)"?', (HERE / ".env").read_text(encoding="utf-8"), re.M))


def call(h, method, url, **kw):
    while True:
        r = requests.request(method, API + url, headers=h, **kw)
        if r.status_code == 429:
            time.sleep(r.json().get("retry_after", 1) + 0.2)
            continue
        if not r.ok:
            sys.exit(f"Discord {method} {url} → {r.status_code}: {r.text[:300]}")
        return r.json() if r.content else None


def parse_draft(text):
    """{language code: block text} from the ## ENGLISH / ## RUSSIAN / ## 中文 sections"""
    out = {}
    for m in re.finditer(r"^## (.+?)\s*\n+```[^\n]*\n(.*?)\n```", text, re.S | re.M):
        lang = SECTION_LANG.get(m[1].strip())
        if lang:
            out[lang] = m[2]
    return out


def split_rules(text):
    post, _, rules = text.partition(RULES_MARK)
    rules = rules.strip()
    if not rules:
        return post.rstrip(), None
    first, _, rest = rules.partition("\n")
    if first.startswith("#"):
        return post.rstrip(), {"title": first.lstrip("#").strip()[:250], "description": rest.strip()[:4000], "color": 0xE0A84A}
    return post.rstrip(), {"description": rules[:4000], "color": 0xE0A84A}


def with_ping(text, mention):
    text = text.rstrip()
    if not mention:
        return text
    head, _, last = text.rpartition("\n")
    if last.startswith("-# "):
        return f"{head}\n{last} · {mention}" if head else f"{last} · {mention}"
    return f"{text}\n-# {mention}"


def message(content, card, mentions):
    body = {"content": content, "allowed_mentions": mentions}
    if card:
        body["embeds"] = [card]
    else:
        body["flags"] = 1 << 2  # no link previews
    return body


def find(items, wanted, what):
    """Exact name, otherwise the single match on part of the name"""
    if wanted in items:
        return items[wanted]
    # «announcements» = «📢・announcements», not «📣・test-announcements»; «Events» = «🔔 Events»
    bare = lambda k: k.split("・", 1)[-1].split(" ", 1)[-1].lower() if ("・" in k or " " in k) else k.lower()
    hits = [v for k, v in items.items() if bare(k) == wanted.lower()]
    if len(hits) == 1:
        return hits[0]
    hits = [v for k, v in items.items() if wanted.lower() in k.lower()]
    if len(hits) == 1:
        return hits[0]
    sys.exit(f"{what} «{wanted}»: " + ("not found" if not hits else f"several matches, be more specific"))


def owner_identity(h, guild):
    """Server owner's name and avatar, for the webhook"""
    m = call(h, "GET", f"/guilds/{guild['id']}/members/{guild['owner_id']}")
    u = m["user"]
    name = m.get("nick") or u.get("global_name") or u["username"]
    if m.get("avatar"):
        avatar = f"https://cdn.discordapp.com/guilds/{guild['id']}/users/{u['id']}/avatars/{m['avatar']}.png?size=256"
    elif u.get("avatar"):
        avatar = f"https://cdn.discordapp.com/avatars/{u['id']}/{u['avatar']}.png?size=256"
    else:
        avatar = None
    return name, avatar


PROJECT_NAME = "Dungeonborne Community"
PROJECT_AVATAR = HERE / "assets" / "project-avatar.png"


def project_avatar_data():
    """Project logo as a data URI: Discord stores it in the webhook itself, no external link needed"""
    import base64
    return "data:image/png;base64," + base64.b64encode(PROJECT_AVATAR.read_bytes()).decode()


def channel_webhook(h, channel_id, project=False):
    """The bot's webhook in the channel (created once): messages sent through it can carry buttons.
    project=True — the webhook has the project's name and avatar (messages without their own username/avatar_url use them)."""
    me = call(h, "GET", "/users/@me")["id"]
    hook = next((w for w in call(h, "GET", f"/channels/{channel_id}/webhooks")
                 if w.get("user", {}).get("id") == me and w.get("token")), None)
    if not hook:
        hook = call(h, "POST", f"/channels/{channel_id}/webhooks", json={"name": "Announcements"})
    if project:
        call(h, "PATCH", f"/webhooks/{hook['id']}", json={"name": PROJECT_NAME, "avatar": project_avatar_data()})
    return hook


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("draft")
    ap.add_argument("--channel", required=True)
    ap.add_argument("--ping", help="subscription role; without it there is no ping (this script never pings @everyone)")
    ap.add_argument("--guild", help="server ID (default: DISCORD_GUILD_ID from .env)")
    ap.add_argument("--publish", action="store_true", help="publish; without the flag, only check")
    ap.add_argument("--as-owner", action="store_true", help="as the server owner (webhook with the owner's name and avatar)")
    ap.add_argument("--as-project", action="store_true", help="as the project: «Dungeonborne Community» with the logo")
    ap.add_argument("--delete-after", help="delete the post after a delay: 30m, 12h, 7d")
    args = ap.parse_args()

    if args.delete_after and not re.fullmatch(r"\d+(m|h|d|w)", args.delete_after.strip()):
        sys.exit("--delete-after: a number and a unit: 30m, 12h, 7d, 2w")
    e = env()
    gid = args.guild or e["DISCORD_GUILD_ID"]
    h = {"Authorization": "Bot " + e["DISCORD_TOKEN"]}
    bot_cfg = tomllib.loads((HERE / "bot" / "bot.toml").read_text(encoding="utf-8"))
    langs = bot_cfg.get("languages", [])
    texts = parse_draft(Path(args.draft).read_text(encoding="utf-8"))
    if "en" not in texts:
        sys.exit("the draft has no «## ENGLISH» section with a ```…``` block")

    guild = call(h, "GET", f"/guilds/{gid}")
    all_chans = call(h, "GET", f"/guilds/{gid}/channels")
    # duplicate names (event category channels): the channel from the higher category wins, same as in the bot
    cat_pos = {c["id"]: c["position"] for c in all_chans if c["type"] == 4}
    chans = {}
    for c in sorted(all_chans, key=lambda c: (cat_pos.get(c.get("parent_id"), -1), c["position"], int(c["id"]))):
        chans.setdefault(c["name"], c)
    roles = {r["name"]: r["id"] for r in call(h, "GET", f"/guilds/{gid}/roles")}
    ch = find(chans, args.channel, "channel")
    # copies to language channels, same as /announce (bot.toml [[announce.channel]] mirror)
    mirror = next((c.get("mirror", {}) for c in bot_cfg.get("announce", {}).get("channel", []) if c.get("name") == ch["name"]), {})
    role_id = find(roles, args.ping, "role") if args.ping else None
    role_name = next((k for k, v in roles.items() if v == role_id), None)

    def render(t):
        def link(m):
            c = chans.get(m[1])
            if not c:
                sys.exit(f"link to a channel that does not exist: {m[1]}")
            return f"<#{c['id']}>"
        t = re.sub(r"\{#([^}]+)\}", link, t.replace("{server}", guild["name"]))
        return re.sub(r"\{@([^}]+)\}", lambda m: f"<@&{roles[m[1]]}>" if m[1] in roles else f"@{m[1]}", t)

    main_post, main_card = split_rules(render(texts["en"]))
    main_text = with_ping(main_post, f"<@&{role_id}>" if role_id else "")
    tr = [(l, split_rules(render(texts[l["code"]]))) for l in langs if l["code"] in texts]
    problems = [f"{name}: post text is {len(p)} characters (> {MAX_POST})"
                for name, p in [("en", main_post)] + [(l["code"], p) for l, (p, _) in tr] if len(p) > MAX_POST]

    who = None
    if args.as_owner and args.as_project:
        sys.exit("--as-owner and --as-project cannot be combined")
    if args.as_owner:
        who = owner_identity(h, guild)
    if args.as_project:
        if not PROJECT_AVATAR.exists():
            sys.exit(f"logo not found: {PROJECT_AVATAR}")
        who = (PROJECT_NAME, None)
    print(f"Server: {guild['name']} ({gid}) · channel: {ch['name']} · ping: {role_name or 'none'}")
    print(f"Posting as: {who[0] + ' (webhook)' if who else 'the bot'}")
    if args.delete_after:
        print(f"Deletion: after {args.delete_after} (the bot must be running with the [expire] section)")
    print(f"en: post {len(main_post)} chars, card: {'yes' if main_card else 'no'}")
    for l, (p, c) in tr:
        copy = mirror.get(l["code"])
        if copy and copy not in chans:
            problems.append(f"{l['code']}: channel for the copy '{copy}' is not on the server")
        print(f"{l['code']}: post {len(p)} chars, card: {'yes' if c else 'no'}" + (f", copy → {copy}" if copy else ""))
    if problems:
        sys.exit("\n".join(problems))
    if not args.publish:
        print("Check passed. Add --publish to publish.")
        return

    mentions = {"parse": [], "roles": [role_id] if role_id else []}
    hook = None
    if who:
        hook = channel_webhook(h, ch["id"], project=args.as_project)
        body = message(main_text, main_card, mentions) | {"username": who[0][:80]}
        if who[1]:
            body["avatar_url"] = who[1]
        msg = call(h, "POST", f"/webhooks/{hook['id']}/{hook['token']}?wait=true", json=body)
    else:
        msg = call(h, "POST", f"/channels/{ch['id']}/messages", json=message(main_text, main_card, mentions))
    if tr:
        name = "🌍 " + " · ".join(f"{l['flag']} {l['name']}" for l in langs)
        thread = call(h, "POST", f"/channels/{ch['id']}/messages/{msg['id']}/threads", json={"name": name[:100], "auto_archive_duration": 10080})
        links, buttons = [], []
        for l, (p, c) in tr:
            m = call(h, "POST", f"/channels/{thread['id']}/messages", json=message(p, c, {"parse": []}))
            links.append(f"{l['flag']} [{l['name']}](https://discord.com/channels/{gid}/{thread['id']}/{m['id']})")
            buttons.append({"type": 2, "style": 2, "custom_id": f"tr:{thread['id']}:{m['id']}", "label": l["name"], "emoji": {"name": l["flag"]}})
        rows = [{"type": 1, "components": buttons[i:i + 5]} for i in range(0, len(buttons), 5)]
        edit = {"content": f"{main_text}\n-# 🌍 **Translate:** {' · '.join(links)}", "components": rows, "allowed_mentions": {"parse": []}}
        if hook:  # a webhook message can only be edited by the webhook itself
            call(h, "PATCH", f"/webhooks/{hook['id']}/{hook['token']}/messages/{msg['id']}", json=edit)
        else:
            call(h, "PATCH", f"/channels/{ch['id']}/messages/{msg['id']}", json=edit)
    # copies: the full translation, no ping, last line links to the original (the bot uses it to delete the copy together with the original)
    original = f"https://discord.com/channels/{gid}/{ch['id']}/{msg['id']}"
    for l, (p, c) in tr:
        copy = mirror.get(l["code"])
        if not copy:
            continue
        body = message(f"{p}\n-# 🇬🇧 [Original]({original})", c, {"parse": []})
        if who:
            mh = channel_webhook(h, chans[copy]["id"], project=args.as_project)
            body |= {"username": who[0][:80]} | ({"avatar_url": who[1]} if who[1] else {})
            call(h, "POST", f"/webhooks/{mh['id']}/{mh['token']}?wait=true", json=body)
        else:
            call(h, "POST", f"/channels/{chans[copy]['id']}/messages", json=body)
        print(f"Copy {l['code']} → {copy}")
    if ch["type"] == 5:
        call(h, "POST", f"/channels/{ch['id']}/messages/{msg['id']}/crosspost")
    if args.delete_after:
        queue = HERE / "bot" / "data" / "expire_queue"
        queue.mkdir(parents=True, exist_ok=True)
        (queue / f"{msg['id']}.json").write_text(
            json.dumps({"channel": ch["id"], "message": msg["id"], "after": args.delete_after.strip()}), encoding="utf-8")
        print(f"Scheduled for deletion in {args.delete_after}")
    print(f"Published: https://discord.com/channels/{gid}/{ch['id']}/{msg['id']}")


if __name__ == "__main__":
    main()
