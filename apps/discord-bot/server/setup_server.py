#!/usr/bin/env python3
"""Brings a Discord server in line with the structure in server.yaml via the Discord REST API.

    python setup_server.py --offline   # check the config and print the tree, no token needed
    python setup_server.py             # show the change plan (nothing changes on the server)
    python setup_server.py --apply     # apply

The bot token and server ID come from .env (DISCORD_TOKEN, DISCORD_GUILD_ID).
The script only creates and updates what the config describes; it deletes nothing.
"""
import argparse
import itertools
import os
import re
import sys
import time
from pathlib import Path

import requests
import yaml

API = "https://discord.com/api/v10"
HERE = Path(__file__).resolve().parent

PERMS = {
    "create_instant_invite": 0, "kick_members": 1, "ban_members": 2, "administrator": 3,
    "manage_channels": 4, "manage_guild": 5, "add_reactions": 6, "view_audit_log": 7,
    "priority_speaker": 8, "stream": 9, "view_channel": 10, "send_messages": 11,
    "send_tts_messages": 12, "manage_messages": 13, "embed_links": 14, "attach_files": 15,
    "read_message_history": 16, "mention_everyone": 17, "use_external_emojis": 18,
    "connect": 20, "speak": 21, "mute_members": 22, "deafen_members": 23, "move_members": 24,
    "use_vad": 25, "change_nickname": 26, "manage_nicknames": 27, "manage_roles": 28,
    "manage_webhooks": 29, "manage_guild_expressions": 30, "use_application_commands": 31,
    "request_to_speak": 32, "manage_events": 33, "manage_threads": 34,
    "create_public_threads": 35, "create_private_threads": 36, "use_external_stickers": 37,
    "send_messages_in_threads": 38, "use_embedded_activities": 39, "moderate_members": 40,
    "send_voice_messages": 46, "send_polls": 49,
}
CHANNEL_TYPES = {"text": 0, "voice": 2, "category": 4, "announcement": 5, "stage": 13, "forum": 15, "media": 16}
TYPE_LABELS = {0: "channel", 2: "voice", 4: "category", 5: "announcement channel", 13: "stage", 15: "forum", 16: "media channel"}
TEXT_LIKE = {0, 5, 15, 16}          # Discord stores their names lowercase, without spaces
NEEDS_COMMUNITY = {5, 13}           # can only be created after Community mode is enabled
FORUM_LAYOUT = {"list": 1, "gallery": 2}
FORUM_SORT = {"activity": 0, "creation": 1}
VERIFICATION = {"none": 0, "low": 1, "medium": 2, "high": 3, "very_high": 4}
NOTIFICATIONS = {"all": 0, "mentions": 1}
REQUIRE_TAG = 1 << 4
SUPPRESS_EMBEDS = 1 << 2
PINNED_THREAD = 1 << 1
DISCORD_EPOCH = 1420070400000


class ConfigError(Exception):
    pass


class DiscordError(Exception):
    pass


# ── Config ──────────────────────────────────────────────────────────────

def perm_bits(names, where):
    total = 0
    for name in names or []:
        if name not in PERMS:
            raise ConfigError(f"{where}: unknown permission '{name}'")
        total |= 1 << PERMS[name]
    return total


def text_name(name):
    return name.lower().replace(" ", "-")


PROD_GUILD = "000000000000000000"  # set your own (production guild ID)
PROD_OVERLAY = HERE / "server.prod.yaml"


def overlay_for(gid, explicit=None):
    """Overlay for a server: the one given explicitly, or server.prod.yaml for production if it exists"""
    if explicit:
        return Path(explicit)
    if gid == PROD_GUILD and PROD_OVERLAY.exists():
        return PROD_OVERLAY
    return None


def apply_overlay(cfg, ov):
    """Per-server differences on top of server.yaml: remove channels, categories, roles and onboarding options,
    rename channels, swap channels in onboarding options, refer to the server's own channels (external)."""
    rename = ov.get("rename_channels", {}) or {}
    ren = lambda n: rename.get(n, n)
    drop = set(ov.get("drop_channels", []) or [])
    drop_cats = set(ov.get("drop_categories", []) or [])
    for cat in cfg["categories"]:
        if cat["name"] in drop_cats:
            drop |= {ch["name"] for ch in cat.get("channels", [])}
    cfg["categories"] = [
        dict(cat, channels=[dict(ch, name=ren(ch["name"])) for ch in cat.get("channels", []) if ch["name"] not in drop])
        for cat in cfg["categories"]
        if cat["name"] not in drop_cats
    ]
    drop_roles = set(ov.get("drop_roles", []) or [])
    cfg["roles"] = [r for r in cfg["roles"] if r["name"] not in drop_roles]
    for key in ("rules_channel", "mod_updates_channel", "safety_alerts_channel"):
        if cfg["server"].get(key):
            cfg["server"][key] = ren(cfg["server"][key])
    ob = cfg.get("onboarding")
    if ob:
        ob["default_channels"] = [ren(n) for n in ob.get("default_channels", []) if n not in drop]
        drop_opts = set(ov.get("drop_onboarding_options", []) or [])
        replace = ov.get("onboarding_channels", {}) or {}
        for prompt in ob.get("prompts", []):
            prompt["options"] = [o for o in prompt.get("options", []) if o["title"] not in drop_opts]
            for opt in prompt["options"]:
                if opt["title"] in replace:
                    opt["channels"] = list(replace[opt["title"]])
                else:
                    opt["channels"] = [ren(c) for c in opt.get("channels", []) if c not in drop]
    # {#channel} links in message texts point to the new names
    def fix(text):
        if not isinstance(text, str):
            return text
        for old, new in rename.items():
            text = text.replace("{#" + old + "}", "{#" + new + "}")
        return text
    for msg in cfg["messages"].values():
        for key in ("content", "title"):
            if key in msg:
                msg[key] = fix(msg[key])
        for e in msg.get("embeds", []):
            for key in ("title", "description"):
                if key in e:
                    e[key] = fix(e[key])
            for f in e.get("fields", []):
                f["value"] = fix(f["value"])
    cfg["external_channels"] = list(ov.get("external_channels", []) or [])


