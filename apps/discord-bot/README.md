# Community Discord bot

Rust: poise 0.7 + serenity 0.12. State is kept in an embedded SurrealDB 2.7 (SurrealKV): the same version and style as `dbserver-rs`.
Modules are enabled by sections in [bot.toml](bot.toml): no section — the module is off.

## Configuration

- [bot.toml](bot.toml) is an example config with placeholder values (guild ID, channel and role names, URLs). Set your own before running.
- Real secrets (bot token, API keys) go only into `.env`, never into `bot.toml`.

## Modules

| Module | What it does | What SurrealDB provides |
|---|---|---|
| `mod_log` | Deletions, edits, joins and leaves, bans, timeouts → `📋・mod-log` | Everything logged is a `mod_log` record. A single `LIVE SELECT` subscription sends them to the channel; missed ones are sent on startup. Joins and leaves are written by a `DEFINE EVENT` on the `member` table; a rejoin is visible via the `joins` counter. The database deletes old records by itself. |
| `welcome` | A newcomer gets the EN or RU role → a greeting in their language's chat, once | `UPSERT … SET greeted += $role RETURN BEFORE` — an atomic mark in a `set<string>`. |
| `temp_voice` | Join `➕・Create room` → your own room; the owner can rename it, limit it and kick members. An empty room is deleted. | `temp_room.owner` is a `record<member>`. `DELETE … RETURN BEFORE` prevents deleting a channel twice. Empty rooms are cleaned up on startup. |
| `role_panel` | `/rolepanel` posts a panel with buttons; a button grants or removes a role | — |
| `embed` | `/embed` — a form: title, text, color → a nice-looking message in the channel | — |
| `timed` | Scheduled messages; the previous message is replaced by the new one | `every` is stored as a `duration`, `next_at` survives restarts. `UPDATE … WHERE next_at <= time::now() RETURN BEFORE` atomically claims the due ones. `ASSERT $value >= 1m` rejects schedules that are too frequent. |
| `status` | A single message in `🌐・server-status`: online, players, 24h peak, uptime, "online since". The message is edited only when the data changes, plus every 10 minutes as a heartbeat. | Source: the backend's public `GET /dbserver/status` (`status_url`: online, capacity, maintenance), or the older admin API `GET /admin/v1/players` (`players_url` + `bearer_env`). Each poll is a `status_sample` time-series record. An event maintains `up_since` and deletes data older than 7 days. Peak and uptime are computed by a single query (`math::max`, `[WHERE …]`). |
| `translate` | Right-click a message → "Translate" (the menu label also has the Russian word): DeepL into the client's language; only the user who clicked sees the reply | Cache under the composite ID `translation:[message, language]`, kept for 30 days. |
| `tickets` | `/ticketpanel` → EN/RU buttons → form → private thread with the author and `@Mod EN` or `@Mod RU`. On close, the transcript is sent to the log as a file. | The number is an atomic counter `UPSERT counter:ticket SET n += 1`. One open ticket per person. Closing fires exactly once (`UPDATE … WHERE status = 'open'`). Opening and closing write a `mod_log` event on `ticket`. The transcript is stored in the database. |
| `invites` | The join log shows which invite code was used and who invited the person. `/invites [user]` — how many they brought and how many stayed; without a parameter — the top 10. | Graph `member->invited->member` (`TYPE RELATION`, unique pair). Stats are a graph traversal `$m->invited->(member WHERE left_at = NONE)`. A snapshot of invite counters is stored in the `invite` table. |
| `starboard` | 3 ⭐ from other people (not the author) → a copy in `⭐・starboard`, then the counter keeps updating | `star:<message>`. The first publication is claimed by `UPDATE … WHERE board_message = NONE`, so quick stars don't produce a duplicate. |
| `giveaways` | `/giveaway start/end/reroll`, a "Participate" button; pressing it again leaves | Participation is a graph `member->entered->giveaway`. Duration is parsed by `<duration>`. Ending is atomic (`open → drawing`), winners are picked via `ORDER BY rand()`. |
| `feed`, `twitch` | New YouTube videos, GitHub releases, any RSS/Atom; Twitch stream start | Seen items are stored under the composite ID `feed_item:[feed, entry]`. The first poll silently marks old items. `stream:<login>.live_id` gives exactly one post per stream. |

## Running

