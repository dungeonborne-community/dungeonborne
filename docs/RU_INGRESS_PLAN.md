# Ingress node in Russia for players with ISP restrictions (option 1)

Draft for discussion with the team, 11.11.2026. The decisions at the end of the document are the team's.

## 1. Problem

Some players from Russia can't play normally: their ISPs (TSPU) cut connections to the Hetzner IP addresses where
our servers are — the VM with the backend (203.0.113.10, domain `dbserver.example.org`, before the move — `dbserver.example.org`) and Robot with the battle servers
(203.0.113.20). Our code is not to blame: the same players with Cloudflare WARP or a VPN play without problems.

What we saw on players' machines (03–08.11, details in the project memory note `dungeonborne-ru-isp-freeze`):

| Pattern | Players | What happens |
|---|---|---|
| Type 1: "freeze" | player A, player B | TCP and TLS connect, then the stream stops in both directions: after ~1.5–2 KB on port 7443, exactly at 16218 bytes for TLS without SNI. The socket stays "alive", the server sees it for 500+ s more. In the game: "Account service login failed (…28,CURLCode)", in the launcher's relay.log — `ms≈19500 reason=io down`. UDP to Robot is cut after the first 11–12 packets on any port |
| Type 2: "reset" | player C (another ISP) | Any TLS with SNI to any Hetzner IP (our domain, `hel1-speed.hetzner.com`) is reset right after ClientHello; without SNI — a freeze at 16218 bytes. Nothing gets through to Hetzner |

What has already been done and where it doesn't help:

- the relay was moved from 7443 to 8443, the relay address is a domain (the launcher sends SNI): it helped the lobby for a type 1 player;
- UDP match tunnel via the launcher relay (`core/backend/src/auth/udp_tunnel.rs`, the "bypass" switch): matches work for type 1;
- **for type 2 nothing works**: both the relay and the tunnel are the same Hetzner addresses;
- recommending WARP/VPN (a banner in the launcher — in progress). WARP has also started being restricted in Russia, and for a player it's
  an extra step.

Conclusion: as long as a player connects to a Hetzner address directly, we depend on the rules of a particular ISP, and they differ
and change. We need an entry point to which Russian ISPs don't cut traffic.

## 2. Idea

We rent a small VPS from a Russian host (an "ingress node"). The player connects to it: this is traffic
inside Russia, and TSPU doesn't touch it. The node forwards the bytes to Hetzner without decrypting.

```
 Player (RU)                      Ingress node (VPS in RU)                 Hetzner
 ┌──────────┐  TCP 443/8443        ┌──────────────────────┐   leg          ┌───────────────────────┐
 │ Launcher │ ────────────────────▶│ TCP forwarding       │ ─────────────▶ │ VM 203.0.113.10        │
 │ (relay,  │  (inside RU:         │ (no decryption,      │  (see sect. 4) │  nginx 443, relay 8443│
 │ UDP      │   TSPU leaves it)    │  PROXY protocol)     │                │  backend              │
 │ tunnel)  │                      └──────────────────────┘                └──────────┬────────────┘
 │ Game     │                                                                         │ UDP tunnel
 └──────────┘                                                                         ▼ (over 10.0.x.x)
                                                                           ┌───────────────────────┐
                                                                           │ Robot: battle servers │
                                                                           └───────────────────────┘
```

Why this is safe:

- The relay TLS and HTTPS are **end-to-end**: the certificate is pinned in the launcher, the node sees only the encrypted stream and
  can neither read nor tamper with the traffic. There are no keys or certificates on the node;
- the match goes through the relay UDP tunnel (already exists), i.e. also through the node. No need to open UDP separately on the node.

## 3. What goes through the node

| Traffic | Now | Via the node | How |
|---|---|---|---|
| Site and launcher API: login, manifest, mods, notes | HTTPS 443 on the VM (nginx) | 443 on the node → 443 VM | TCP forwarding; SNI stays our domain, certificate from the VM |
| Launcher relay: game ↔ lobby 6601–6604, game HTTP | TLS 8443 on the VM | 8443 on the node → 8443 VM | TCP forwarding |
| Match (UDP) | directly to Robot, for some players — the tunnel | only the tunnel via the relay | no change, the tunnel already goes over the relay |
| Discord login (browser) | `browser_url` = http://203.0.113.10 | a separate question, see section 9 | — |

Players without problems are not affected: they stay on the direct addresses.

## 4. Main risk: the "node → Hetzner" leg

Traffic between data centers is cut less often than home traffic, but there is no guarantee: there are known cases where TSPU also restricts
outgoing traffic from Russian data centers to foreign hosts. So this is **the first thing we check**, before any code
edits.

Options in increasing order:

1. **Direct TCP forwarding** (haproxy or nginx stream). The simplest. If node → Hetzner isn't cut — that's all.
2. **One permanent tunnel link node ↔ VM** that TSPU doesn't recognise:
   - **VLESS + Reality** (Xray): from outside looks like TLS to a large site, currently the most resilient option in Russia;
   - **AmneziaWG**: WireGuard with obfuscation. Plain WireGuard is often blocked in Russia, we don't use it.
   Inside the tunnel — the same TCP forwarding. One tunnel, between two of our servers, not a tunnel per
   player, so it can be set up and monitored reliably.
3. If this is cut too — a second node in another country, "closer" by TSPU rules (e.g. transit through another data center). That
   is already the extreme case.

Leg check: from the node — download a large file from the VM (megabytes, no drops), a long TLS connection to the relay
(minutes, with exchange), the same test without SNI and with SNI. The same 16218 bytes is a sign of a restriction.