def load_config(path, overlay=None):
    cfg = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
    for key, default in (("server", {}), ("roles", []), ("staff", {}), ("presets", {}),
                         ("categories", []), ("messages", {})):
        cfg.setdefault(key, default)
    if overlay:
        apply_overlay(cfg, yaml.safe_load(Path(overlay).read_text(encoding="utf-8")) or {})
        cfg["_overlay"] = Path(overlay).name

    known_roles = {"@everyone"} | {r["name"] for r in cfg["roles"]}
    warnings = []

    def check_block(block, where):
        for role, spec in (block or {}).items():
            if role not in known_roles:
                warnings.append(f"{where}: role '{role}' is not described in roles; it must already exist on the server")
            if spec is None:
                continue
            extra = set(spec) - {"allow", "deny", "reset"}
            if extra:
                raise ConfigError(f"{where}/{role}: unknown keys {sorted(extra)}")
            for key in ("allow", "deny", "reset"):
                perm_bits(spec.get(key), f"{where}/{role}")

    for role in cfg["roles"]:
        perm_bits(role.get("permissions"), f"role {role['name']}")
    for name in cfg["staff"].get("roles", []):
        if name not in known_roles:
            warnings.append(f"staff: role '{name}' is not described in roles")
    perm_bits(cfg["staff"].get("allow"), "staff")
    for name, block in cfg["presets"].items():
        check_block(block, f"preset {name}")

    channel_names = set()
    for cat in cfg["categories"]:
        check_block(cat.get("perms"), cat["name"])
        for ch in cat.get("channels", []):
            where = f"{cat['name']} / {ch['name']}"
            ctype = ch.get("type", "text")
            if ctype not in CHANNEL_TYPES or ctype == "category":
                raise ConfigError(f"{where}: unknown type '{ctype}'")
            if CHANNEL_TYPES[ctype] in TEXT_LIKE and text_name(ch["name"]) != ch["name"]:
                raise ConfigError(f"{where}: the name must be lowercase without spaces: '{text_name(ch['name'])}'")
            if ch["name"] in channel_names:
                raise ConfigError(f"{where}: duplicate channel name")
            channel_names.add(ch["name"])
            for preset in ch.get("presets", []):
                if preset not in cfg["presets"]:
                    raise ConfigError(f"{where}: no preset '{preset}'")
            check_block(ch.get("perms"), where)
            keys = message_keys(ch)
            if CHANNEL_TYPES[ctype] in (15, 16):
                # in a forum every message is a separate post; Discord allows pinning only one
                if sum(1 for k in keys if cfg["messages"].get(k, {}).get("pin")) > 1:
                    raise ConfigError(f"{where}: only one post can be pinned in a forum")
                titles = [cfg["messages"].get(k, {}).get("title") for k in keys]
                if len(set(titles)) != len(titles):
                    raise ConfigError(f"{where}: forum posts must have different titles")
            for key in keys:
                msg = cfg["messages"].get(key)
                if msg is None:
                    raise ConfigError(f"{where}: no message '{key}' in messages")
                if not msg.get("content") and not msg.get("embeds"):
                    raise ConfigError(f"messages.{key}: content or embeds is required")
                for lang, tkey in (msg.get("translations") or {}).items():
                    if lang not in cfg.get("languages", {}):
                        raise ConfigError(f"messages.{key}.translations: language '{lang}' is not in languages")
                    if tkey not in cfg["messages"]:
                        raise ConfigError(f"messages.{key}.translations.{lang}: no message '{tkey}' in messages")
                    if len(cfg["messages"][tkey].get("content") or "") > 2000:
                        raise ConfigError(f"messages.{tkey}: content is longer than 2000 characters")
                reserve = 130 * len(msg.get("translations") or {})  # line of links to the translations
                if msg.get("translations") and len(msg.get("content") or "") + reserve > 2000:
                    raise ConfigError(f"messages.{key}: content is longer than 2000 characters including the translation links")
                if len(msg.get("content") or "") > 2000:
                    raise ConfigError(f"messages.{key}: content is longer than 2000 characters ({len(msg['content'])}) — split it into several messages")
                if CHANNEL_TYPES[ctype] in (15, 16):
                    if not msg.get("title"):
                        raise ConfigError(f"messages.{key}: a forum post needs a title")
                    unknown = set(msg.get("tags", [])) - {t["name"] for t in ch.get("tags", [])}
                    if unknown:
                        raise ConfigError(f"messages.{key}: forum {ch['name']} has no tags {sorted(unknown)}")
            if len(ch.get("tags", [])) > 20:
                raise ConfigError(f"{where}: a forum can have at most 20 tags")
            for key, table in (("layout", FORUM_LAYOUT), ("sort", FORUM_SORT)):
                if key in ch and ch[key] not in table:
                    raise ConfigError(f"{where}: {key} must be one of {list(table)}")

    # languages: language category, manager, link swap {#English channel} → its copy in the language category
    cat_names_all = {c["name"] for c in cfg["categories"]}
    for code, lang in (cfg.get("languages") or {}).items():
        if lang.get("category") and lang["category"] not in cat_names_all:
            raise ConfigError(f"languages.{code}.category: no category '{lang['category']}'")
        if lang.get("manager") and lang["manager"] not in known_roles:
            warnings.append(f"languages.{code}.manager: role '{lang['manager']}' is not described in roles")
        for src, dst in (lang.get("channels") or {}).items():
            for name in (src, dst):
                if name not in channel_names:
                    raise ConfigError(f"languages.{code}.channels: no channel '{name}'")

    for name in cfg.get("remove_channels", []):
        if name in channel_names:
            raise ConfigError(f"remove_channels: '{name}' is also described in categories")

    # was: an old name must not match the new name of another channel/role
    for role in cfg["roles"]:
        for old in role.get("was", []):
            if old in known_roles:
                raise ConfigError(f"role {role['name']}: was '{old}' — such a role is already described in roles")
    cat_names = {c["name"] for c in cfg["categories"]}
    for cat in cfg["categories"]:
        for old in cat.get("was", []):
            if old in cat_names:
                raise ConfigError(f"{cat['name']}: was '{old}' — such a category is already described")
        for ch in cat.get("channels", []):
            for old in ch.get("was", []):
                if old in channel_names:
                    raise ConfigError(f"{cat['name']} / {ch['name']}: was '{old}' — such a channel is already described")

    arch = cfg.get("archive")
    if arch:
        if arch.get("category") not in cat_names:
            raise ConfigError(f"archive.category: category '{arch.get('category')}' is not in categories")
        for name in arch.get("channels", []):
            if name in channel_names:
                raise ConfigError(f"archive: '{name}' is also described in categories")

    for name in cfg.get("remove_roles", []):
        if name in known_roles:
            raise ConfigError(f"remove_roles: '{name}' is also described in roles")
        if name == "@everyone":
            raise ConfigError("remove_roles: @everyone cannot be removed")

    am = cfg.get("automod")
    if am:
        counts = {}
        for rule in am.get("rules", []):
            where = f"automod «{rule.get('name')}»"
            trig = rule.get("trigger")
            if trig not in AUTOMOD_TRIGGERS:
                raise ConfigError(f"{where}: trigger must be one of {list(AUTOMOD_TRIGGERS)}")
            counts[trig] = counts.get(trig, 0) + 1
            if len(rule.get("keywords", [])) > 1000 or any(len(k) > 60 for k in rule.get("keywords", [])):
                raise ConfigError(f"{where}: keywords: up to 1000 words, each up to 60 characters")
            if len(rule.get("regex", [])) > 10 or any(len(r) > 260 for r in rule.get("regex", [])):
                raise ConfigError(f"{where}: regex: up to 10 expressions, each up to 260 characters")
            for x in rule.get("presets", []):
                if x not in AUTOMOD_PRESETS:
                    raise ConfigError(f"{where}: preset must be one of {list(AUTOMOD_PRESETS)}")
            for a in rule.get("actions", []):
                if isinstance(a, dict) and "timeout" in a and trig not in ("keyword", "mention_spam"):
                    raise ConfigError(f"{where}: only keyword and mention_spam rules can have a timeout")
            for r in rule.get("exempt_roles", am.get("exempt_roles", [])):
                if r not in known_roles:
                    warnings.append(f"{where}: role '{r}' is not described in roles")
        if counts.get("keyword", 0) > 6:
            raise ConfigError("automod: no more than 6 keyword rules")
        for t in ("spam", "keyword_preset", "mention_spam", "member_profile"):
            if counts.get(t, 0) > 1:
                raise ConfigError(f"automod: there can be only one {t} rule")
        if am.get("alert_channel") and am["alert_channel"] not in channel_names:
            raise ConfigError(f"automod.alert_channel: channel '{am['alert_channel']}' is not in the config")

    rs = cfg.get("rules_screening")
    if rs:
        rules = rs.get("rules") or []
        if not 1 <= len(rules) <= 16:
            raise ConfigError("rules_screening.rules: from 1 to 16 items")
        for i, rule in enumerate(rules, 1):
            if len(rule) > 300:
                raise ConfigError(f"rules_screening.rules[{i}]: longer than 300 characters")

    srv = cfg["server"]
    for key in ("rules_channel", "mod_updates_channel"):
        if srv.get(key) and srv[key] not in channel_names:
            raise ConfigError(f"server.{key}: channel '{srv[key]}' is not in the config")
    if srv.get("verification_level", "low") not in VERIFICATION:
        raise ConfigError(f"server.verification_level: must be one of {list(VERIFICATION)}")
    if srv.get("explicit_content_filter", "all") not in ("off", "no_roles", "all"):
        raise ConfigError("server.explicit_content_filter: off | no_roles | all")
    if srv.get("default_notifications", "mentions") not in NOTIFICATIONS:
        raise ConfigError(f"server.default_notifications: must be one of {list(NOTIFICATIONS)}")

    # the server's own channels (from the overlay): the script does not configure them, but onboarding can refer to them
    channel_names |= set(cfg.get("external_channels", []))
    ob = cfg.get("onboarding")
    if ob:
        for name in ob.get("default_channels", []):
            if name not in channel_names:
                raise ConfigError(f"onboarding.default_channels: channel '{name}' is not in the config")
        for prompt in ob.get("prompts", []):
            for opt in prompt.get("options", []):
                for role in opt.get("roles", []):
                    if role not in known_roles:
                        warnings.append(f"onboarding '{opt['title']}': role '{role}' is not described in roles")
                for name in opt.get("channels", []):
                    if name not in channel_names:
                        raise ConfigError(f"onboarding '{opt['title']}': channel '{name}' is not in the config")
    return cfg, warnings


