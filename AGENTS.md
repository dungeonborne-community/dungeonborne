# AGENTS.md — for AI assistants

A person gave you a link to this repository and asked you to check it and set it up. Assume they know nothing about
Rust, servers or the game's internals; you do the work and explain each step in plain words. The documentation
is in English; some code comments, player-facing texts and game data are in Russian (the live server's community
spoke Russian). Answer in the person's language.

## What this is
A framework for running a **community server** for Dungeonborne, an online game whose official servers were shut
down:

| Part | Path | What it is |
|---|---|---|
| backend | `core/backend` | game server emulator in Rust (login, lobby, inventory, trade, matchmaking, relay for the launcher) |
| launcher | `apps/launcher` | players' launcher (Tauri): server list, login, mods, network checks |
| Balance Studio | `apps/studio` | editor for balance tables; produces a patch file |
| DevKit | `apps/devkit` | window over `ws`: what is missing on this machine, fix it, secrets, checks |
| ws | `tools/ws` | workspace tool: `check`, `doctor`, `bootstrap`, `secrets`, `roles` |
| xtask | `xtask` | `cargo xtask setup / devkit / ci / dist` |
| UE4SS mods | `game/ue4ss` | Lua mods for the game client and the dedicated server |
| client mod | `game/client-mod` | installer that points a game client at a community server |

Words you will meet are in `docs/GLOSSARY.md`. Each member has a `member.toml` (commands, links) and a `README.md`.

**The game's server tables and protocol descriptors are here; the publisher's client code, assets and paks are not**
(`docs/DISCLAIMER.md`). Other notes: `docs/THIRD_PARTY.md` (programs by others), `docs/CHANGELOG.md`,
`.github/SECURITY.md` (reporting security problems).

## What the person needs
- Windows 10/11 and internet. `bootstrap.ps1` takes ~3 GB (~5 GB without Visual Studio Build Tools) and
  `cargo ws bootstrap --profile=dev` adds ~1 GB of tools, so ~4 GB in all; building the backend, launcher and Studio
  later adds several GB of build cache — the script warns below 20 GB free.
- For the developer tools: nothing else — `bootstrap.ps1` installs Rust and, if missing, git; `cargo ws bootstrap`
  installs the rest (PortableGit, gh, uv + Python 3.12, Lua 5.4, the pak and asset tools, the Goldberg Steam
  emulator and UE4SS), all pinned in `config/tools.lock`.
- Optional, installed by the person when needed: KeePassXC (secrets), Visual Studio Build Tools (otherwise the
  Microsoft CRT/SDK is fetched by cargo-xwin and LLVM is used).
- To run a real server: their **own copy of the game** (Steam), a machine for the backend, their own TLS key,
  launcher signing key and (optionally) a Discord app. See `core/backend/README.md` for what is not automated yet.

## Installing — you drive it, step by step
You run the install yourself, one step at a time, and fix what goes wrong. Every step is a command of
`bootstrap.ps1`, so you never improvise downloads: URLs and sha256 come only from `config/bootstrap.manifest.toml` and
`config/tools.lock`. If you cannot run commands, use the fallback at the end of this section.

All commands below are `powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root <root> …`, where `<root>` is
an empty folder the person chose (for example `C:\DBDev`). Everything goes under it: `workspace` (this repository),
`home` (toolchains, tools, caches), `vault`. No administrator rights, the user PATH and profile are not changed.

**0. Get the script.** Download `bootstrap.ps1` and `config/bootstrap.manifest.toml` into a folder outside `<root>`,
keeping the layout (`<folder>\bootstrap.ps1`, `<folder>\config\bootstrap.manifest.toml`), or use them from a copy of
the repository the person already has. All workspace settings live in `config/`; only `Cargo.toml`, `Cargo.lock` and
`rust-toolchain.toml` stay in the root, because cargo and rustup look for them there.

