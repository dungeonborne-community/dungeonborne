> Copy of the contract as of 14.11. Source of truth — `docs/specs/bug-report-api.md` (maintained by the "Discord server via MCP" session; changes go there first, then here). Implementation: backend `core/backend/src/bug_reports.rs`, launcher `apps/launcher/src-tauri/src/bugreport.rs`.

# Bug report from the game and launcher → Discord: API contract

Draft to agree on with the launcher and the backend (dbserver-rs). The bot side is already written: module
`bot/src/modules/bug_reports.rs`. It is off until `bot.toml` has a `[bug_reports]` section.

**What exists now (orchestrator analysis, 14.11).** There is no "report a bug" button in either the mods or the launcher.
- **Game.** The native "SUPPORT" button (ESC menu, login window) opens a dead Tencent chat. The game itself does not talk HTTP to the backend and cannot send logs.
- **Launcher.** There is "Copy report" on the ISP-block banner (`net_report_text`) and automatic sending of network reports `POST net/report` with a Bearer session. The new button follows their pattern.

**So the report is sent by the launcher**, and the game only later and only as a trigger:
- the `/bug` chat command creates a ticket by `open_id`, and the launcher uploads the logs afterwards;
- or the native "SUPPORT" button is intercepted by an UnLua pak.

## Report path

1. The player presses "Report a bug" in the launcher.
2. The launcher builds the report: description, type, versions, tails of the launcher and game logs.
3. The launcher sends the report to the **backend** with the same Bearer session as `net/report`.
4. The backend adds what only it knows: `discord_id` from the session and the last match number by `open_id`. It rate-limits and stores the report with status `new`.
5. The **bot** on the same VM picks up new reports through the admin API every 20 seconds and creates a post in `🐛・bug-reports`. The author is mentioned and added to the post; a language tag and a type tag are set.
6. The bot tells the backend the report is published and passes the post link. The client can show the player: "Report #id published", with a link.

Why this way: the client has neither the bot token nor a webhook address. A player's client can be taken apart, and such a token would leak. The client knows only its normal game login. Only the bot writes to Discord.

## 1. Client → backend

`POST /api/launcher/v1/bug_report` — next to `net/report`, via `authed()`: `Authorization: Bearer <session_token>`. `min_client` check by User-Agent `dbl/<ver>`, ban and whitelist — as for the other launcher routes.

`multipart/form-data`:

| Part | Type | Required | What |
|---|---|---|---|
| `report` | JSON | yes | Fields below |
| `launcher.log` | file | no | Tail, up to 2 MB: `%LOCALAPPDATA%\DungeonborneLauncher\logs\` (no rotation — take the end) |
| `relay.log` | file | no | Tail, up to 2 MB, same place |
| `crash_ue4ss.log` | file | no | `<install>\Dungeonborne\Binaries\Win64\` (overwritten on launch — send before the game restarts) |
| `Dungeonborne.log` | file | no | Tail, up to 3 MB: `%LOCALAPPDATA%\Dungeonborne\Saved\Logs\` |
| `screenshot.png` | file | no | Up to 1 MB |

The launcher knows the install path (`settings.installs[0]`). All files together — up to 9 MB: the bot's attachment limit in Discord is 10 MB.

```json
{
  "source": "launcher",
  "kind": "game",
  "title": "Inventory is not saved after the raid",
  "description": "Picked up an item, exited through the blue portal, the stash is empty.",
  "versions": { "game": "<manifest.game.label>", "mods": "<modset_sha256, first 12 chars>" },
  "exe_sha256": "<sha256 of Dungeonborne.exe from check_install>",
  "os": "Windows 11 23H2",
  "lang": "ru"
}
```

- `source`: `launcher` (later `game`, if the report was started with the in-game `/bug` command).
- `kind`: `crash` | `game` | `launcher` — what broke. If the launcher sees a fresh `crash_ue4ss.log` after a game crash, it suggests `crash`.
- `title` — up to 100 characters. `description` — up to 1500.
- The backend takes the launcher version from the User-Agent (`dbl/<ver>`). There is no string game version, so the launcher sends `manifest.game.label` and `exe_sha256`.
- **The client does not send the match number.** Neither the mod nor the launcher knows it. The backend takes it itself by `open_id`: the current match — `matchmaking::room_of_player`, the last one — `PlayerModel.state["raid_gear"]`.
- `lang` — the launcher language (`en`, `ru`, …): the bot sets the language tag from it.

Response: `201 {"id": "r_01JD…"}`. On the limit — `429 {"retry_after": 1800}`: **no more than 3 reports per hour** per player.

`GET /api/launcher/v1/bug_report/{id}` — status for the launcher: `{"status": "new" | "posted", "url": "https://discord.com/channels/…"}`.

The backend **must**:
- take `discord_id` (`Account.subject` of the discord provider) and `open_id` from the session, not from the request body;
- add `battle_id` and the launcher version itself;
- check file sizes and types;
- truncate fields to the limits.

## 2. Bot ← backend (admin API, loopback only, Bearer)

Same access as the status module: `http://127.0.0.1:9092/admin/v1`, token `ADMIN_BEARER`.

`GET /admin/v1/bug-reports?status=new&limit=10` — new reports, oldest first:

```json
[
  {
    "id": "r_01JD…",
    "discord_id": "111111111111111111",
    "created_at": 1763150400,
    "source": "launcher",
    "kind": "crash",
    "title": "Inventory is not saved after the raid",
    "description": "…",
    "battle_id": "48213",
    "versions": { "game": "1.0.3", "launcher": "0.9.1", "mods": "0.4.2" },
    "os": "Windows 11 23H2",
    "lang": "ru",
    "files": [ { "name": "relay.log", "size": 183204 }, { "name": "launcher.log", "size": 40211 } ]
  }
]
```

`GET /admin/v1/bug-reports/{id}/files/{name}` — the file content as is (`application/octet-stream`).

`POST /admin/v1/bug-reports/{id}/posted` with body `{"thread_id": "…", "url": "https://discord.com/channels/…"}` → `204`. After that the report is no longer returned for `status=new`.

The bot does not publish twice: it remembers published ids in its database. If the confirmation did not reach the backend, the bot sends it again on the next poll, but does not create a second post.

## What the post looks like

- Title: `title`.
- Tags: type (`crash` → Crash, `game` → Gameplay, `launcher` → Launcher) and language (`ru` → RU, everything else → EN).

Text:

```
📋 Report from @player · game · November 14, 2026 20:00
Battle: 48213 · Game 1.0.3 · Launcher 0.9.1 · Mods 0.4.2 · Windows 11 23H2

> Picked up an item, exited through the blue portal, the stash is empty.
```

Attachments — logs and the screenshot. If the files together exceed 10 MB (Discord's limit for a bot), the extra ones are not attached, and the text says which files the bot skipped.

## What each side needs to do

| Side | What to do |
|---|---|
| Launcher | Tauri command `bug_report` following `net_report_text` and `send_net_reports`: tails of the four logs, `exe_sha256`, mod versions. "Report a bug" button in the UI → form (title, description, type) → `POST /api/launcher/v1/bug_report` → link to the post. The `contact` field in the manifest — for the Discord link |
| Backend | Reports table. Public routes: `POST` and status `GET` next to `net/report`. Admin API: list, file, `posted`. Add `battle_id` by `open_id`, limit 3 per hour |
| Game (later) | The `/bug` chat command creates a ticket, the launcher uploads the logs; or the "SUPPORT" button is intercepted by an UnLua pak |
| Bot | Done: `[bug_reports]` in `bot.toml` enables the module |