def apply_perm_block(acc, block, where):
    """Applies a permission block onto acc ({role: (allow, deny)}) permission by permission, not as a whole."""
    for role, spec in (block or {}).items():
        if spec is None:
            acc.pop(role, None)
            continue
        allow, deny = acc.get(role, (0, 0))
        reset = perm_bits(spec.get("reset"), where)
        add_allow = perm_bits(spec.get("allow"), where)
        add_deny = perm_bits(spec.get("deny"), where)
        allow = (allow & ~(reset | add_deny)) | add_allow
        deny = (deny & ~(reset | add_allow)) | add_deny
        acc[role] = (allow, deny)


def resolve_perms(cfg, *blocks):
    acc = {}
    for block in blocks:
        apply_perm_block(acc, block, "perms")
    staff = cfg["staff"]
    apply_perm_block(acc, {role: {"allow": staff.get("allow", [])} for role in staff.get("roles", [])}, "staff")
    # staging (--staging ROLE): on top of everything, hide from @everyone and show only to these roles
    staging = cfg.get("_staging")
    if staging:
        block = {"@everyone": {"deny": ["view_channel"]}}
        block.update({role: {"allow": ["view_channel"]} for role in staging})
        apply_perm_block(acc, block, "staging")
    return acc


def channel_perms(cfg, cat, ch):
    presets = [cfg["presets"][p] for p in ch.get("presets", [])]
    return resolve_perms(cfg, cat.get("perms"), *presets, ch.get("perms"))


def format_perms(acc, hide):
    parts = []
    for role, (allow, deny) in acc.items():
        if role in hide:
            continue
        flags = [f"+{n}" for n, b in PERMS.items() if allow >> b & 1]
        flags += [f"-{n}" for n, b in PERMS.items() if deny >> b & 1]
        if flags:
            parts.append(f"{role}: {' '.join(flags)}")
    return "; ".join(parts)


def print_tree(cfg):
    hide = set(cfg["staff"].get("roles", []))
    print("Roles (top to bottom):")
    for role in cfg["roles"]:
        perms = ", ".join(role.get("permissions") or []) or "—"
        print(f"  {role['name']}  [{role.get('color', 'no color')}]  permissions: {perms}")
    print(f"\nModerators ({', '.join(sorted(hide))}) get in all channels: {', '.join(cfg['staff'].get('allow', []))}\n")
    for cat in cfg["categories"]:
        cat_acc = resolve_perms(cfg, cat.get("perms"))
        print(f"{cat['name']}    {format_perms(cat_acc, hide) or 'visible to everyone'}")
        for ch in cat.get("channels", []):
            acc = channel_perms(cfg, cat, ch)
            extra = "" if acc == cat_acc else f"    {format_perms(acc, hide)}"
            ctype = ch.get("type", "text")
            tags = f"  tags: {', '.join(t['name'] for t in ch['tags'])}" if ch.get("tags") else ""
            print(f"  {ch['name']}  ({ctype}){tags}{extra}")
        print()
    ob = cfg.get("onboarding")
    if ob:
        print(f"Onboarding: {len(ob.get('prompts', []))} prompts, default channels: {len(ob.get('default_channels', []))}")
        for prompt in ob.get("prompts", []):
            opts = ", ".join(f"{o['title']} → {'/'.join(o.get('roles', []))}" for o in prompt["options"])
            print(f"  {prompt['title']}: {opts}")


# ── Discord API ─────────────────────────────────────────────────────────

class Discord:
    def __init__(self, token):
        self.session = requests.Session()
        self.session.headers.update({
            "Authorization": f"Bot {token}",
            "User-Agent": "DiscordBot (server-setup-script, 1.0)",
            "X-Audit-Log-Reason": "server setup script",
        })

    def req(self, method, path, body=None):
        for _ in range(6):
            resp = self.session.request(method, API + path, json=body, timeout=30)
            if resp.status_code == 429:
                time.sleep(float(resp.json().get("retry_after", 1)) + 0.2)
                continue
            if resp.status_code >= 400:
                raise DiscordError(f"{method} {path} → {resp.status_code}: {resp.text[:600]}")
            if resp.headers.get("X-RateLimit-Remaining") == "0":
                time.sleep(float(resp.headers.get("X-RateLimit-Reset-After", 0)))
            return resp.json() if resp.content else None
        raise DiscordError(f"{method} {path}: too many retries due to rate limit")

    def get(self, path):
        return self.req("GET", path)


_snowflake_counter = itertools.count()


def snowflake():
    return str(((int(time.time() * 1000) - DISCORD_EPOCH) << 22) | (next(_snowflake_counter) & 0xFFF))


def strip_vs(emoji):
    return (emoji or "").replace("️", "")


class Ctx:
    def __init__(self, api, gid, apply):
        self.api, self.gid, self.apply = api, gid, apply
        self.role_ids = {"@everyone": gid}
        self.cat_ids = {}
        self.chan_ids = {}
        self.channels = []
        self.claimed = set()
        self.bot_id = None
        self.active_threads = None
        self.errors = []
        self.warnings = []

    def write(self, method, path, body, desc):
        print(desc)
        if not self.apply:
            return None
        return self.api.req(method, path, body)

    def fail(self, what, err):
        print(f"! ERROR {what}: {err}")
        self.errors.append(f"{what}: {err}")

    def warn(self, msg):
        print(f"! {msg}")
        self.warnings.append(msg)

    def role_id(self, name):
        if name not in self.role_ids:
            raise ConfigError(f"role '{name}' not found on the server")
        return self.role_ids[name]

    def channel_id(self, name):
        return self.chan_ids.get(name) or f"new:{name}"


# ── Roles ───────────────────────────────────────────────────────────────

def sync_roles(ctx, cfg, roles):
    by_name = {r["name"]: r for r in roles if not r.get("managed")}
    for r in roles:
        ctx.role_ids.setdefault(r["name"], r["id"])
    for spec in cfg["roles"]:
        name = spec["name"]
        want = {"name": name}
        if "permissions" in spec:
            want["permissions"] = str(perm_bits(spec["permissions"], name))
        want["color"] = int(str(spec.get("color", "0")).lstrip("#"), 16)
        want["hoist"] = bool(spec.get("hoist", False))
        want["mentionable"] = bool(spec.get("mentionable", False))
        # was: old role names; the role is renamed rather than recreated (members keep it)
        cur = by_name.get(name) or next((by_name[w] for w in spec.get("was", []) if w in by_name), None)
        try:
            if cur is None:
                res = ctx.write("POST", f"/guilds/{ctx.gid}/roles", want, f"+ role {name}")
                ctx.role_ids[name] = res["id"] if res else f"new:{name}"
            else:
                ctx.role_ids[name] = cur["id"]
                diff = {k: v for k, v in want.items() if str(cur.get(k)) != str(v)}
                if diff:
                    ctx.write("PATCH", f"/guilds/{ctx.gid}/roles/{cur['id']}", diff, f"~ role {name}: {', '.join(diff)}")
                else:
                    print(f"= role {name}")
        except DiscordError as e:
            ctx.fail(f"role {name}", e)


def sync_role_order(ctx, cfg):
    """Puts the config's roles in order within the positions they occupy; other roles are not moved."""
    roles = ctx.api.get(f"/guilds/{ctx.gid}/roles") if ctx.apply else ctx.roles
    current = {r["id"]: r["position"] for r in roles}
    ids = [ctx.role_ids.get(r["name"]) for r in cfg["roles"]]
    if not all(i in current for i in ids):
        print("~ role order will be set after the roles are created")
        return
    slots = sorted((current[i] for i in ids), reverse=True)
    updates = [{"id": i, "position": p} for i, p in zip(ids, slots) if current[i] != p]
    if not updates:
        print("= role order")
        return
    try:
        ctx.write("PATCH", f"/guilds/{ctx.gid}/roles", updates, f"~ role order: {len(updates)} changes")
    except DiscordError as e:
        ctx.fail("role order (the bot's role must be above all these roles)", e)


# ── Channels ─────────────────────────────────────────────────────────────

def find_channel(ctx, name, ctype, parent_id):
    def matches(c):
        if c["id"] in ctx.claimed:
            return False
        if ctype == 4:
            return c["type"] == 4 and c["name"] == name
        if c["type"] == 4:
            return False
        compatible = c["type"] == ctype or {c["type"], ctype} == {0, 5}
        cname = text_name(c["name"]) if c["type"] in TEXT_LIKE else c["name"]
        return compatible and cname == name
    candidates = [c for c in ctx.channels if matches(c)]
    same_parent = [c for c in candidates if c.get("parent_id") == parent_id]
    found = (same_parent or candidates or [None])[0]
    return found