**1. Audit it — before anything runs.** Read `bootstrap.ps1` fully and compare every URL and sha256 with
`config/bootstrap.manifest.toml`. It must not contain: `iex`/`Invoke-Expression` of downloaded text, base64 or
`-EncodedCommand`, Defender exclusions, autostart entries, scheduled tasks, downloads without a hash, bare IP
addresses, telemetry. Where you can, compare the pins with the official `.sha256` files / GitHub release digests.
Tell the person what you found.

**2. Look at the machine.** `-Status` prints every step as `done`/`todo` and the next command. It changes nothing
in the system; once Rust is installed it checks the `tools` step by building xtask (first time ~40 s, crates from
crates.io, build cache in `home\cache\target`).

| Step | What it does | Typical cost |
|---|---|---|
| `machine` | 64-bit Windows, PowerShell 5.1+, free space | — |
| `rustup` | rustup-init (pinned) into `home\toolchains` | 13 MB, seconds |
| `git` | git from PATH, otherwise PortableGit (pinned, with Git Bash) into `home\tools\git`; `ws bootstrap` later adds PortableGit and `gh` there anyway | 0 or ~60 MB download, ~355 MB on disk |
| `clone` | `git clone` into `workspace` | ~3 MB |
| `toolchain` | the Rust version from `rust-toolchain.toml` | ~120 MB download, ~0.9 GB on disk with the crate cache |
| `linker` | Visual Studio Build Tools if present, otherwise cargo-xwin | 0, or ~1 GB Microsoft CRT/SDK |
| `tools` | trunk, tauri-cli, tailwindcss from `config/tools.lock` (pinned); LLVM too without Visual Studio | ~130 MB (+~860 MB LLVM) |
| `devkit` | ws and DevKit: a signed release if there is one, otherwise built here | ~2 min build, ~2 GB build cache |
| `components` | a partial checkout when `-Components` was given; `done` for a full clone | — |

With Visual Studio Build Tools and a fast connection `bootstrap.ps1` takes about 5 minutes, downloads ~0.3 GB and
takes ~3 GB on disk (2.1 GB of it is the build cache `home\cache\target`, safe to delete: ws and DevKit are copied to
`home\tools`). On a slow link to `static.rust-lang.org` (100–500 KB/s) the `toolchain` step alone takes 10+ minutes
and the whole install 45 minutes or more — tell the person up front.

Network access besides the pinned files: crates from crates.io while building (each checked against `Cargo.lock`
checksums) and trunk's own update check (it prints "Found an update" — ignore it; versions come only from
`config/tools.lock`). Tell the person about both.

**3. Show the plan and get a "yes".** Run it without `-Apply` (plan only), show the person the plan, the sizes and
the time. If your tools refuse to run the script (an assistant's safety mode, e.g. Claude Code "auto"), do not look
for other ways to run it: give the person your audit conclusion and the exact commands (plan, then `-Apply`) and ask
them to run it themselves; when they say it finished, check with `-Status` (read-only) and go on. If the `linker` step will use cargo-xwin, say that the Microsoft CRT/SDK comes under Microsoft's license
and is **not** pinned by hash. The repository URL is `config/bootstrap.manifest.toml [repo] url`; for a fork add
`-Repo <url>`. Do not continue without an explicit "yes".

**4. Run the steps.** For each `todo` step in order: `-Step <step> -Apply`, read the output, then `-Status`.
Tell the person in one line what each step did. Running all of them at once is the same as `-Apply` without
`-Step`; one at a time is better for you because a failure stays small and easy to repeat.

**5. When a step fails** — the script prints the command to repeat it. What the messages mean:

| Message | Cause | What you do |
|---|---|---|
| `download failed`, curl `(28)`/`(56)`, `Connection was reset` | network | Repeat the same step (it already retried 5 times). If it fails again, measure the speed (`curl.exe -s -o NUL --max-time 10 -w "%{speed_download}" <url>`) and tell the person: their connection, not the script. Never switch to another download source. |
| `sha256 … does not match … file deleted` | broken or changed file | Repeat the step **once**. If it fails again, **stop**: the file on the server differs from the pinned one. Tell the person; never edit a hash to make it pass. |
| `step 'X' needs step 'Y' first` | order | Run step `Y`. |
| `git clone … failed` | private repository or wrong URL | A private repository needs the person to sign in (a window opens); a fork needs `-Repo`. |
| `… exists, is not empty and has no config/workspace.toml` | a broken earlier clone | Show the person what is inside; delete it only with their yes; repeat `clone`. |
| `[tools] tool <name>: …` failed | one download | `-Step tools -Tool <name> -Apply` repeats only that tool. |
| `curl … exit code Some(-1)` (or another negative code) | curl was killed: often antivirus checking a new `.exe` while it is written, or someone stopped it | Repeat with `-Tool <name>`. If it keeps happening, tell the person; do not touch antivirus settings. |
| linker errors (`link.exe` not found) in `devkit` | Visual Studio found but its C++ tools are broken (without Visual Studio the `linker` step picks cargo-xwin by itself) | `-Step linker -Linker xwin -Apply`, then `-Step devkit -Apply`. |
| `WARN … GB free` | disk | Tell the person how much is needed; do not delete their files. |
| rustup `component download failed … (os error 10013)` / `tcp connect error` in `toolchain` | the connection dropped during a long download | Repeat the step: rustup resumes the `.partial` file. |

Messages to ignore (they are not errors): rustup's `no default toolchain set` / `run 'rustup default stable'` (do not
run it: the `toolchain` step installs the pinned version), trunk's `Found an update` and `no rust project found`,
`STATIC_VCRUNTIME is deprecated`; in Windows PowerShell 5.1, red `NativeCommandError` blocks around cargo or curl
progress lines (`Downloading crates …`) when you capture output with `2>&1` — that is stderr, not a failure; judge
by the exit code and the step's last lines.

