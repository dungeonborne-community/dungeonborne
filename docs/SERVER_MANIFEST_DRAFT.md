# Server manifest and mod package — draft v0

Status: draft for discussion, 2026-11-02. None of this is implemented yet.

## Model

Like community servers in other games: one client (one exe), one client protocol,
many independent servers. Each server has its own backend, its own accounts and progress,
its own mod set. Only three things are shared:

1. the game version (exe hash);
2. the server manifest format (`dbserver/1`);
3. the mod package format (`dbmod/1`).

The launcher reads the server manifest, builds a profile for it (mods + address + CA),
starts the game and after exit leaves nothing behind in other profiles.

## How the address is set now (important for the format)

The client goes to fixed domains (`sg.intlgame.com`, `tgf123.com`,
`dir.228918463-2-1.dmpplat.com` and others — the list is in `game/client-mod/install.ps1`).
Right now the address is changed like this:

- `hosts` redirects these domains to the server IP;
- the server CA is appended to `Dungeonborne/Binaries/Win64/cacert.pem`.

`hosts` is a global system file, editing it requires admin rights and does not allow keeping two servers
at once. So a **local relay** is proposed (see the "Launcher" section):
`hosts` points to `127.0.0.1` once at install time, and the launcher points the
relay at the chosen server on launch. Until there is a relay, the launcher can write `hosts` from the manifest.

## 1. Server manifest — `dbserver/1`

Served by the server at `https://<server>/.well-known/dbserver.json` or listed in the server list.