def build_overwrites(ctx, acc, cur):
    out = [{"id": ctx.role_id(role), "type": 0, "allow": str(a), "deny": str(d)}
           for role, (a, d) in acc.items() if a or d]
    if cur:  # members' personal overrides are left alone
        out += [o for o in cur.get("permission_overwrites", []) if int(o["type"]) == 1]
    return out


def merge_tags(desired, cur):
    existing = {t["name"]: t for t in (cur or {}).get("available_tags", [])}
    out = []
    for t in desired:
        tag = {"name": t["name"], "emoji_id": None, "emoji_name": t.get("emoji"), "moderated": bool(t.get("moderated"))}
        old = existing.pop(t["name"], None)
        if old and old.get("id"):
            tag["id"] = old["id"]
        out.append(tag)
    # tags missing from the config are kept: existing posts may refer to them
    extra = list(existing.values())
    # Discord: at most 20 tags. The excess is cut from the new config tags (from the end); the server's old tags are left alone
    while len(out) + len(extra) > 20:
        new = [i for i, t in enumerate(out) if "id" not in t]
        if not new:
            break
        dropped = out.pop(new[-1])
        print(f"  ! tag «{dropped['name']}» not added: the forum already has 20 tags (the server's old tags are kept)")
    return out + extra


def same(key, current, wanted):
    if key == "permission_overwrites":
        norm = lambda lst: sorted((str(o["id"]), int(o["type"]), int(o["allow"]), int(o["deny"]))
                                  for o in lst or [] if int(o["allow"]) or int(o["deny"]))
        return norm(current) == norm(wanted)
    if key == "available_tags":
        norm = lambda lst: [(t["name"], strip_vs(t.get("emoji_name")), bool(t.get("moderated"))) for t in lst or []]
        return norm(current) == norm(wanted)
    if key == "default_reaction_emoji":
        return strip_vs((current or {}).get("emoji_name")) == strip_vs(wanted.get("emoji_name"))
    if key == "topic":
        return (current or "") == (wanted or "")
    return current == wanted


def sync_channel(ctx, spec, ctype, parent_id, acc):
    name = spec["name"]
    label = f"{TYPE_LABELS[ctype]} {name}"
    cur = find_channel(ctx, name, ctype, parent_id)
    renamed = None
    for old in spec.get("was", []) if cur is None else []:
        # was: old names; the channel is renamed and moved, its history is kept
        cur = find_channel(ctx, old, ctype, None) or find_channel(ctx, old, ctype, parent_id)
        if cur:
            renamed = old
            break
    want = {"type": ctype, "permission_overwrites": build_overwrites(ctx, acc, cur)}
    if renamed:
        want["name"] = name
        label += f" (was {renamed})"
    if ctype != 4:
        want["parent_id"] = parent_id
    if "topic" in spec and ctype in TEXT_LIKE:
        want["topic"] = spec["topic"].strip()
    if "slowmode" in spec:
        want["rate_limit_per_user"] = int(spec["slowmode"])
    if "auto_archive" in spec and ctype in (0, 5, 15, 16):
        # minutes of inactivity before a thread is archived: 60, 1440, 4320 or 10080
        minutes = int(spec["auto_archive"])
        if minutes not in (60, 1440, 4320, 10080):
            raise ConfigError(f"{name}: auto_archive must be 60, 1440, 4320 or 10080 minutes")
        want["default_auto_archive_duration"] = minutes
    if "user_limit" in spec and ctype == 2:
        want["user_limit"] = int(spec["user_limit"])
    if ctype in (15, 16):
        want["available_tags"] = merge_tags(spec.get("tags", []), cur)
        flags = (cur or {}).get("flags", 0)
        want["flags"] = flags | REQUIRE_TAG if spec.get("require_tag") else flags & ~REQUIRE_TAG
        if "default_reaction" in spec:
            want["default_reaction_emoji"] = {"emoji_id": None, "emoji_name": spec["default_reaction"]}
        if "layout" in spec and ctype == 15:
            want["default_forum_layout"] = FORUM_LAYOUT[spec["layout"]]
        if "sort" in spec:
            want["default_sort_order"] = FORUM_SORT[spec["sort"]]

    if cur is None:
        body = {"name": name, **want}
        flags = body.pop("flags", 0)  # flags are not accepted on create, so they are set by a follow-up request
        res = ctx.write("POST", f"/guilds/{ctx.gid}/channels", body, f"+ {label}")
        if res is None:
            return f"new:{name}"
        ctx.claimed.add(res["id"])
        if flags != res.get("flags", 0):
            ctx.api.req("PATCH", f"/channels/{res['id']}", {"flags": flags})
        return res["id"]

    ctx.claimed.add(cur["id"])
    diff = {k: v for k, v in want.items() if not same(k, cur.get(k), v)}
    if "type" in diff and {cur["type"], ctype} != {0, 5}:
        ctx.warn(f"{label}: the channel on the server has a different type; Discord cannot change it, recreate it manually")
        del diff["type"]
    if diff:
        ctx.write("PATCH", f"/channels/{cur['id']}", diff, f"~ {label}: {', '.join(diff)}")
    else:
        print(f"= {label}")
    return cur["id"]


def sync_one(ctx, cfg, cat, ch, ctype=None):
    ctype = ctype if ctype is not None else CHANNEL_TYPES[ch.get("type", "text")]
    parent_id = ctx.cat_ids.get(cat["name"])
    try:
        ctx.chan_ids[ch["name"]] = sync_channel(ctx, ch, ctype, parent_id, channel_perms(cfg, cat, ch))
    except DiscordError as e:
        ctx.fail(f"{TYPE_LABELS[ctype]} {ch['name']}", e)


def sync_layout(ctx, cfg, community):
    """Creates/updates categories and channels. Returns the channels that are waiting for Community mode."""
    pending = []
    for cat in cfg["categories"]:
        try:
            ctx.cat_ids[cat["name"]] = sync_channel(ctx, cat, 4, None, resolve_perms(cfg, cat.get("perms")))
        except DiscordError as e:
            ctx.fail(f"category {cat['name']}", e)
            continue
        for ch in cat.get("channels", []):
            if CHANNEL_TYPES[ch.get("type", "text")] in NEEDS_COMMUNITY and not community:
                pending.append((cat, ch))
            else:
                sync_one(ctx, cfg, cat, ch)
    return pending


def sync_positions(ctx, cfg):
    channels = ctx.api.get(f"/guilds/{ctx.gid}/channels") if ctx.apply else ctx.channels
    current = {c["id"]: c["position"] for c in channels}
    ours = [ctx.cat_ids[c["name"]] for c in cfg["categories"] if c["name"] in ctx.cat_ids]
    others = [c["id"] for c in sorted(channels, key=lambda c: c["position"]) if c["type"] == 4 and c["id"] not in ours]
    # in staging (and with _old_first) new categories go below the existing ones so the server is not rearranged
    cat_order = others + ours if cfg.get("_staging") or cfg.get("_old_first") else ours + others
    updates = [{"id": cid, "position": i} for i, cid in enumerate(cat_order) if current.get(cid) != i]
    for cat in cfg["categories"]:
        cat_id = ctx.cat_ids.get(cat["name"])
        if not cat_id:
            continue
        kids = [ctx.chan_ids[ch["name"]] for ch in cat.get("channels", []) if ch["name"] in ctx.chan_ids]
        kids += [c["id"] for c in sorted(channels, key=lambda c: c["position"])
                 if c.get("parent_id") == cat_id and c["id"] not in kids]
        updates += [{"id": cid, "position": i} for i, cid in enumerate(kids) if current.get(cid) != i]
    if not updates:
        print("= channel order")
        return
    try:
        ctx.write("PATCH", f"/guilds/{ctx.gid}/channels", updates, f"~ channel order: {len(updates)} changes")
    except DiscordError as e:
        ctx.fail("channel order", e)


# ── Server, messages, onboarding ───────────────────────────────────────