Never run the script as administrator, never turn off antivirus, never change the user PATH or profile, never
delete anything outside `<root>` — and inside it only `home\cache` without asking.

**6. Finish and check.** `-Status` shows everything `done`. Further commands run in `<root>\workspace` in a shell
with these variables (set them for your commands only, not for the user):
`RUSTUP_HOME=<root>\home\toolchains\rustup`, `CARGO_HOME=<root>\home\toolchains\cargo`,
`CARGO_TARGET_DIR=<root>\home\cache\target` and `<root>\home\toolchains\cargo\bin` first on PATH. Without
`CARGO_TARGET_DIR` cargo builds into `workspace\target` and `ws check` reports it as a layout error.
1. `cargo ws doctor --profile=dev` first: it is read-only and lists what the next command will do. `ws bootstrap` has
   no plan mode — it always installs (`--apply` is accepted and changes nothing). Without `--profile` the profile is
   `owner` (everything); pick `dev` (developer), `designer` (Balance Studio only) or `agent`.
2. `cargo ws bootstrap --profile=dev` — creates `config/local.toml` from `config/local.example.toml` and the folders in
   `home`, downloads ~1 GB of pinned tools into `home\tools` (PortableGit, gh, uv + Python 3.12, Lua, repak, retoc,
   kismet-analyzer, Oodle, UAssetGUI, FModel, Goldberg, UE4SS), writes `.claude\settings.local.json` (the agent's
   environment and a SessionStart hook that puts `gh` on PATH) and `<root>\CLAUDE.md` (sends an assistant opened in
   the root to `workspace`), and looks for the game in the person's Steam libraries (read only). Tell the person all of
   this before running it.
3. `cargo ws check` — 0 errors; the one warning is `[preview] apps/discord-bot/member.toml`. `cargo ws doctor` — one
   **expected error**, `[game] reference`, until the person has a clean copy of the game: `cargo ws game` prints the
   ways (copy a clean Steam install, or the exact Steam `download_depot` command). Getting it is the person's
   decision; `ws bootstrap` exits with code 1 for the same reason.
4. DevKit (`<root>\home\tools\devkit\DevKit.exe`) shows the same in a window.

