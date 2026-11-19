# FAQ

Questions people ask about this repository, with straight answers. Questions about one particular community server
(its players, donations, plans) belong to that server's own channels, not here.

## The project

**What is this?**
Everything needed to run a community server for Dungeonborne after the official servers closed: a game-server
emulator, a launcher, game mods, a balance editor, a Discord bot, a website and the tooling around them. It ran a
live community server with real players in November 2026. Overview: the root [`README.md`](../README.md).

**Where does the server code come from?**
It started from a community patch: a Chinese PvE patch contained the game's protocol dumps, configuration tables and
a working backend in Go. That backend was rewritten in Rust and grew far beyond it; what was reused, file by file, is
in [`CN_PATCH_IMPORTS.md`](CN_PATCH_IMPORTS.md), the whole story in [`HISTORY.md`](HISTORY.md).

**Was it written by AI?**
It was built together with AI assistants, openly — the repository is set up for that (`AGENTS.md`, roles in
`.claude/agents`, notes in `agents/memory`). Decisions, testing on real game sessions and responsibility stayed with
people.

**Why are the commits from "Dungeonborne community" and why is there almost no history?**
The project belongs to its community, not to one name. The working history contained personal data and internal
addresses, so the public edition starts from a clean commit.

**Why is the documentation in English but some comments and texts in Russian?**
The live server grew out of a Russian-speaking community. Documentation is English for everyone; code comments,
player-facing texts and some game data are partly Russian. Translations are a good first pull request.

## Legal

**Is this legal?**
The repository holds our own code, the game's server tables and protocol descriptors the emulator needs, and
research notes. It does not hold the publisher's client code, assets or paks: everything game-specific is rebuilt
from your own copy of the game ([`DISCLAIMER.md`](DISCLAIMER.md)). The rights holder can still ask for removal; the
code is MIT / Apache-2.0, so forks keep it alive.

**Can I run my own server? Charge for it?**
Our code allows it (MIT or Apache-2.0). The game itself belongs to its rights holder; running a server, and above all
making money with one, is your own legal risk.

**Where is the pak key?**
Not in this repository — it is the publisher's. It is public: community collections of AES keys for UE4/UE5 games
list it. Put it into your `config/local.toml` ([`SECRETS.md`](SECRETS.md), "The pak key").

**Will there be ready-made builds (launcher exe, server)?**
Not with game files inside. Everything here builds from source; what touches the game is built from your own copy.

## The game client

**Did you crack the anti-cheat, patch the exe or read the game's memory?**
No, and these are written rules for everyone working here (`AGENTS.md`, "The game client — our rules"): the exe and
the game's binaries are not modified, the memory of a running game is not read or written, the anti-cheat is not
bypassed, the game's cryptography is not touched. Native code was studied only through the full crash dump the engine
writes itself (`-fullcrashdump`), offline ([`owner/RE_TECHNIQUE.md`](owner/RE_TECHNIQUE.md)).

**What does the launcher put into my game folder?**
For a game session only: UE4SS (Lua mod runtime, with its `dwmapi.dll` loader), the Goldberg Steam emulator in place
of `steam_api64.dll`, and the server's mods and paks. Files of earlier versions are removed when they are unchanged;
files you changed yourself are left alone ([`apps/launcher/README.md`](../apps/launcher/README.md)).

**Why does the launcher ask for administrator rights?**
Once, to add lines to `hosts` (the game's former service names → the local relay); it explains this before Windows
asks. Its updates are signed, mods are checked by hash, the server certificate is pinned.

**Can I get banned for playing on a community server?**
The game's online services are closed; the anti-cheat still runs inside the client. Our servers and test stands ran
without bans, but there is no guarantee — use your own copy at your own risk.

## Using the repository

**What do I need?**
Windows 10/11, ~3.5–5.5 GB for the base install, and for anything game-related your own copy of Dungeonborne on Steam.
A real server additionally needs a Linux VM and a physical Windows machine for the battle servers (the anti-cheat
refuses virtual machines). Details: [`README.md`](../README.md#requirements).

**What is the fastest way to start?**
Give the repository link to an AI assistant that can run commands and ask it to check and set it up — it follows
[`AGENTS.md`](../AGENTS.md). By hand: `bootstrap.ps1` (README, "Install").

**I found a vulnerability.**
Do not open a public issue: use the repository's private vulnerability reporting
([`SECURITY.md`](../.github/SECURITY.md)).

**How do I contribute?**
A branch, a small change, `cargo xtask ci` (fmt, clippy, tests, `ws check`), a pull request that says what changed and
how you checked it ([`CONTRIBUTING.md`](CONTRIBUTING.md)). Open problems: [`FIX_LIST.md`](FIX_LIST.md),
[`BACKLOG.md`](BACKLOG.md).