def enable_community(ctx, cfg, guild):
    srv = cfg["server"]
    level = max(VERIFICATION[srv.get("verification_level", "low")], 1)
    body = {
        "features": list(guild["features"]) + ["COMMUNITY"],
        "verification_level": level,
        "explicit_content_filter": 2,
        "rules_channel_id": ctx.channel_id(srv["rules_channel"]),
        "public_updates_channel_id": ctx.channel_id(srv["mod_updates_channel"]),
    }
    ctx.write("PATCH", f"/guilds/{ctx.gid}", body, "~ server: enable Community mode")


CONTENT_FILTER = {"off": 0, "no_roles": 1, "all": 2}


def sync_server_settings(ctx, cfg, guild, community):
    srv = cfg["server"]
    want = {}
    if "verification_level" in srv:
        want["verification_level"] = max(VERIFICATION[srv["verification_level"]], 1 if community else 0)
    if "default_notifications" in srv:
        want["default_message_notifications"] = NOTIFICATIONS[srv["default_notifications"]]
    if "preferred_locale" in srv:
        want["preferred_locale"] = srv["preferred_locale"]
    if community:
        if "description" in srv:
            want["description"] = srv["description"]
        if srv.get("rules_channel"):
            want["rules_channel_id"] = ctx.channel_id(srv["rules_channel"])
        if srv.get("mod_updates_channel"):
            want["public_updates_channel_id"] = ctx.channel_id(srv["mod_updates_channel"])
        if srv.get("safety_alerts_channel"):
            want["safety_alerts_channel_id"] = ctx.channel_id(srv["safety_alerts_channel"])
    if "explicit_content_filter" in srv:
        want["explicit_content_filter"] = CONTENT_FILTER[srv["explicit_content_filter"]]
    if "boost_progress_bar" in srv:
        want["premium_progress_bar_enabled"] = bool(srv["boost_progress_bar"])
    diff = {k: v for k, v in want.items() if guild.get(k) != v}
    if not diff:
        print("= server settings")
        return
    try:
        ctx.write("PATCH", f"/guilds/{ctx.gid}", diff, f"~ server settings: {', '.join(diff)}")
    except DiscordError as e:
        ctx.fail("server settings", e)


CHANNEL_LINK = re.compile(r"\{#([^}]+)\}")
ROLE_MENTION = re.compile(r"\{@([^}]+)\}")


def render_message(ctx, msg, server_name, swap=None):
    """swap — {English channel: copy}: in a language category, links point to its copies of the channels"""
    def fmt(s):
        if not isinstance(s, str):
            return s
        def link(m):
            name = (swap or {}).get(m[1], m[1])
            cid = ctx.channel_id(name)
            return f"#{name}" if cid.startswith("new:") else f"<#{cid}>"
        def role(m):
            rid = ctx.role_ids.get(m[1])
            return f"<@&{rid}>" if rid and not str(rid).startswith("new:") else f"@{m[1]}"
        s = CHANNEL_LINK.sub(link, s.replace("{server}", server_name))
        return ROLE_MENTION.sub(role, s).strip()

    def embed(e):
        out = {k: fmt(v) for k, v in e.items() if k not in ("color", "fields")}
        if "color" in e:
            out["color"] = int(str(e["color"]).lstrip("#"), 16)
        if "fields" in e:
            out["fields"] = [{"name": fmt(f["name"]), "value": fmt(f["value"]), "inline": bool(f.get("inline"))}
                             for f in e["fields"]]
        return out

    body = {}
    if msg.get("content"):
        body["content"] = fmt(msg["content"])
    if msg.get("embeds"):
        body["embeds"] = [embed(e) for e in msg["embeds"]]
    elif not msg.get("previews"):
        body["flags"] = SUPPRESS_EMBEDS  # plain text: no link previews unless previews: true is set
    body["allowed_mentions"] = {"parse": []}  # {@role} is a role link without a ping
    return body


def message_keys(ch):
    """message: a key or a list of keys, i.e. several messages in a row (e.g. EN and RU separately)"""
    keys = ch.get("message") or []
    return [keys] if isinstance(keys, str) else list(keys)


def text_channel_state(ctx, cid):
    """Channel state before publishing: whether it is empty, whether only the bot posts there, the bot's messages (oldest first),
    the bot's pinned messages"""
    if cid.startswith("new:"):
        return {"empty": True, "only_bot": True, "mine": [], "pins": []}
    recent = ctx.api.get(f"/channels/{cid}/messages?limit=100")
    pins = [m for m in ctx.api.get(f"/channels/{cid}/pins") if m["author"]["id"] == ctx.bot_id]
    # system messages (pins etc.) are not counted
    real = [m for m in recent if m.get("type", 0) in (0, 19)]
    return {"empty": not recent, "only_bot": all(m["author"]["id"] == ctx.bot_id for m in real),
            "mine": [m for m in reversed(real) if m["author"]["id"] == ctx.bot_id], "pins": pins}


def edit_body(body):
    """PATCH body that replaces a bot message's text (buttons and the translations line are set later by sync_translation_links)"""
    return {"content": body.get("content", ""), "embeds": body.get("embeds", []),
            "flags": body.get("flags", 0), "allowed_mentions": {"parse": []}}


def edit_message(ctx, cid, old, body, label):
    res = ctx.write("PATCH", f"/channels/{cid}/messages/{old['id']}", edit_body(body), f"~ message {label}: text updated")
    if res is None:
        return dict(old, _stale=True)
    res.setdefault("thread", old.get("thread"))
    return res


TRANSLATION_FOOTER = "\n-# 🌍 "  # start of the translations line (also matches the old form without «Translate»)
TRANSLATION_LABEL = "**Translate:** "  # bold word before the links, next to the language buttons


def find_same(messages, body):
    """The bot message with the same text (ignoring the trailing translation-links line); used to find
    the translations thread of an already published message"""
    want = body.get("content", "")

    def same_text(m):
        text = m.get("content") or ""
        return text == want or bool(want and text.startswith(want + TRANSLATION_FOOTER))
    return next((m for m in messages if same_text(m) and len(m.get("embeds", [])) == len(body.get("embeds", []))), None)


def post_text_message(ctx, cid, msg, body, label, state, idx=0):
    """Returns the message (new, updated or already published) or None.
    Text changed in the config → the bot's message is edited in place (the translations thread and buttons are kept):
    pinned: the bot's first pin; regular: the bot's idx-th message, if only the bot posts in the channel."""
    if msg.get("pin") and state["pins"]:
        same = find_same(state["pins"], body)
        if same:
            print(f"= message {label} (already pinned)")
            return same
        return edit_message(ctx, cid, state["pins"][0], body, label)
    if not msg.get("pin") and not state["empty"]:
        same = find_same(state["mine"], body)
        if same:
            print(f"= message {label}")
            return same
        unpinned = [m for m in state["mine"] if not m.get("pinned")]
        if not state["only_bot"]:
            print(f"= message {label} (people post in this channel, leaving it alone)")
            return None
        if idx < len(unpinned):
            return edit_message(ctx, cid, unpinned[idx], body, label)
        # the series got longer: the new message goes at the end
    res = ctx.write("POST", f"/channels/{cid}/messages", body,
                    f"+ message {label}" + (" (pinned)" if msg.get("pin") else ""))
    if res and msg.get("pin"):
        ctx.api.req("PUT", f"/channels/{cid}/pins/{res['id']}")
    return res or {"id": "new", "new": True}


def post_forum_example(ctx, cid, msg, body, label):
    """Returns the id of the post's thread (new or existing) or None"""
    title = msg["title"]
    tag_ids = []
    if not cid.startswith("new:"):
        if ctx.active_threads is None:
            ctx.active_threads = ctx.api.get(f"/guilds/{ctx.gid}/threads/active")["threads"]
        archived = ctx.api.get(f"/channels/{cid}/threads/archived/public")["threads"]
        found = next((t for t in ctx.active_threads + archived if t.get("parent_id") == cid and t["name"] == title), None)
        if found:
            starter = ctx.api.get(f"/channels/{found['id']}/messages/{found['id']}")
            if find_same([starter], body):
                print(f"= post {label}")
            else:
                ctx.write("PATCH", f"/channels/{found['id']}/messages/{found['id']}", edit_body(body),
                          f"~ post {label}: text updated")
            return found["id"]
        forum = ctx.api.get(f"/channels/{cid}")
        tag_ids = [t["id"] for t in forum.get("available_tags", []) if t["name"] in msg.get("tags", [])]
    thread = {"name": title, "message": body, "applied_tags": tag_ids}
    res = ctx.write("POST", f"/channels/{cid}/threads", thread,
                    f"+ post {label}" + (" (pinned)" if msg.get("pin") else ""))
    if res and msg.get("pin"):
        ctx.api.req("PATCH", f"/channels/{res['id']}", {"flags": PINNED_THREAD})
    return res["id"] if res else "new"


