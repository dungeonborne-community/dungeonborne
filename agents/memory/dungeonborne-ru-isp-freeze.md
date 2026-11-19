---
name: dungeonborne-ru-isp-freeze
description: "03.11 prod — some RU players' ISPs freeze relay TLS connections to Hetzner after ~1.5–2 KB; WARP fixes it; symptom CURLCode 28 on login"
metadata:
  visibility: public
  node_type: memory
  type: project
---

Found 2026-11-03 from players' `%LOCALAPPDATA%\DungeonborneLauncher\logs\relay.log` (player A <player> <ip>, player B
<player> <ip>): TCP to 203.0.113.10:7443 connects (Test-NetConnection True), but each relay connection freezes in both
directions after ~1.4–2 KB downstream: game-side curl gives up after ~19.5 s (`ms≈19500 reason=io down` with up=643
down=1422) → "Account service login failed (4,Network error,28,CURLCode)"; lobby 6601 stalls at ~2 KB down and the
game resets it after ~30 s. The server never sees the client's FIN/RST (closes after 500+ s with os error 110/104).
Same player via Cloudflare WARP (104.28.246.x) works perfectly. So: ISP interference with Hetzner, not our backend.

Side findings the same day: game hammers `f.tgf123.com` (~30 req/s total), rejecting our self-signed cert with
alert 48 unknown_ca; launcher `check_session` treats nginx 502 during a backend restart as a dead session
(`http_status` → creds deleted), so every deploy logs some players out.

06.11 test on an affected player's PC: `curl -k` of a 136 KB file from https://203.0.113.10 froze at 16218 bytes (30 s, code 200). MTU is not the cause: ping 1430 with DF passes, the home router reports its limit for 1472 by ICMP, and ~11 full segments got through. Fits the known ~16 KB freeze of TLS to foreign hosting; whether it hits non-TLS was not tested yet. Tests need that player's PC.
Same player 06.11, no WARP: TLS to 203.0.113.10 WITH any SNI (`curl --resolve cdn.dungeonborne.net:443:203.0.113.10`, also a Hetzner name) downloads fully; WITHOUT SNI (bare IP URL) froze twice at 16218 bytes; hel1-speed.hetzner.com HTTPS/HTTP fine; with WARP fine. So the freeze hits TLS without SNI. Launcher connects by IP, so rustls sends no SNI in both relay.rs and reqwest (net.rs). Not yet proven on relay port 7443 or on other ISPs.
Done 06.11 (approved): domain dbserver.example.org -> VM; /opt/dungeonborne/server.toml public_url and public_address = domain (backup server.toml.bak-ip-20261106), browser_url still http://203.0.113.10. Manifest now gives the domain, so old launchers send SNI too; launcher built-in manifest URL stays the IP (3.6 KB, under the freeze). Certificate unchanged (pinned). Let's Encrypt and the https login page not done yet.
Then the same player's lobby still froze on 7443 with SNI (server: bytes_acked 4730, retransmits), while nettest (launcher/tools/nettest, rustls) passed everywhere on 443/8443 including a lobby-like exchange. Relay moved to 8443 (server.toml bind + public_port, backup server.toml.bak-port7443-20261106): the player's lobby WORKS. So port 7443 itself was the problem for this player. Battle for this player: without WARP DS on Robot logs `Connection TIMED OUT` every ~65 s (client address seen, data requested, then silence); with WARP the same room works (06.11). UDP test (dbnettest 3) on this player's PC without WARP: on all 7 ports (7807, 443, 3478, 27015, 9000, 5000, 53) the server received only 11-12 of 455 packets, replies stopped at 0.4 s; from an unaffected connection ~all pass. So each UDP flow to Robot is cut after the first packets regardless of port: changing DS ports will not help. Whether other destinations are cut too is unknown.
Fix built 06.11: UDP tunnel through the relay, only for affected players (src/auth/udp_tunnel.rs, launcher relay.rs 127.0.0.1:27777, launcher 1.3.2, spec §LAUNCHER_SPEC; GM `udp on|off`, chat `/to <open_id> udp on`; auto-enable on network_block_loading). Verified in a real fight: player B plays through it without WARP. Auto-enable not verified yet.

08.11 second ISP pattern (player C <player>, another region and route): ANY TLS with SNI to any Hetzner IP (ours with any name, hel1-speed.hetzner.com) is RESET right after ClientHello (launcher: "error sending request for url .../login/start"); our SNI to 1.1.1.1 is fine (domain not blocked); TLS without SNI connects but freezes at 16218 bytes. So nothing direct to Hetzner works for this player — no launcher-side fallback can help (the launcher's "bypass" toggle = UDP tunnel via relay, also on Hetzner). Only VPN/WARP, or ingress outside Hetzner.

**How to apply:** for "timeout / CURL 28" reports ask for relay.log first; 19.5 s stalls = network freeze. Workaround
VPN/WARP; real fix candidate = relay ingress on a non-Hetzner (e.g. RU) host forwarding TCP to :7443 (cert pin is
end-to-end, so pass-through works). DS UDP on Robot 203.0.113.20 may need the same. See [[dungeonborne-launcher-auth]].
