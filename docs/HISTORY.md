# Development history

How the project got from a translated community patch to a playable server with its own tools. Dates are 2026;
details are in `docs/BACKLOG.md`, `docs/DS_PIPELINE.md`, `docs/CN_PATCH_IMPORTS.md` and the other documents named below.

## 1. Starting point: someone was still playing (October)

It began with a number: about a thousand players online in a game whose official servers were gone. They turned out
to be players from China, playing through community patches. Through Discord we found a person who had the Chinese
online patch, and he passed on everything the Chinese authors had given him: a PVP patch (network play) and a PVE
patch. The PVE patch was the key: it contained dumps of the game's RPC protocol (protobuf), the configuration tables,
and a working backend written in Go. What we reused from the patches, file by file, is in `docs/CN_PATCH_IMPORTS.md`.

## 2. Everything moved to Rust

That backend was rewritten in Rust (`core/backend`, `dbserver-rs`): the directory server, the gateway with the game's
own TGCP framing, and the RPC handlers built from the protocol dumps. By the end of October all 185 RPCs of the protocol
had handlers (`docs/HANDLERS_STATUS.md`): accounts and characters, stash and inventory, trading, mail, quests, rooms,
matchmaking. From the start the work was done together with AI agents.

## 3. Getting the rest out of the game

What the patches did not cover was extracted from a legally owned copy of the game: the client's Lua scripts, the
engine's type information (UE4SS header dumps and a usmap), current versions of the tables. Later the client executable
was examined through a memory image written by the engine's own crash-dump option. How each piece is obtained is
described as research in `docs/GAME_DATA.md` and `docs/owner/RE_TECHNIQUE.md`; the repository ships the method and the
findings, not the publisher's code.

## 4. Multiplayer: first peer-to-peer (late October)

The first multiplayer was peer-to-peer: one player hosted the match in their own game ("listen" mode), the others
joined. It worked, but the game was built for dedicated servers: abilities that check "am I the server" took the wrong
branch on the host (charged attacks, relic waves, souls), and a long list of fixes followed in the client mods.

## 5. From "listen" to dedicated servers (30.10 – 04.11)

On 30.10 the approach changed to "listen as dedicated": a headless game instance runs as the server and every player
connects as a client. On 31.10 the game's native match flow worked end to end (search → room → server). From 03.11 the
battle servers ran on a dedicated Windows machine with the backend in the cloud; on 04.11 the old local-match code was
removed and the native match became the only one (`docs/DS_PIPELINE.md`).

## 6. Fixing what broke, until it was playable (November)

The playtest went live in early November, and most of the work since then was fixing what players and logs showed:
item and affix rules, loot tables (`docs/LOOT_AUDIT.md`), the market, insurance, rooms, portals, crashes of the
server mods, network problems of players behind restrictive providers. Around it grew what a live server needs:
a launcher with Discord login and mod packages, seasons with their own rules and patch notes, balance tools, backups,
a status page and a community site.

## 7. A framework for working with AI agents (mid-November)

The last part is the workspace itself: `ws` and DevKit (setup, checks, secrets, the local test stand), roles and memory
for AI agents (`agents/`, `CLAUDE.md`), and editions for a team and for the public. The goal: a newcomer with an AI
assistant can set up the project and start fixing things in an evening. It was tested on real newcomers in November.
