#!/usr/bin/env python3
"""Go live for everyone: lift staging (reveal the new channels), enable onboarding, open the bot's commands.

    python go_live.py                  # plan for the production server; nothing changes
    python go_live.py --apply          # apply
    python go_live.py --guild <ID>     # another server (e.g. the test one)
    python go_live.py --old-first      # keep the old categories above the new ones

What --apply does:
1. setup_server.py without --staging: channel permissions per server.yaml (hiding from @everyone goes away; channels that are
   private by design (language, STAFF, creator channels) stay private), server settings, onboarding;
2. in this server's bot config (bot/*.toml with the same guild_id): admin_only = true → false;
3. API check: channel permissions match the config, onboarding is enabled.
Discord's onboarding requirements are checked before applying; if they are not met, the script changes nothing.
For the production server, server.prod.yaml is layered on top of server.yaml (own chats, removed channels, renames).
After --apply restart this server's bot: its commands become visible to everyone.
"""
import argparse
import os
import re
import sys
import tomllib
from pathlib import Path

import setup_server as s

HERE = Path(__file__).resolve().parent
PROD_GUILD = s.PROD_GUILD
VIEW = 1 << s.PERMS["view_channel"]
SEND = 1 << s.PERMS["send_messages"]


def config_channels(cfg):
    """{name: (category, channel)} for every channel in server.yaml"""
    return {ch["name"]: (cat, ch) for cat in cfg["categories"] for ch in cat.get("channels", [])}


def everyone_deny(channel, gid):
    return sum(int(o["deny"]) for o in channel.get("permission_overwrites", []) if o["id"] == gid)


def onboarding_problems(cfg, on_server, gid):
    """Discord's requirements: ≥ 7 default channels, of which ≥ 5 are visible to everyone and @everyone can post in.
    Channels from server.yaml are checked against the config, the server's own channels (external) against their permissions on the server."""
    ob = cfg.get("onboarding") or {}
    defaults = ob.get("default_channels", [])
    channels = config_channels(cfg)
    problems, writable = [], 0
    for name in defaults:
        if name in channels:
            cat, ch = channels[name]
            deny = s.channel_perms(cfg, cat, ch).get("@everyone", (0, 0))[1]
            text_like = ch.get("type", "text") not in ("voice", "stage")
        elif name in on_server:
            deny = everyone_deny(on_server[name], gid)
            text_like = on_server[name]["type"] not in (2, 13)
        else:
            problems.append(f"{name}: neither in the config nor on the server")
            continue
        if deny & VIEW:
            problems.append(f"{name}: hidden from @everyone, Discord will not accept it as a default channel")
        elif not deny & SEND and text_like:
            writable += 1
    for prompt in ob.get("prompts", []):
        for opt in prompt.get("options", []):
            for name in opt.get("channels", []):
                if name in cfg.get("external_channels", []):
                    if name not in on_server:
                        problems.append(f"onboarding «{opt['title']}»: own channel {name} is not on the server")
                    elif everyone_deny(on_server[name], gid) & VIEW:
                        problems.append(f"onboarding «{opt['title']}»: {name} is hidden from @everyone")
    if len(defaults) < 7:
        problems.append(f"{len(defaults)} default channels, at least 7 required")
    if writable < 5:
        problems.append(f"{writable} default channels open for posting, at least 5 required")
    return problems, len(defaults), writable


def hidden_from_everyone(channel, gid):
    return any(o["id"] == gid and int(o["deny"]) & VIEW for o in channel.get("permission_overwrites", []))


def visibility(cfg, api, gid):
    """(will become visible to everyone, stay private by design, missing on the server), based on the server's current state"""
    on_server = {c["name"]: c for c in api.get(f"/guilds/{gid}/channels")}
    opens, stays, missing = [], [], []
    for name, (cat, ch) in config_channels(cfg).items():
        want_hidden = bool(s.channel_perms(cfg, cat, ch).get("@everyone", (0, 0))[1] & VIEW)
        cur = on_server.get(name)
        if cur is None:
            missing.append(name)
        elif want_hidden:
            stays.append(name)
        elif hidden_from_everyone(cur, gid):
            opens.append(name)
    return opens, stays, missing


def bot_config_for(gid):
    for path in sorted((HERE / "bot").glob("*.toml")):
        try:
            if str(tomllib.loads(path.read_text(encoding="utf-8"))["discord"]["guild_id"]) == gid:
                return path
        except (KeyError, tomllib.TOMLDecodeError):
            continue
    return None


