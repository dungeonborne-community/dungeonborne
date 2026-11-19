#!/usr/bin/env python3
"""Dumps a Discord server's structure and metadata for analysis. Read-only: the server is not changed.

    python audit.py                 # production server
    python audit.py --guild <ID>    # another server

Output: audit/<date>_<guild>/
  dump.json   — everything that could be read (roles, channels with permissions by role name, activity, onboarding, …)
  report.md   — the same in readable form: channel tree, roles, activity, members

Activity: based on the last 100 messages of each channel (the Discord API does not give exact message counts).
Members: aggregates only (no nicknames): roles, join dates, onboarding completion.
"""
import argparse
import json
import os
import sys
from collections import Counter
from datetime import datetime, timedelta, timezone
from pathlib import Path

import setup_server as s

HERE = Path(__file__).resolve().parent
TYPES = {0: "text", 2: "voice", 4: "category", 5: "announcement", 13: "stage", 15: "forum", 16: "media"}
PERM_NAMES = {b: n for n, b in s.PERMS.items()}
DISCORD_EPOCH_MS = 1420070400000


def perm_list(value):
    v = int(value)
    return [PERM_NAMES[b] for b in sorted(PERM_NAMES) if v >> b & 1]


def snowflake_time(sf):
    return datetime.fromtimestamp(((int(sf) >> 22) + DISCORD_EPOCH_MS) / 1000, tz=timezone.utc)


def safe(api, path, default=None):
    try:
        return api.get(path)
    except s.DiscordError as e:
        print(f"  ! {path}: {str(e)[:160]}")
        return default


def activity(api, channel_id, now):
    """Last 100 messages: latest message, how many in 7/30 days, how many distinct authors, bot share"""
    msgs = safe(api, f"/channels/{channel_id}/messages?limit=100", [])
    if not msgs:
        return {"sampled": 0, "last": None}
    times = [datetime.fromisoformat(m["timestamp"]) for m in msgs]
    humans = [m for m in msgs if not m["author"].get("bot")]
    return {
        "sampled": len(msgs),
        "last": max(times).isoformat(),
        "days_since_last": (now - max(times)).days,
        "last_7d": sum(t > now - timedelta(days=7) for t in times),
        "last_30d": sum(t > now - timedelta(days=30) for t in times),
        "authors": len({m["author"]["id"] for m in humans}),
        "bot_share": round(1 - len(humans) / len(msgs), 2),
        "capped": len(msgs) == 100,  # 100 means there may be more messages
    }


def forum_activity(api, gid, channel_id, active_threads, now):
    archived = safe(api, f"/channels/{channel_id}/threads/archived/public?limit=100", {"threads": []})["threads"]
    threads = [t for t in active_threads if t.get("parent_id") == channel_id] + archived
    last = [snowflake_time(t.get("last_message_id") or t["id"]) for t in threads]
    return {
        "posts": len(threads),
        "posts_capped": len(archived) == 100,
        "last": max(last).isoformat() if last else None,
        "days_since_last": (now - max(last)).days if last else None,
        "posts_30d": sum(snowflake_time(t["id"]) > now - timedelta(days=30) for t in threads),
        "messages_total": sum(t.get("message_count", 0) for t in threads),
    }


def overwrites_named(ch, roles_by_id, gid):
    out = []
    for o in ch.get("permission_overwrites", []):
        who = "@everyone" if o["id"] == gid else roles_by_id.get(o["id"], {}).get("name", f"member:{o['id']}") if o["type"] == 0 else f"member:{o['id']}"
        out.append({"target": who, "allow": perm_list(o["allow"]), "deny": perm_list(o["deny"])})
    return out