def translation_bodies(ctx, cfg, msg, server_name):
    """translations: {language: key} → [(language, message body)] in languages order"""
    tr = msg.get("translations") or {}
    return [(lang, render_message(ctx, cfg["messages"][tr[lang]], server_name)) for lang in cfg.get("languages", {}) if lang in tr]


def sync_translations(ctx, cfg, thread_id, bodies, label):
    """Translations are bot messages in the thread; missing ones are added, existing ones are left alone.
    Returns {language: id of the translation message} (in preview mode, only the already published ones)."""
    # the bot's messages in the thread, oldest first; a forum post's starter message (id == thread id) is not a translation.
    # Translations follow languages order: changed text is edited in place by position (new languages go at the end).
    have = [] if thread_id == "new" else [m for m in reversed(ctx.api.get(f"/channels/{thread_id}/messages?limit=100"))
                                          if m["author"]["id"] == ctx.bot_id and m["id"] != thread_id
                                          and m.get("type", 0) in (0, 19)]
    ids = {}
    for i, (lang, body) in enumerate(bodies):
        found = find_same(have, body)
        if found:
            print(f"= translation {lang} {label}")
            ids[lang] = found["id"]
        elif thread_id == "new":
            print(f"+ translation {lang} {label}")
        elif i < len(have) and not any(find_same([have[i]], b) for _, b in bodies):
            ctx.write("PATCH", f"/channels/{thread_id}/messages/{have[i]['id']}", edit_body(body),
                      f"~ translation {lang} {label}: text updated")
            ids[lang] = have[i]["id"]
        else:
            res = ctx.write("POST", f"/channels/{thread_id}/messages", body, f"+ translation {lang} {label}")
            if res:
                ids[lang] = res["id"]
    return ids


def sync_translation_links(ctx, cfg, cid, posted, body, thread_id, ids, label):
    """Under the original: a line of translation links (works without the bot) and language buttons; the bot replies with a translation
    that only the clicker sees (custom_id tr:<thread>:<message>, handler in bot/src/modules/translate.rs)"""
    langs = cfg.get("languages", {})
    order = [lang for lang in langs if lang in ids]
    if not order:
        print(f"+ translation links and buttons under {label}")
        return
    link = lambda lang: f"https://discord.com/channels/{ctx.gid}/{thread_id}/{ids[lang]}"
    content = body.get("content", "") + TRANSLATION_FOOTER + TRANSLATION_LABEL + " · ".join(
        f"{langs[lang]['flag']} [{langs[lang]['name']}]({link(lang)})" for lang in order)
    buttons = [{"type": 2, "style": 2, "custom_id": f"tr:{thread_id}:{ids[lang]}", "label": langs[lang]["name"],
                "emoji": {"name": langs[lang]["flag"]}} for lang in order]
    rows = [{"type": 1, "components": buttons[i:i + 5]} for i in range(0, len(buttons), 5)]
    cur_ids = [c.get("custom_id") for row in posted.get("components", []) for c in row.get("components", [])]
    if posted.get("content") == content and cur_ids == [b["custom_id"] for b in buttons]:
        print(f"= translation links and buttons under {label}")
        return
    ctx.write("PATCH", f"/channels/{cid}/messages/{posted['id']}",
              {"content": content, "components": rows, "flags": SUPPRESS_EMBEDS, "allowed_mentions": {"parse": []}},
              f"~ translation links and buttons under {label}: {', '.join(order)}")


def translation_thread_name(cfg, bodies):
    langs = cfg.get("languages", {})
    return "🌍 " + " · ".join(f"{langs[l]['flag']} {langs[l]['name']}" for l, _ in bodies)


def sync_message_translations(ctx, cfg, cid, posted, bodies, label):
    """The «🌍 …» thread under a message (a message can have only one thread, so all languages go in it)"""
    name = translation_thread_name(cfg, bodies)
    if posted is None:
        ctx.warn(f"{label}: published message not found, translations skipped (the text in the channel differs from the config)")
        return
    if posted.get("new"):  # preview mode: the message does not exist yet
        print(f"+ thread {name} under {label}")
        sync_translations(ctx, cfg, "new", bodies, label)
        return print(f"+ translation links and buttons under {label}")
    thread = posted.get("thread")
    if thread is None:
        thread = ctx.write("POST", f"/channels/{cid}/messages/{posted['id']}/threads",
                           {"name": name, "auto_archive_duration": 10080}, f"+ thread {name} under {label}")
        if not thread:  # preview mode
            sync_translations(ctx, cfg, "new", bodies, label)
            return print(f"+ translation links and buttons under {label}")
    elif thread["name"] != name:
        ctx.write("PATCH", f"/channels/{thread['id']}", {"name": name}, f"~ thread {thread['name']} → {name}")
    ids = sync_translations(ctx, cfg, thread["id"], bodies, label)
    sync_translation_links(ctx, cfg, cid, posted, posted["_body"], thread["id"], ids, label)


def language_swap(cfg, cat_name):
    """Link swap for a language category (languages.<code>.channels) or None"""
    for lang in (cfg.get("languages") or {}).values():
        if lang.get("category") == cat_name:
            return lang.get("channels") or {}
    return None


def sync_messages(ctx, cfg, server_name):
    for cat in cfg["categories"]:
        swap = language_swap(cfg, cat["name"])
        for ch in cat.get("channels", []):
            keys = message_keys(ch)
            cid = ctx.chan_ids.get(ch["name"])
            if not keys or not cid:
                continue
            forum = CHANNEL_TYPES[ch.get("type", "text")] in (15, 16)
            try:
                # channel state is read once before publishing so a series of messages is posted as a whole
                state = None if forum else text_channel_state(ctx, cid)
                for idx, key in enumerate(keys):
                    msg = cfg["messages"][key]
                    label = f"'{key}' in {ch['name']}"
                    body = render_message(ctx, msg, server_name, swap)
                    bodies = translation_bodies(ctx, cfg, msg, server_name)
                    if forum:
                        # in a forum, translations are the following messages in the post itself
                        thread_id = post_forum_example(ctx, cid, msg, body, label)
                        if bodies and thread_id:
                            sync_translations(ctx, cfg, thread_id, bodies, label)
                    else:
                        posted = post_text_message(ctx, cid, msg, body, label, state, idx)
                        if posted is not None:
                            posted["_body"] = body
                        if bodies:
                            sync_message_translations(ctx, cfg, cid, posted, bodies, label)
            except DiscordError as e:
                ctx.fail(f"message in {ch['name']}", e)


ARCHIVABLE = {0, 2, 5, 13, 15, 16}  # text, voice, announcement, stage, forum, media


def sync_archive(ctx, cfg):
    """archive: channels move to the archive category, visible to the team only and read-only. Nothing is deleted."""
    arch = cfg.get("archive") or {}
    cat_id = ctx.cat_ids.get(arch.get("category"))
    if not arch.get("channels"):
        return
    if not cat_id:
        ctx.warn(f"archive category '{arch.get('category')}' not found in categories")
        return
    cat = next(c for c in cfg["categories"] if c["name"] == arch["category"])
    acc = resolve_perms(cfg, cat.get("perms"))
    read_only = perm_bits(["send_messages", "send_messages_in_threads", "create_public_threads", "create_private_threads",
                           "add_reactions", "connect"], "archive")
    view = perm_bits(["view_channel", "read_message_history"], "archive")
    for role in cfg["staff"].get("roles", []):
        acc[role] = (view, read_only)
    for name in arch["channels"]:
        # regular channels only: Discord's service channels (e.g. type 20 «server-hub», which Discord creates itself) are left alone
        found = [c for c in ctx.channels if c["name"] == name and c["type"] in ARCHIVABLE and c["id"] not in ctx.claimed]
        odd = [c for c in ctx.channels if c["name"] == name and c["type"] not in ARCHIVABLE and c["type"] != 4]
        for c in odd:
            print(f"= archive: {name} is a Discord service channel (type {c['type']}), leaving it alone")
        if not found:
            print(f"= archive: {name} (not on the server)")
            continue
        for c in found:
            ctx.claimed.add(c["id"])
            want = {"parent_id": cat_id, "permission_overwrites": build_overwrites(ctx, acc, c)}
            diff = {k: v for k, v in want.items() if not same(k, c.get(k), v)}
            if not diff:
                print(f"= archive: {name}")
                continue
            try:
                ctx.write("PATCH", f"/channels/{c['id']}", diff, f"~ to archive: {TYPE_LABELS.get(c['type'], 'channel')} {name}")
            except DiscordError as e:
                ctx.fail(f"archive {name}", e)