def open_bot_commands(path, apply):
    text = path.read_text(encoding="utf-8")
    if not re.search(r"(?m)^admin_only\s*=\s*true", text):
        print(f"= {path.name}: bot commands are already visible to everyone")
        return False
    print(f"~ {path.name}: admin_only = true → false (bot commands will become visible to everyone)")
    if apply:
        text = re.sub(r"(?m)^admin_only\s*=\s*true.*$", "admin_only = false                 # live for everyone: commands visible to everyone", text)
        path.write_text(text, encoding="utf-8")
    return True


def verify(cfg, api, gid):
    problems = []
    on_server = {c["name"]: c for c in api.get(f"/guilds/{gid}/channels")}
    for name, (cat, ch) in config_channels(cfg).items():
        cur = on_server.get(name)
        if cur is None:
            continue
        want_hidden = bool(s.channel_perms(cfg, cat, ch).get("@everyone", (0, 0))[1] & VIEW)
        if hidden_from_everyone(cur, gid) != want_hidden:
            problems.append(f"{name}: {'hidden from' if not want_hidden else 'visible to'} @everyone, but the config says the opposite")
    ob = api.get(f"/guilds/{gid}/onboarding")
    if not ob.get("enabled"):
        problems.append("onboarding did not turn on; enable it manually: Server Settings → Onboarding")
    print(f"Check: {len(on_server)} channels, {len(ob.get('prompts', []))} onboarding prompts, "
          f"{len(ob.get('default_channel_ids', []))} default channels, onboarding {'enabled' if ob.get('enabled') else 'DISABLED'}")
    return problems


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser(description="Go live for everyone: reveal channels, enable onboarding, open the bot's commands")
    ap.add_argument("--guild", default=PROD_GUILD, help=f"server ID (default: production {PROD_GUILD})")
    ap.add_argument("--apply", action="store_true", help="apply (without the flag, only the plan is shown)")
    ap.add_argument("--old-first", action="store_true", help="keep the old categories above the new ones")
    ap.add_argument("--config", default=str(HERE / "server.yaml"))
    ap.add_argument("--overlay", help="overlay on top of server.yaml (production defaults to server.prod.yaml)")
    args = ap.parse_args()

    s.load_env(HERE / ".env")
    token = os.environ.get("DISCORD_TOKEN")
    if not token:
        print("DISCORD_TOKEN is required in .env")
        return 2
    gid = args.guild
    overlay = s.overlay_for(gid, args.overlay)
    cfg, warnings = s.load_config(Path(args.config), overlay)
    for w in warnings:
        print(f"! {w}")
    cfg["_staging"] = []
    cfg["_old_first"] = args.old_first
    api = s.Discord(token)
    guild = api.get(f"/guilds/{gid}")
    print(f"══ Go live: {guild['name']} ({gid}){' — PRODUCTION' if gid == PROD_GUILD else ''}")
    print(f"══ Config: {Path(args.config).name}" + (f" + {overlay.name}" if overlay else ""))
    print(f"══ Mode: {'APPLY' if args.apply else 'PLAN — nothing changes; add --apply to apply'}\n")

    on_server = {c["name"]: c for c in api.get(f"/guilds/{gid}/channels")}
    problems, total, writable = onboarding_problems(cfg, on_server, gid)
    print(f"Onboarding: {total} default channels (≥ 7 required), {writable} of them open for posting (≥ 5 required)")
    if "COMMUNITY" not in guild["features"] and not cfg["server"].get("community"):
        problems.append("Community is disabled on the server, and onboarding requires it")
    for p in problems:
        print(f"! {p}")
    if problems:
        print("\nStopped: fix server.yaml; the server was not changed.")
        return 1

    opens, stays, missing = visibility(cfg, api, gid)
    print(f"Will become visible to everyone: {len(opens)}" + (f" — {', '.join(opens)}" if opens else ""))
    print(f"Stay private by design (language, role, STAFF): {len(stays)}")
    if missing:
        print(f"Will be created: {', '.join(missing)}")
    print()

    rc = s.run(cfg, api, gid, args.apply)
    print()
    bot_cfg = bot_config_for(gid)
    reopened = open_bot_commands(bot_cfg, args.apply) if bot_cfg else False
    if not bot_cfg:
        print(f"! bot config with guild_id = {gid} not found in bot/*.toml, leaving the bot commands alone")

    if not args.apply:
        return rc
    print()
    issues = verify(cfg, api, gid)
    for p in issues:
        print(f"! {p}")
    print("\nDone." if rc == 0 and not issues else "\nDone with errors, see above.")
    if reopened:
        print(f"Restart the bot with {bot_cfg.name}: in the bot folder → .\\target\\release\\discord-bot.exe {bot_cfg.name}")
    return 1 if rc or issues else 0


if __name__ == "__main__":
    sys.exit(main())