def members_summary(api, gid, roles_by_id, now):
    members, after = [], "0"
    while True:
        page = safe(api, f"/guilds/{gid}/members?limit=1000&after={after}", [])
        if not page:
            break
        members += page
        if len(page) < 1000:
            break
        after = page[-1]["user"]["id"]
    humans = [m for m in members if not m["user"].get("bot")]
    role_counts = Counter(r for m in humans for r in m["roles"])
    joined = [datetime.fromisoformat(m["joined_at"]) for m in humans if m.get("joined_at")]
    flags = [m.get("flags", 0) for m in humans]
    return {
        "total": len(members),
        "humans": len(humans),
        "bots": [m["user"]["username"] for m in members if m["user"].get("bot")],
        "no_roles": sum(1 for m in humans if not m["roles"]),
        "pending_screening": sum(1 for m in humans if m.get("pending")),
        "onboarding_completed": sum(1 for f in flags if f & (1 << 1)),
        "onboarding_started_not_completed": sum(1 for f in flags if f & (1 << 3) and not f & (1 << 1)),
        "joined_7d": sum(t > now - timedelta(days=7) for t in joined),
        "joined_30d": sum(t > now - timedelta(days=30) for t in joined),
        "joined_by_month": dict(sorted(Counter(t.strftime("%Y-%m") for t in joined).items())),
        "role_counts": {roles_by_id[r]["name"]: n for r, n in role_counts.most_common() if r in roles_by_id},
    }


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser(description="Dump the server's structure and metadata (read-only)")
    ap.add_argument("--guild", default=s.PROD_GUILD)
    args = ap.parse_args()
    s.load_env(HERE / ".env")
    api = s.Discord(os.environ["DISCORD_TOKEN"])
    gid = args.guild
    now = datetime.now(timezone.utc)

    print("Server and settings…")
    guild = api.get(f"/guilds/{gid}?with_counts=true")
    roles = sorted(api.get(f"/guilds/{gid}/roles"), key=lambda r: -r["position"])
    roles_by_id = {r["id"]: r for r in roles}
    channels = api.get(f"/guilds/{gid}/channels")
    names = {c["id"]: c["name"] for c in channels}
    active_threads = safe(api, f"/guilds/{gid}/threads/active", {"threads": []})["threads"]

    print(f"Activity of {len(channels)} channels…")
    chan_out = []
    for c in sorted(channels, key=lambda c: (c["type"] != 4, c["position"])):
        item = {
            "id": c["id"],
            "name": c["name"],
            "type": TYPES.get(c["type"], c["type"]),
            "parent": names.get(c.get("parent_id")),
            "position": c["position"],
            "topic": c.get("topic"),
            "nsfw": c.get("nsfw", False),
            "slowmode": c.get("rate_limit_per_user", 0),
            "overwrites": overwrites_named(c, roles_by_id, gid),
            "hidden_from_everyone": any(o["id"] == gid and int(o["deny"]) & (1 << 10) for o in c.get("permission_overwrites", [])),
        }
        if c.get("parent_id"):
            parent = next((p for p in channels if p["id"] == c["parent_id"]), None)
            item["synced_with_category"] = parent is not None and sorted(map(str, c["permission_overwrites"])) == sorted(map(str, parent["permission_overwrites"]))
        if c["type"] in (15, 16):
            item["tags"] = [t["name"] for t in c.get("available_tags", [])]
            item["activity"] = forum_activity(api, gid, c["id"], active_threads, now)
        elif c["type"] in (0, 5, 2, 13):
            item["activity"] = activity(api, c["id"], now)
        if c["type"] == 2:
            item["user_limit"] = c.get("user_limit", 0)
        chan_out.append(item)

    print("Members, onboarding, automod, webhooks, invites…")
    members = members_summary(api, gid, roles_by_id, now)
    role_out = [
        {
            "name": r["name"],
            "id": r["id"],
            "position": r["position"],
            "color": f"#{r['color']:06X}" if r["color"] else None,
            "hoist": r["hoist"],
            "mentionable": r["mentionable"],
            "managed": r.get("managed", False),
            "members": members["role_counts"].get(r["name"], 0) if r["id"] != gid else members["humans"],
            "permissions": perm_list(r["permissions"]),
        }
        for r in roles
    ]
    onboarding = safe(api, f"/guilds/{gid}/onboarding", {})
    for p in onboarding.get("prompts", []):
        for o in p["options"]:
            o["role_names"] = [roles_by_id.get(r, {}).get("name", r) for r in o.get("role_ids", [])]
            o["channel_names"] = [names.get(c, c) for c in o.get("channel_ids", [])]
    onboarding["default_channel_names"] = [names.get(c, c) for c in onboarding.get("default_channel_ids", [])]
    invites = safe(api, f"/guilds/{gid}/invites", [])
    webhooks = safe(api, f"/guilds/{gid}/webhooks", [])
    dump = {
        "exported_at": now.isoformat(),
        "guild": {
            k: guild.get(k)
            for k in (
                "id", "name", "description", "features", "approximate_member_count", "approximate_presence_count",
                "verification_level", "explicit_content_filter", "default_message_notifications", "mfa_level",
                "nsfw_level", "premium_tier", "premium_subscription_count", "preferred_locale", "vanity_url_code",
                "afk_timeout", "system_channel_flags", "max_members",
            )
        }
        | {
            "rules_channel": names.get(guild.get("rules_channel_id")),
            "public_updates_channel": names.get(guild.get("public_updates_channel_id")),
            "system_channel": names.get(guild.get("system_channel_id")),
            "afk_channel": names.get(guild.get("afk_channel_id")),
            "safety_alerts_channel": names.get(guild.get("safety_alerts_channel_id")),
        },
        "roles": role_out,
        "channels": chan_out,
        "members": members,
        "onboarding": onboarding,
        "welcome_screen": safe(api, f"/guilds/{gid}/welcome-screen", None),
        "automod_rules": [
            {"name": r["name"], "enabled": r["enabled"], "trigger_type": r["trigger_type"], "actions": [a["type"] for a in r["actions"]]}
            for r in safe(api, f"/guilds/{gid}/auto-moderation/rules", [])
        ],
        "webhooks": [{"name": w.get("name"), "channel": names.get(w.get("channel_id")), "type": w.get("type")} for w in webhooks],
        "integrations": [{"name": i.get("name"), "type": i.get("type")} for i in safe(api, f"/guilds/{gid}/integrations", [])],
        "invites": {
            "count": len(invites),
            "permanent": sum(1 for i in invites if not i.get("max_age")),
            "total_uses": sum(i.get("uses", 0) for i in invites),
            "top": sorted(
                ({"code": i["code"], "uses": i.get("uses", 0), "channel": i.get("channel", {}).get("name"), "permanent": not i.get("max_age")} for i in invites),
                key=lambda i: -i["uses"],
            )[:10],
        },
        "scheduled_events": [{"name": e["name"], "start": e["scheduled_start_time"], "status": e["status"]} for e in safe(api, f"/guilds/{gid}/scheduled-events", [])],
        "emojis": len(safe(api, f"/guilds/{gid}/emojis", [])),
        "stickers": len(safe(api, f"/guilds/{gid}/stickers", [])),
        "active_threads": len(active_threads),
    }

    out = HERE / "audit" / f"{now:%Y-%m-%d}_{gid}"
    out.mkdir(parents=True, exist_ok=True)
    (out / "dump.json").write_text(json.dumps(dump, ensure_ascii=False, indent=2), encoding="utf-8")
    (out / "report.md").write_text(render(dump), encoding="utf-8")
    print(f"\nDone: {out}")