```json
{
  "schema": "dbserver/1",
  "id": "ru.dungeonborne.main",
  "name": "Dungeonborne RU",
  "description": "Custom game mode, co-op dungeons",
  "region": "eu-central",
  "languages": ["ru", "en"],
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
    "id": "ru-coop",
    "tags": ["pve", "coop", "custom-balance"]
  },

  "mods": [
    { "id": "ru.rebuild-private", "version": "1.4.0", "sha256": "…", "required": true },
    { "id": "ru.r14-fix",         "version": "0.3.1", "sha256": "…", "required": true },
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
| `schema` | yes | Format version. The launcher refuses an unknown major version. |
| `id` | yes | Stable server id (reverse-DNS). The profile key in the launcher. |
| `game.exe_sha256` | yes | Which exe the server is built for. If it does not match, the launcher does not start. |
| `connect.address` | yes | Where to point the client domains (IP or DNS name). |
| `connect.domains` | no | `"default"` = the game's standard domain list. An array — if the server overrides it. |
| `connect.ports` | no | For reference, for the relay and diagnostics. Standard ones by default. |
| `connect.ca_cert` / `ca_sha256` | yes | Server CA for `cacert.pem`. The launcher checks the hash and **replaces** the CA in the profile instead of accumulating all CAs in one file. |
| `mode` | no | Only for display and filters in the server list. |
| `mods[]` | yes | Client mod set. `version` is exact for `required`, a semver range is allowed for optional ones. `sha256` — package hash, mandatory for `required`. |
| `policy.allow_unlisted_client_mods` | no | `false` = the launcher starts only with the mods from the manifest. |
| `policy.accounts` | no | `local` = own accounts. Reserved: `shared:<provider>` for shared login in the future. |
| `status_url` | no | Online count, capacity, backend version (format below). |
| `key` / `signature` | not in v0 | Server key and manifest signature. The launcher remembers the key on first connection (TOFU) and warns when it changes. |

**Only client** mods go into the server manifest. Server mods are the server's internal business,
the client does not need to know about them.

### `status_url`

```json
{ "online": 14, "capacity": 64, "backend": "dbserver-rs 0.9.2", "modset_sha256": "…", "motd": "…" }
```

## 2. Mod package — `dbmod/1`

A `.zip` archive with `mod.json` in the root and the mod files.

```json
{
  "schema": "dbmod/1",
  "id": "ru.r14-fix",
  "version": "0.3.1",
  "name": "R14 equipment manager fix",
  "authors": ["…"],
  "license": "MIT",
  "side": "client",

  "game": { "exe_sha256": ["62d509a0…"] },

  "components": [
    { "type": "pak",   "path": "Paks/zzzzzzz_BPFix_P.pak", "priority": 70 },
    { "type": "ue4ss", "path": "ue4ss/DungeonborneR14", "ue4ss": ">=3.0.1" }
  ],

  "depends":   [ { "id": "ru.ue4ss-runtime", "version": "^3.0.1" } ],
  "conflicts": [ { "id": "ru.client-hooks", "reason": "both patch the hero UCS" } ],

  "config": {
    "files": ["ue4ss/DungeonborneR14/settings.txt"],
    "server_overridable": ["settings.txt:native_data"]
  },

  "risk": { "native_code": false, "network": false },

  "files": { "Paks/zzzzzzz_BPFix_P.pak": "sha256…", "ue4ss/DungeonborneR14/main.lua": "sha256…" },
  "signature": { "author": "ed25519:…", "catalog": "ed25519:…" }
}
```

### Fields

| Field | Meaning |
| --- | --- |
| `side` | `client`, `server` or `both`. `client` and `both` go into the server manifest. |
| `components[].type` | `pak`, `ue4ss` (Lua mod), `native` (DLL), `config`. |
| `components[].priority` | Pak order. Paks override each other by name (`zz_` < `zzzzz_`), so the launcher **itself** assigns the name by priority instead of relying on the author's name. A conflict of identical files inside paks is a warning when building the profile. |
| `depends` / `conflicts` | Resolved by the launcher before launch. A conflict = refusal with an explanation. |
| `config.server_overridable` | Keys the server may override in its manifest (reserved, not used in v0). |
| `risk.native_code` | `true` if there is a `native` component or the Lua loads a DLL (`package.loadlib`). Such a mod is installed only from a trusted catalog or after explicit player confirmation. |
| `files` | Hashes of all files. The launcher checks them after unpacking. |
| `signature` | The author's signature, and the catalog's signature if the mod is in the catalog. |

## 3. Server list (optional)

```json
{ "schema": "dbserverlist/1", "servers": ["https://a.example/.well-known/dbserver.json", "https://b.example/…"] }
```

The list only points to where the manifests are. A server can always be added manually by address.

## 4. What the launcher does

1. Downloads the manifest, checks `schema`, `game.exe_sha256` and the signature/TOFU key.
2. Resolves mods: downloads, checks hashes and signatures, checks `depends`/`conflicts`.
   A mod with `risk.native_code` not from the catalog — explicit player confirmation.
3. Builds the server **profile**: its own pak set (by priority), its own UE4SS mods,
   `cacert.pem` with this server's CA, its own local data (saved login, settings).
4. Points the address: relay to `connect.address` (or `hosts`, until there is a relay).
5. Starts the game. After exit returns the install to a clean state.
6. Computes `modset_sha256` = sha256 of the sorted `id@version:sha256` lines of the client mods.

### Open questions

- **Profile isolation.** UE mounts all `Content/Paks` subfolders recursively, and UE4SS
  reads `game/ue4ss/Mods` next to the exe. Options: move files on every launch, keep
  inactive paks outside `Paks` (as `_pve_disabled_paks` does now), or a separate game copy per
  profile. A choice is needed.
- **Relay and TLS.** If the relay simply forwards TCP, the CA in `cacert.pem` still has to be
  from the chosen server — that is fine, the profile swaps it. There is no need to terminate TLS in the relay.
- **Mod check on login.** The client does not report its mod set. Options: the launcher puts
  `modset_sha256` into the login data, or a client mod sends it in a separate request. For now
  the check is only on the launcher side (enough for honest players).
- **Mod catalog.** Who maintains the trusted catalog and whose key signs it.
- **ACE.** Swapping paks between launches works now. But replacing files while the game is running
  is ruled out: the launcher changes the profile only when the game is closed.