## 5. Choosing a VPS

- A host in Russia: Selectel, Timeweb Cloud, Yandex Cloud, VK Cloud etc. Preferably Moscow or St Petersburg
  (lower latency to Helsinki).
- 1 vCPU, 1 GB RAM, Linux (Ubuntu). Forwarding bytes barely loads the CPU.
- Bandwidth from 100 Mbit/s, traffic without a hard limit or from 1–2 TB per month.
- Public static IPv4.
- Budget guideline: 300–600 RUB per month.

Traffic estimate: a match is on the order of 10–30 KB/s per player, lobby and login — little. 30 concurrent players via the node —
about 1 MB/s (~8 Mbit/s), i.e. with a large margin.

Latency: player → Moscow → Helsinki adds roughly 10–20 ms to the direct route. For players for whom nothing works
at all now, this doesn't matter; for the rest the node is not enabled.

## 6. Changes in our code

The minimal set to make the node work:

1. **The player's real IP.** Without changes the backend would see all node players from one address. This breaks logs, bans
   and per-IP limits. Solution: the node passes the player's IP via **PROXY protocol v2** (a haproxy, nginx stream standard).
   - Relay (`core/backend/src/auth/relay.rs`): read the PROXY header only from trusted node addresses (a list in
     `server.toml`), from others — as now;
   - nginx on the VM: `listen 443 proxy_protocol` for the node address and `set_real_ip_from`.
2. **Address list in the manifest** (`server.toml` → launcher manifest): besides the main relay and site address — the
   ingress node addresses, e.g. `[[ingress]] name = "ru-msk" address = "ru.dungeonborne-community.com" relay_port = 8443
   web_port = 443`. The field is invisible to old launchers, compatibility isn't broken.
3. **Launcher:**
   - "ISP is cutting" detection (a launcher session is doing it now for the WARP banner) — the same function;
   - when it fires — switch to the node address, remember the choice, show the player that a "bypass
     route" is on;
   - a manual switch in settings: "Auto / Direct / Via RU node";
   - startup manifest: if the first request to the main address stalled or was reset — retry via the node.
     The manifest address built into the launcher is now the VM IP. A built-in fallback node address is needed.
4. **Monitoring:** in the backend log — how many connections came via the node (by the PROXY header); on `/admin`
   — the leg state (node responds, latency).

The route itself can be checked without code changes: a node with forwarding + a manual address edit in the launcher settings of
an affected player (or a hosts file with the domain pointing at the node IP). PROXY protocol is not needed for such a check.

## 7. Operations

- Node config (haproxy/nginx, if needed Xray/AmneziaWG) — in the repository, `deploy/ingress-ru/`, with a README and
  an install script, like ds-agent.
- Node firewall: only 443 and 8443 open (and SSH by key, better — only from the VM). On the VM incoming from the node — from its
  IP.
- Backend watchdog (`[Watchdog]`): alert if the node doesn't respond or the leg latency grew.
- No need to renew the certificate on the node: it only forwards.
- If the node is down — the launcher of affected players tries the direct address (as now) and shows the WARP banner.

## 8. Stages and checks

| # | Step | Who | Done when |
|---|---|---|---|
| 1 | Renting a VPS in Russia | team (purchase) | there is an IP and SSH access |
| 2 | Checking the node → Hetzner leg (section 4) | us | a large file and a long TLS pass without drops, or it's clear a tunnel is needed |
| 3 | TCP forwarding 443/8443 on the node; a tunnel if needed | us | the VM site and relay are reachable from the node |
| 4 | Manual check with affected players: type 1 (player B), type 2 (player C) — node address in hosts/settings | players via the team | login, lobby and match without WARP |
| 5 | Code changes: PROXY protocol, `ingress` in the manifest, auto-switch in the launcher | us | tests, stand |
| 6 | Launcher release and backend deploy | on "yes" | affected players play without manual steps |
| 7 | Watching for a week: share of players via the node, drops, latency | us | a decision on whether a second node is needed |

Steps 2–4 answer "does the idea work at all" before any code changes.

## 9. Open questions for the team

1. **How many players are affected and with which ISPs?** Urgency depends on this. 3 players and 2 types of
   restrictions are known now.
2. **Where to get the VPS and who pays for it?** A hosting account for the team is needed, not for one person.
3. **Discord login.** Discord is blocked in Russia, and login through it for affected players most likely goes through their
   own VPN. The node doesn't solve this part. Whether a second login method is needed is a separate discussion (we don't touch Discord
   in this task).
4. **Battle servers directly.** A match via the relay tunnel goes through the node and the VM — an extra path and load on the VM. If
   there are many affected players — consider a separate UDP entry on the node to Robot.
5. **Legal side.** The node only forwards our game's encrypted traffic, with no VPN functions for the player.
   Check the hosting terms.
6. **Redundancy.** Do we need a second node at another host right away (if the first gets restricted or goes down)?

## 10. What we don't do and why

- **Splitting connections into pieces below the limit** (the relay itself reopens the connection every N KB): with a limit of
  ~1.5 KB that's hundreds of reconnects per minute, fragile, and it doesn't help against type 2 (immediate reset).
- **Disguising the launcher itself as someone else's site** (Reality in the launcher): a lot of work in the launcher and relay, and type 2
  cuts by Hetzner IP with any SNI — most likely won't help.
- **Moving everything from Hetzner to another foreign host:** the restrictions affect many foreign hosts
  at once, there's no guarantee, and moving is expensive.
