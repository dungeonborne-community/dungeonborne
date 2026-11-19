# Server manifest and mod package — draft v0

Status: draft for discussion, 2026-11-02. None of this is implemented yet.

## Model

Like community servers in other games: one client (one exe), one client protocol,
many independent servers. Each server has its own backend, its own accounts and progress,
and its own set of mods. Only three things are shared:

1. the game version (exe hash);
2. the server manifest format (`dbserver/1`);
3. the mod package format (`dbmod/1`).

The launcher reads the server manifest, builds a profile for it (mods + address + CA),
starts the game, and leaves no traces in other servers' profiles after exit.

## How the address is set today (this shapes the format)

The client talks to fixed domains (`sg.intlgame.com`, `tgf123.com`,
`dir.228918463-2-1.dmpplat.com` and others). Today the address is changed like this:

- the `hosts` file redirects these domains to the server's IP;
- the server's CA is appended to `Dungeonborne/Binaries/Win64/cacert.pem`.

`hosts` is a system-wide file: editing it needs admin rights, and it can't point at two
servers at once. So we propose a **local relay** (see "Launcher"): at install time `hosts`
points to `127.0.0.1` once, and at launch the launcher points the relay at the selected
server. Until the relay exists, the launcher can write `hosts` from the manifest.

## 1. Server manifest — `dbserver/1`

Served at `https://<server>/.well-known/dbserver.json` or listed in a server list.

```json
{
  "schema": "dbserver/1",
  "id": "org.example.main",
  "name": "Example Server",
  "description": "Custom game mode, co-op dungeons",
  "region": "eu-central",
  "languages": ["en", "ru"],
  "website": "https://example.org",
  "contact": "discord.gg/xxxx",

  "game": {
    "exe_sha256": "62d509a00d984696e021fb5ddba9f87fd1b27c84f695877345ff5fe9401f58b3",
    "label": "Dungeonborne Win64 final"
  },

  "connect": {
    "address": "203.0.113.10",
    "domains": "default",
    "ports": { "dir": 8085, "lobby": [6601, 6602, 6603, 6604], "https": 443 },
    "ca_cert": "https://example.org/dbserver-ca.pem",
    "ca_sha256": "…"
  },

  "mode": {
    "id": "example-coop",
    "tags": ["pve", "coop", "custom-balance"]
  },

  "mods": [
    { "id": "example.base-patch", "version": "1.4.0", "sha256": "…", "required": true },
    { "id": "example.gameplay-fix", "version": "0.3.1", "sha256": "…", "required": true },
    { "id": "community.better-hud", "version": "^2.0.0", "required": false }
  ],

  "policy": {
    "allow_unlisted_client_mods": false,
    "accounts": "local"
  },

  "status_url": "https://example.org/dbserver/status",
  "key": "ed25519:…",
  "signature": "…"
}
```

### Fields

| Field | Req. | Meaning |
| --- | --- | --- |
| `schema` | yes | Format version. The launcher rejects an unknown major version. |
| `id` | yes | Stable server id (reverse-DNS). Key of the launcher profile. |
| `game.exe_sha256` | yes | The exe the server is built for. On mismatch the launcher won't start the game. |
| `connect.address` | yes | Where to send the client's domains (IP or DNS name). |
| `connect.domains` | no | `"default"` = the game's standard domain list. An array if the server overrides it. |
| `connect.ports` | no | Informational, for the relay and diagnostics. Defaults to the standard ports. |
| `connect.ca_cert` / `ca_sha256` | yes | The server's CA for `cacert.pem`. The launcher checks the hash and **replaces** the CA in the profile instead of piling every CA into one file. |
| `mode` | no | Display and server-list filters only. |
| `mods[]` | yes | Client mod set. `version` is exact for `required` mods; a semver range is allowed for optional ones. `sha256` is the package hash, mandatory for `required`. |
| `policy.allow_unlisted_client_mods` | no | `false` = the launcher only starts with the mods listed in the manifest. |
| `policy.accounts` | no | `local` = the server's own accounts. Reserved: `shared:<provider>` for a future shared login. |
| `status_url` | no | Players online, capacity, backend version (format below). |
| `key` / `signature` | not in v0 | Server key and manifest signature. The launcher remembers the key on first connect (TOFU) and warns if it changes. |

Only **client** mods go into the server manifest. Server-side mods are the server's
internal business; the client doesn't need to know about them.