1. Developer Portal → the bot application → **Bot** → enable **Server Members Intent** and **Message Content Intent**.
   The `mod_log` and `welcome` modules need them. Without these modules the bot doesn't request them.
2. Token: `DISCORD_TOKEN` in `.env`. The bot looks for `.env` in the current folder and its parents, so `Discord/.env` from the setup script works too.
3. Run:
   ```bash
   cargo run --release
   ```
   A different config: env `BOT_CONFIG=path`.

The bot's role must be above the roles it grants (`Events`).

After the first run, post the panels with `/ticketpanel` and `/rolepanel name:notifications`.

Keys for external services (in `.env`):
- `DEEPL_KEY` — translation;
- `TWITCH_CLIENT_ID` and `TWITCH_CLIENT_SECRET` — streams;
- `ADMIN_BEARER` — needed if the backend requires the relay.

YouTube and GitHub need no keys.

SurrealQL pitfall: the statements of a single query are **not** transactional. A `THROW` in the first statement won't undo a `CREATE` in the second. Do the check in a separate query or wrap it in `BEGIN … COMMIT`.

## Deploying on a server

The production bot runs on the same Linux VM as the backend (`docs/CLOUD_ARCHITECTURE.md`): it reads the status and
the patch notes from the backend over loopback, without a token.

1. **Build** for Linux x86_64: `cargo build --release` on Linux or in WSL. The binary links glibc dynamically, so the
   VM's glibc (`ldd --version`) must be at least the build machine's (Ubuntu 24.04 builds need 2.39). An older VM:
   build on the VM itself, or for the `x86_64-unknown-linux-musl` target.
2. **Layout** — a system user `discord-bot` (no login) owns `/opt/discord-bot`:
   ```
   /opt/discord-bot/
     discord-bot    the binary (0755)
     bot.toml       your config (0640)
     .env           secrets (0600)
     data/          the database (SurrealKV), created on the first start
   ```
3. **`.env`**: `DISCORD_TOKEN` (required). Optional: `DEEPL_KEY` (translation), `TWITCH_CLIENT_ID` and
   `TWITCH_CLIENT_SECRET` (streams), `PATREON_TOKEN` (the `fund` module counts Patreon; without it only manual sums),
   `ADMIN_BEARER` (only with the old `[status] players_url`).
4. **`bot.toml` on the VM** — your guild ID, channel and role names, and the backend on loopback (its `[web] bind`):
   ```toml
   [status]
   status_url = "http://127.0.0.1:8090/dbserver/status"
   [patch_notes]
   url = "http://127.0.0.1:8090/dbserver/patch_notes"
   [fund]
   json_path = "/var/www/status/support.json"   # only if your website shows the fund (nginx serves this folder)
   ```
   For `json_path`: the folder belongs to `discord-bot`, group `www-data`, mode 2750; the bot writes the file
   atomically (a temp file, then rename).
5. **Service**: [deploy/discord-bot.service](deploy/discord-bot.service) into `/etc/systemd/system/`, then
   `systemctl daemon-reload && systemctl enable --now discord-bot`.
6. **Check** `journalctl -u discord-bot -n 60`: `[Discord] … connected to '<your server>', commands: N`,
   `[Status] … server online`, and no `channel '…' not found` / `role '…' not found` (fix those names in `bot.toml`).

**Updating**: `systemctl stop discord-bot`; back up the binary, `bot.toml`, `.env` and `data/` (copy `data/` only while
the service is stopped: two processes on one database corrupt it, and the bot refuses to open a database another
process holds); replace the binary and the config; `systemctl start discord-bot`; check the journal as in step 6.
`sha256sum` the binary before and after copying it to the VM.

## Tests

```bash
cargo test
```
The tests (16) run on a real SurrealKV in a temporary folder: `member` and `ticket` events, `LIVE SELECT`, atomic "claims" (log, greeting, starboard, tickets, giveaways), schedule and ASSERT, status summary, invite and giveaway graphs, composite IDs (translation, feeds), YouTube, GitHub and RSS parsing, text substitutions.

## Adding a module

1. Create `src/modules/<name>.rs`, with `pub const SCHEMA` if needed. Write every `DEFINE` with `OVERWRITE`.
2. Add it to `modules/mod.rs`: to `SCHEMAS`, `commands()` or `start()`.
3. Add a section to `config.rs` and `bot.toml`.
4. If the module reacts to Discord events, hook it up in `events.rs`.