def sync_role_removals(ctx, cfg, roles):
    for name in cfg.get("remove_roles", []):
        found = [r for r in roles if r["name"] == name and not r.get("managed")]
        if not found:
            print(f"= role {name} (already gone)")
            continue
        for r in found:
            try:
                ctx.write("DELETE", f"/guilds/{ctx.gid}/roles/{r['id']}", None, f"- delete role {name} ({r['id']})")
            except DiscordError as e:
                ctx.fail(f"deleting role {name}", e)


def sync_removals(ctx, cfg):
    for name in cfg.get("remove_channels", []):
        found = [c for c in ctx.channels if c["name"] == name and c["id"] not in ctx.claimed]
        if not found:
            print(f"= {name} (already deleted)")
            continue
        for c in found:
            ctx.claimed.add(c["id"])
            try:
                ctx.write("DELETE", f"/channels/{c['id']}", None, f"- delete {TYPE_LABELS.get(c['type'], 'channel')} {name}")
            except DiscordError as e:
                ctx.fail(f"deleting {name}", e)


AUTOMOD_TRIGGERS = {"keyword": 1, "spam": 3, "keyword_preset": 4, "mention_spam": 5, "member_profile": 6}
AUTOMOD_SINGLE = {3, 4, 5, 6}  # a server can have only one trigger of this kind
AUTOMOD_PRESETS = {"profanity": 1, "sexual_content": 2, "slurs": 3}


def automod_body(ctx, am, rule):
    """Config rule → Discord API body"""
    trig = AUTOMOD_TRIGGERS[rule["trigger"]]
    meta = {}
    if trig in (1, 6):
        meta = {"keyword_filter": rule.get("keywords", []), "regex_patterns": rule.get("regex", []),
                "allow_list": rule.get("allow", [])}
    elif trig == 4:
        meta = {"presets": [AUTOMOD_PRESETS[x] for x in rule.get("presets", [])], "allow_list": rule.get("allow", [])}
    elif trig == 5:
        meta = {"mention_total_limit": int(rule.get("limit", 5)),
                "mention_raid_protection_enabled": bool(rule.get("raid_protection", True))}
    alert = ctx.chan_ids.get(am.get("alert_channel")) if am.get("alert_channel") else None
    actions = []
    for a in rule.get("actions", ["block", "alert"]):
        if a == "block":
            actions.append({"type": 4} if trig == 6 else
                           {"type": 1, "metadata": {"custom_message": am.get("block_message", "")[:150]}})
        elif a == "alert" and alert and not str(alert).startswith("new:"):
            actions.append({"type": 2, "metadata": {"channel_id": alert}})
        elif isinstance(a, dict) and "timeout" in a:
            actions.append({"type": 3, "metadata": {"duration_seconds": int(a["timeout"])}})
    exempt_roles = [ctx.role_ids[r] for r in rule.get("exempt_roles", am.get("exempt_roles", []))
                    if r in ctx.role_ids and not str(ctx.role_ids[r]).startswith("new:")]
    exempt_channels = [ctx.chan_ids[c] for c in rule.get("exempt_channels", am.get("exempt_channels", []))
                       if c in ctx.chan_ids and not str(ctx.chan_ids[c]).startswith("new:")]
    return {"name": rule["name"], "event_type": 2 if trig == 6 else 1, "trigger_type": trig,
            "trigger_metadata": meta, "actions": actions, "enabled": rule.get("enabled", True),
            "exempt_roles": exempt_roles, "exempt_channels": exempt_channels}


def describe_automod(rule):
    """Short summary of what to enable manually"""
    parts = []
    if rule.get("limit"):
        parts.append(f"mention limit {rule['limit']}")
    if rule.get("raid_protection"):
        parts.append("mention raid protection")
    for a in rule.get("actions", []):
        if a == "block":
            parts.append("block the message")
        elif a == "alert":
            parts.append("alert in mod-log")
        elif isinstance(a, dict) and "timeout" in a:
            parts.append(f"timeout {int(a['timeout']) // 60} min")
    return ", ".join(parts)


def automod_norm(r):
    """Rule fields to compare (order and empty values do not matter)"""
    meta = {k: (sorted(v) if isinstance(v, list) else v) for k, v in (r.get("trigger_metadata") or {}).items()
            if v not in ([], None, "")}
    acts = sorted((a["type"], tuple(sorted((k, str(v)) for k, v in (a.get("metadata") or {}).items() if v not in ("", None))))
                  for a in r.get("actions", []))
    return (r["name"], r["trigger_type"], meta, acts, bool(r.get("enabled")),
            sorted(map(str, r.get("exempt_roles", []))), sorted(map(str, r.get("exempt_channels", []))))


def sync_automod(ctx, cfg):
    am = cfg["automod"]
    try:
        current = ctx.api.get(f"/guilds/{ctx.gid}/auto-moderation/rules")
    except DiscordError as e:
        return ctx.fail("automod: reading rules (the bot needs the Manage Server permission)", e)
    used = set()
    for rule in am.get("rules", []):
        want = automod_body(ctx, am, rule)
        cur = next((r for r in current if r["name"] == rule["name"] and r["id"] not in used), None)
        if cur is None and want["trigger_type"] in AUTOMOD_SINGLE:
            cur = next((r for r in current if r["trigger_type"] == want["trigger_type"] and r["id"] not in used), None)
        label = f"automod «{rule['name']}»"
        try:
            if cur is None:
                ctx.write("POST", f"/guilds/{ctx.gid}/auto-moderation/rules", want, f"+ {label}")
                continue
            used.add(cur["id"])
            if automod_norm(cur) == automod_norm(want):
                print(f"= {label}")
                continue
            body = {k: v for k, v in want.items() if k != "trigger_type"}  # the trigger type cannot be changed
            was = f" (was «{cur['name']}»)" if cur["name"] != rule["name"] else ""
            try:
                ctx.write("PATCH", f"/guilds/{ctx.gid}/auto-moderation/rules/{cur['id']}", body, f"~ {label}{was}")
            except DiscordError as e:
                # a Discord system rule (created before the automod API): the bot cannot write to it,
                # and it cannot be deleted on a community server, only edited manually in the server settings
                if "404" not in str(e) or cur.get("creator_id") == ctx.bot_id:
                    raise
                ctx.warn(f"{label}: the bot cannot change the system rule «{cur['name']}», configure it manually "
                         f"(Server Settings → AutoMod): {describe_automod(rule)}")
        except DiscordError as e:
            ctx.fail(label, e)
    for r in current:
        if r["id"] not in used and r["name"] not in {x["name"] for x in am.get("rules", [])}:
            print(f"= automod «{r['name']}» is not in the config, leaving it alone")


def sync_rules_screening(ctx, cfg, guild):
    """Join rules (Rules Screening): the member-verification endpoint is not in the Discord documentation,
    but the client itself uses it; a bot with the Manage Server permission can change it"""
    rs = cfg["rules_screening"]
    want_fields = [{"field_type": "TERMS", "label": "Read and agree to the server rules", "required": True,
                    "values": list(rs.get("rules", []))}]
    try:
        cur = ctx.api.get(f"/guilds/{ctx.gid}/member-verification")
    except DiscordError:
        cur = {}
    enabled = "MEMBER_VERIFICATION_GATE_ENABLED" in guild.get("features", [])
    cur_rules = next((f.get("values") for f in cur.get("form_fields", []) if f.get("field_type") == "TERMS"), None)
    want_enabled = bool(rs.get("enabled", True))
    if cur_rules == want_fields[0]["values"] and cur.get("description") == rs.get("description") and enabled == want_enabled:
        print(f"= join rules: {len(cur_rules)} items, enabled: {enabled}")
        return
    body = {"enabled": want_enabled, "description": rs.get("description"), "form_fields": want_fields}
    try:
        ctx.write("PATCH", f"/guilds/{ctx.gid}/member-verification", body,
                  f"~ join rules: {len(want_fields[0]['values'])} items, enabled: {want_enabled}")
    except DiscordError as e:
        ctx.fail("join rules", e)