### `status_url`

```json
{ "online": 14, "capacity": 64, "backend": "example-backend 0.9.2", "modset_sha256": "…", "motd": "…" }
```

## 2. Mod package — `dbmod/1`

A `.zip` archive with `mod.json` at the root plus the mod's files.

```json
{
  "schema": "dbmod/1",
  "id": "example.gameplay-fix",
  "version": "0.3.1",
  "name": "Example gameplay fix",
  "authors": ["…"],
  "license": "MIT",
  "side": "client",

  "game": { "exe_sha256": ["62d509a0…"] },

  "components": [
    { "type": "pak",   "path": "Paks/ExampleFix_P.pak", "priority": 70 },
    { "type": "ue4ss", "path": "ue4ss/ExampleFix", "ue4ss": ">=3.0.1" }
  ],

  "depends":   [ { "id": "example.ue4ss-runtime", "version": "^3.0.1" } ],
  "conflicts": [ { "id": "other.hero-tweaks", "reason": "both patch the same hero blueprint" } ],

  "config": {
    "files": ["ue4ss/ExampleFix/settings.txt"],
    "server_overridable": ["settings.txt:some_option"]
  },

  "risk": { "native_code": false, "network": false },

  "files": { "Paks/ExampleFix_P.pak": "sha256…", "ue4ss/ExampleFix/main.lua": "sha256…" },
  "signature": { "author": "ed25519:…", "catalog": "ed25519:…" }
}
```

### Fields

| Field | Meaning |
| --- | --- |
| `side` | `client`, `server` or `both`. `client` and `both` go into the server manifest. |
| `components[].type` | `pak`, `ue4ss` (Lua mod), `native` (DLL), `config`. |
| `components[].priority` | Pak order. Paks override each other by file name (`zz_` < `zzzzz_`), so the launcher **itself** names the pak from its priority instead of trusting the author's file name. Identical files in several paks produce a warning when the profile is built. |
| `depends` / `conflicts` | Resolved by the launcher before launch. A conflict = refusal with an explanation. |
| `config.server_overridable` | Keys the server may override in its manifest (reserved, unused in v0). |
| `risk.native_code` | `true` if the mod has a `native` component or its Lua loads a DLL (`package.loadlib`). Such a mod is installed only from a trusted catalog or after the player explicitly confirms. |
| `files` | Hashes of all files. The launcher verifies them after unpacking. |
| `signature` | The author's signature, plus the catalog's if the mod is in the catalog. |

## 3. Server list (optional)

```json
{ "schema": "dbserverlist/1", "servers": ["https://a.example/.well-known/dbserver.json", "https://b.example/…"] }
```

The list only says where the manifests are. A server can always be added manually by address.

## 4. What the launcher does

1. Downloads the manifest, checks `schema`, `game.exe_sha256` and the signature / TOFU key.
2. Resolves mods: downloads them, checks hashes and signatures, checks `depends`/`conflicts`.
   A mod with `risk.native_code` from outside the catalog requires explicit player confirmation.
3. Builds the server **profile**: its own pak set (by priority), its own UE4SS mods,
   a `cacert.pem` with this server's CA, its own local data (saved login, settings).
4. Points the address: relay to `connect.address` (or `hosts`, until the relay exists).
5. Starts the game. After exit, returns the install to a clean state.
6. Computes `modset_sha256` = sha256 of the sorted `id@version:sha256` lines of the client mods.

### Open questions

- **Profile isolation.** UE mounts every subfolder of `Content/Paks` recursively, and UE4SS
  reads `game/ue4ss/Mods` next to the exe. Options: move files on every launch, keep inactive paks
  outside `Paks`, or a separate game copy per profile. We need to pick one.
- **Relay and TLS.** If the relay just forwards TCP, the CA in `cacert.pem` must still be the
  selected server's — that's fine, the profile swaps it. The relay doesn't need to terminate TLS.
- **Mod check at login.** The client doesn't report its mod set. Options: the launcher puts
  `modset_sha256` into the login data, or a client mod sends it in a separate request. For now
  only the launcher checks (enough for honest players).
- **Mod catalog.** Who runs the trusted catalog and whose key signs it.
- **Anti-cheat (ACE).** Swapping paks between launches works today. Swapping files while the
  game is running is out of the question: the launcher changes the profile only while the game is closed.