**7. Tell the person** what is installed, where, how much space it takes, and that removing it is deleting
`<root>` (nothing was installed outside it; a GitHub sign-in, if there was one, stays in Windows' credential store).

**Fallback — no agent, or an agent that cannot run commands:** the person runs the whole script themselves:
`powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root C:\DBDev` (plan), then the same with `-Apply`. After
a failure they run it again: finished steps are skipped.

ws and DevKit: `cargo xtask setup` accepts a prebuilt release only if it is signed with the key in `xtask/dist.pub`
(the maintainers' public key). Otherwise — and always with `-Build source` — they are built here. A fork that ships
its own releases replaces `xtask/dist.pub` (`cargo xtask dist keygen`).

## Scripts that change the system — never run without the person's explicit yes
- `game/client-mod/install.ps1` — asks for administrator rights and edits the `hosts` file (redirects the game's
  former service domains to the community server).
- `core/backend/ds-agent/install_task.ps1` — registers a scheduled task for the dedicated-server agent (server
  machines only).
- The launcher may ask (UAC) to edit `hosts` and installs the client runtime (UE4SS, Steam API emulator) into the
  game folder.

## Rules for you while working here
- Paths of this machine go only into `config/local.toml` (copy `config/local.example.toml`); never into tracked files.
- Secrets never go into git: `config/secrets.toml` lists what is needed, values live in the person's KeePassXC database
  (`cargo ws secrets init`, then `cargo ws secrets run <name> -- <command>`). Never print secret values.
- The pak key is not in this repository; the person finds it themselves. Tell them it is public (community collections
  of AES keys for UE4/UE5 games list Dungeonborne's), help them search if they ask, and let them put it into
  `config/local.toml` `[re] aes_key` (not in git). Never write it into a tracked file or print it (docs/SECRETS.md).
- Before every commit: `cargo xtask ci` (fmt, clippy, tests, `ws check`).
- Large or machine-specific things (toolchains, game copies, dumps, build output) live in `home\`, never in git.
- Do not download or run anything that is not pinned by sha256 in `config/bootstrap.manifest.toml` or `config/tools.lock`
  without asking the person.
- Do not decrypt or unpack the game's paks unless the person explicitly asks; their copy, their decision.

## The game client — our rules
They hold for you, for the person and for any research in this repository:
- **The exe and the game's own binaries are not modified** on disk: no patching, no unpacking, no rewriting. What goes
  into the game folder is only the documented runtime (UE4SS with its `dwmapi.dll` loader, the Goldberg
  `steam_api64.dll` in place of Steam's), our mods and our paks — the launcher puts them in for a session and takes
  them out again.
- **No reading or writing the memory of live game processes**: no ReadProcessMemory, debuggers, SeDebugPrivilege,
  memory scanners or injected tools beyond UE4SS.
- **Anti-cheat**: do not bypass, disable or fool it; do not kill game processes without the person's permission
  (under the anti-cheat they exit only by themselves — `quit`); keep a 30 s pause between client restarts.
- **The game's cryptography is not touched**: protocol encryption, signatures and integrity checks, the anti-cheat's
  protection — no cracking, no brute force, no patching them out. The only key used is the public pak key, and only
  to read the paks of the person's own copy.
- **Native code research** uses only the decrypted image the game itself writes into a crash dump
  (`-fullcrashdump`, `docs/owner/RE_TECHNIQUE.md`), analysed offline in Ghidra (`research/ghidra/README.md`).

## Known limits of this edition
- "internal notes", "(internal notes)" and paths under `ops/…`, `tools/stand` or `content/…` in comments point to
  the maintainers' material that is not part of this edition (`content/seasons` is where the backend expects your
  own season data).
- The backend builds and starts with the tables in `core/backend`; matches need the game's own battle server, the mods
  and the patch paks built from the person's own game copy (`README.md`, "Your copy of the game").
- The launcher needs `apps/launcher/bundle/client.zip` before it compiles: `cargo xtask setup --apply
  --tool=goldberg,ue4ss`, then `python apps/launcher/tools/make_client_bundle.py --upstream`.
- Addresses in the code (`*.example.org`, `203.0.113.x`, `192.0.2.x`) and the zeroed certificate pin in
  `apps/launcher/src-tauri/src/builtin.rs` are placeholders: set your own server there and in `server.toml`.