def sync_onboarding(ctx, cfg):
    ob = cfg.get("onboarding")
    if not ob:
        return
    try:
        cur = ctx.api.get(f"/guilds/{ctx.gid}/onboarding")
    except DiscordError:
        cur = {}
    old_prompts = {p["title"]: p for p in cur.get("prompts", [])}
    prompts = []
    for p in ob.get("prompts", []):
        old = old_prompts.get(p["title"], {})
        old_opts = {o["title"]: o for o in old.get("options", [])}
        options = [{
            "id": old_opts.get(o["title"], {}).get("id") or snowflake(),
            "title": o["title"],
            "description": o.get("description"),
            "emoji_id": None,
            "emoji_name": o.get("emoji"),
            "emoji_animated": False,
            "role_ids": [ctx.role_id(r) for r in o.get("roles", [])],
            "channel_ids": [ctx.channel_id(c) for c in o.get("channels", [])],
        } for o in p.get("options", [])]
        prompts.append({
            "id": old.get("id") or snowflake(),
            "type": 1 if p.get("style") == "dropdown" else 0,
            "title": p["title"],
            "options": options,
            "single_select": bool(p.get("single_select", False)),
            "required": bool(p.get("required", True)),
            "in_onboarding": bool(p.get("in_onboarding", True)),
        })
    body = {
        "prompts": prompts,
        "default_channel_ids": [ctx.channel_id(c) for c in ob.get("default_channels", [])],
        "enabled": bool(ob.get("enabled", True)),
        "mode": 0,
    }
    desc = f"~ onboarding: {len(prompts)} prompts, {len(body['default_channel_ids'])} default channels"
    try:
        ctx.write("PUT", f"/guilds/{ctx.gid}/onboarding", body, desc + f", enabled: {body['enabled']}")
    except DiscordError as e:
        if not body["enabled"] or cur.get("enabled"):
            # an onboarding that already works is not turned off because of a failed update
            ctx.fail("onboarding (the current setting on the server was not changed)", e)
            return
        ctx.warn(f"Discord refused to enable onboarding: {e}")
        body["enabled"] = False
        try:
            ctx.write("PUT", f"/guilds/{ctx.gid}/onboarding", body, "~ onboarding: saving the prompts with onboarding disabled")
            ctx.warn("Prompts saved. Enable onboarding manually: Server Settings → Onboarding.")
        except DiscordError as e2:
            ctx.fail("onboarding", e2)


# ── main ────────────────────────────────────────────────────────────────

def load_env(path):
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip().strip("\"'"))


def check_bot(ctx, roles):
    me = ctx.api.get("/users/@me")
    ctx.bot_id = me["id"]
    member = ctx.api.get(f"/guilds/{ctx.gid}/members/{me['id']}")
    perms = 0
    for r in roles:
        if r["id"] in member["roles"] or r["id"] == ctx.gid:
            perms |= int(r["permissions"])
    print(f"Bot: {me['username']}")
    if not perms & (1 << PERMS["administrator"]):
        ctx.warn("The bot lacks the Administrator permission. Without it, the bot loses access to the hidden channels "
                 "it creates and cannot grant moderators permissions it does not have itself.")


def run(cfg, api, gid, apply):
    ctx = Ctx(api, gid, apply)
    guild = api.get(f"/guilds/{gid}")
    roles = api.get(f"/guilds/{gid}/roles")
    ctx.roles = roles
    ctx.channels = api.get(f"/guilds/{gid}/channels")
    mode = "APPLY" if apply else "PREVIEW — nothing changes; add --apply to apply"
    print(f"Server: {guild['name']} ({gid})\nMode: {mode}")
    if cfg.get("_overlay"):
        print(f"Overlay: {cfg['_overlay']}")
    for name in cfg.get("external_channels", []):
        found = next((c for c in ctx.channels if c["name"] == name), None)
        if found:
            ctx.chan_ids[name] = found["id"]
            ctx.claimed.add(found["id"])  # the server's own channel: not configured, only referenced
        else:
            ctx.warn(f"own channel '{name}' from the overlay not found on the server")
    staging = cfg.get("_staging")
    if staging:
        print(f"STAGING: new items are visible only to {', '.join(staging)} (and roles with Administrator). "
              "Community, server settings and onboarding are not changed.")
    check_bot(ctx, roles)

    print("\n── Roles")
    sync_roles(ctx, cfg, roles)

    print("\n── Categories and channels")
    community = "COMMUNITY" in guild["features"]
    pending = sync_layout(ctx, cfg, community)

    if not staging:
        print("\n── Server")
        if cfg["server"].get("community") and not community:
            try:
                enable_community(ctx, cfg, guild)
                community = True
                if apply:
                    guild = api.get(f"/guilds/{gid}")
            except DiscordError as e:
                ctx.fail("Community mode", e)
        sync_server_settings(ctx, cfg, guild, community)

    if pending:
        print("\n── Channels that need Community mode")
        for cat, ch in pending:
            ctype = CHANNEL_TYPES[ch.get("type", "text")]
            if community:
                sync_one(ctx, cfg, cat, ch)
            elif staging and ctype == CHANNEL_TYPES["announcement"]:
                # a regular text channel for now; it becomes an announcement channel on a run without --staging (type 0 → 5 changes in place)
                sync_one(ctx, cfg, cat, ch, CHANNEL_TYPES["text"])
            else:
                ctx.warn(f"{TYPE_LABELS[ctype]} {ch['name']} skipped: Community mode is not enabled")

    if cfg.get("archive"):
        print("\n── Archive")
        sync_archive(ctx, cfg)

    print("\n── Order")
    sync_role_order(ctx, cfg)
    sync_positions(ctx, cfg)

    print("\n── Messages")
    sync_messages(ctx, cfg, guild["name"])

    if community and not staging:
        print("\n── Onboarding")
        sync_onboarding(ctx, cfg)
        if cfg.get("rules_screening"):
            print("\n── Join rules")
            sync_rules_screening(ctx, cfg, guild)

    if cfg.get("automod") and not staging:
        print("\n── Automod")
        sync_automod(ctx, cfg)

    if (cfg.get("remove_channels") or cfg.get("remove_roles")) and not staging:
        print("\n── Deletion")
        sync_removals(ctx, cfg)
        sync_role_removals(ctx, cfg, roles)

    extra = [c["name"] for c in ctx.channels if c["id"] not in ctx.claimed and c["type"] not in (10, 11, 12)]
    if extra:
        print(f"\nThe server has channels that are not in the config (leaving them alone): {', '.join(extra)}")

    print(f"\nDone. Errors: {len(ctx.errors)}, warnings: {len(ctx.warnings)}")
    return 1 if ctx.errors else 0


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser(description="Set up a Discord server from a YAML config")
    ap.add_argument("--config", default=str(HERE / "server.yaml"))
    ap.add_argument("--apply", action="store_true", help="apply the changes (without the flag, only the plan is shown)")
    ap.add_argument("--offline", action="store_true", help="check the config without connecting to Discord")
    ap.add_argument("--guild", help="server ID (default: DISCORD_GUILD_ID from .env)")
    ap.add_argument("--staging", action="append", metavar="ROLE",
                    help="show new items only to this role (can be repeated); everything else is hidden from @everyone;"
                         "Community, server settings and onboarding are not changed")
    ap.add_argument("--overlay", help="overlay on top of server.yaml (production defaults to server.prod.yaml)")
    args = ap.parse_args()

    load_env(HERE / ".env")
    gid = args.guild or os.environ.get("DISCORD_GUILD_ID")
    try:
        cfg, warnings = load_config(Path(args.config), overlay_for(gid, args.overlay))
    except ConfigError as e:
        print(f"Config error: {e}")
        return 2
    for w in warnings:
        print(f"! {w}")
    cfg["_staging"] = args.staging or []

    if args.offline:
        print_tree(cfg)
        print("\nConfig is OK.")
        return 0

    token = os.environ.get("DISCORD_TOKEN")
    if not token or not gid:
        print("DISCORD_TOKEN and DISCORD_GUILD_ID are required in .env (see .env.example)")
        return 2
    try:
        return run(cfg, Discord(token), gid, args.apply)
    except (DiscordError, ConfigError) as e:
        print(f"Stopped: {e}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