def act_str(c):
    a = c.get("activity") or {}
    if c["type"] in ("forum", "media"):
        if not a.get("posts"):
            return "no posts"
        return f"posts {a['posts']}{'+' if a.get('posts_capped') else ''}, in 30 d {a['posts_30d']}, last {a['days_since_last']} d ago"
    if not a.get("sampled"):
        return "no messages"
    cap = "+" if a.get("capped") else ""
    return (f"last {a['days_since_last']} d ago · 7 d: {a['last_7d']}{cap} · 30 d: {a['last_30d']}{cap} · "
            f"authors {a['authors']} · bots {int(a['bot_share'] * 100)}%")


def render(d):
    g, m = d["guild"], d["members"]
    L = [f"# Server audit «{g['name']}»", "", f"Exported: {d['exported_at'][:16]} UTC", ""]
    L += ["## Server", "",
          f"- Members: {g['approximate_member_count']} (online {g['approximate_presence_count']}), humans {m['humans']}, bots {len(m['bots'])} ({', '.join(m['bots'])})",
          f"- Community: {'yes' if 'COMMUNITY' in (g['features'] or []) else 'no'} · verification {g['verification_level']} · content filter {g['explicit_content_filter']} · default notifications {'mentions only' if g['default_message_notifications'] else 'all messages'}",
          f"- Boosts: level {g['premium_tier']}, boosts {g['premium_subscription_count']} · locale {g['preferred_locale']}",
          f"- Channels: rules {g['rules_channel']}, updates {g['public_updates_channel']}, system {g['system_channel']}, AFK {g['afk_channel']}",
          f"- Description: {g['description']}", ""]
    L += ["## Members", "",
          f"- No roles: {m['no_roles']} · completed onboarding: {m['onboarding_completed']} · started but quit: {m['onboarding_started_not_completed']}",
          f"- Joined in 7 days: {m['joined_7d']} · in 30 days: {m['joined_30d']}",
          "- By month: " + ", ".join(f"{k}: {v}" for k, v in m["joined_by_month"].items()), ""]
    L += ["## Roles (top to bottom)", "", "| Role | Members | Color | Hoisted | Permissions |", "|---|---|---|---|---|"]
    for r in d["roles"]:
        perms = "Administrator" if "administrator" in r["permissions"] else ", ".join(p for p in r["permissions"] if p in (
            "kick_members", "ban_members", "moderate_members", "manage_messages", "manage_channels", "manage_roles", "manage_guild", "mention_everyone"))
        L.append(f"| {r['name']}{' (bot)' if r['managed'] else ''} | {r['members']} | {r['color'] or '—'} | {'yes' if r['hoist'] else ''} | {perms or '—'} |")
    L += ["", "## Channels", ""]
    cats = [c for c in d["channels"] if c["type"] == "category"]
    for parent in [None] + [c["name"] for c in cats]:
        kids = [c for c in d["channels"] if c["type"] != "category" and c["parent"] == parent]
        if parent is None and not kids:
            continue
        cat = next((c for c in cats if c["name"] == parent), None)
        hidden = " 🔒" if cat and cat["hidden_from_everyone"] else ""
        L.append(f"### {parent or '(no category)'}{hidden}")
        for c in sorted(kids, key=lambda c: c["position"]):
            flags = (" 🔒" if c["hidden_from_everyone"] else "") + (f" ⏱{c['slowmode']}s" if c["slowmode"] else "")
            L.append(f"- **{c['name']}** ({c['type']}){flags} — {act_str(c)}")
        L.append("")
    ob = d["onboarding"]
    L += ["## Onboarding", "", f"- Enabled: {ob.get('enabled')} · default channels: {len(ob.get('default_channel_names', []))}"]
    for p in ob.get("prompts", []):
        L.append(f"- **{p['title']}** ({'on join' if p['in_onboarding'] else 'only in «Channels & Roles»'})")
        for o in p["options"]:
            L.append(f"  - {o['title']} → roles {o['role_names']}, channels {o['channel_names']}")
    L += ["", "## Other", "",
          f"- Automod: {d['automod_rules'] or 'no rules'}",
          f"- Webhooks: {d['webhooks'] or 'none'}",
          f"- Integrations: {[i['name'] for i in d['integrations']]}",
          f"- Invites: {d['invites']['count']} (permanent {d['invites']['permanent']}), uses {d['invites']['total_uses']}",
          f"- Events: {d['scheduled_events'] or 'none'} · emojis {d['emojis']} · stickers {d['stickers']} · active threads {d['active_threads']}", ""]
    return "\n".join(L)


if __name__ == "__main__":
    sys.exit(main())
