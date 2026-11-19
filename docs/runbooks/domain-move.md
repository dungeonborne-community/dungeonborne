# Moving prod to dungeonborne-community.com

15.11.2026, branch `new-domain`. Before: prod `dbserver.example.org` (VM 203.0.113.10). After: backend and relay —
`dbserver.example.org`, the game directory without hosts — `dir.228918463-2-1.example.org`,
the main site — `dungeonborne-community.com` itself, the test site — `test.`, server status for the site — `status.`.

## Why no new certificate and no downtime

- The launcher accepts the backend and relay certificate **by pin** (sha256 of the leaf DER, `builtin::CERT_SHA256` and
  `relay.cert_sha256` in the manifest) and does not check the host name when the pin matches (`net.rs`, `Verifier`).
  The certificate `deploy/tls/backend.crt` (SAN only `IP:203.0.113.10`) works for any name — **the fingerprint does not
  change**, no new pin is needed.
- nginx on the VM: `server_name _` on 443 and 80 — answers any name, no nginx change is needed for `play.`.
- All addresses the launcher uses after loading the manifest (API, mods, launcher update, relay, notes) are built by
  the backend from `[web] public_url` and `[relay] public_address`. Change them in `server.toml` and old launchers
  immediately go to the new name; they need the old name only to download the manifest itself.
- New launcher (`builtin::SERVERS` with the new address): a player's server record saved under the old address gets
  the new one through the same `upsert` by manifest id — the new address is not yet in `builtin_added.json`.
- Discord login returns the browser to `[web] browser_url = "http://203.0.113.10"` (K17) — the redirect URI in Discord
  does not depend on the domain, the Discord application is not touched.

## DNS (at Squarespace)

| Type | Name (host) | Value | Purpose |
|---|---|---|---|
| A | `play` | `203.0.113.10` | backend: manifest, launcher API, mods, launcher updates, relay TCP 8443 |
| A | `dir.228918463-2-1` | `127.0.0.1` | game directory without hosts (mod ru.rebuild-private, `maple_url`); the game requires the form `dir.<id>-2-1.<domain>` |
| A | `status` | `203.0.113.10` | server status widget on the site (address chosen by the owner); needs a real certificate on the VM, see below |
| CNAME | `test` | `db-site-test.pages.dev` | test site (already exists) |
| CNAME | `www` | `<main site project>.pages.dev` | main site (Cloudflare Pages), see below |
| forwarding | `@` | → `https://www.dungeonborne-community.com` (301, path preserved) | domain root to the site, via Squarespace |
| TXT | `@` | `v=spf1 -all` | the domain sends no mail — third-party mail in its name is rejected |
| TXT | `_dmarc` | `v=DMARC1; p=reject;` | the same for DMARC |

TTL during the move — the minimum Squarespace allows; after adding, wait until the names resolve.

**Mail.** There is no mail: `noreply@dungeonborne-community.com` is only the commit author of the public edition; no
mail is sent from or received at it. MX is not needed. SPF/DMARC above are anti-spoofing protection, optional
(recommended).

**Main site — decided 15.11: `www` + root forwarding, DNS stays at Squarespace.** Cloudflare Pages attaches the
domain root only if the domain's DNS is in Cloudflare, so the site lives on `www`:

1. In Cloudflare, in the main site project (Pages → Custom domains), add `www.dungeonborne-community.com`; Pages will
   show which CNAME it expects. The main site project does not exist yet (only `db-site-test`) — the site session creates it.
2. In Squarespace: CNAME `www` → `<project>.pages.dev`; for the root — forwarding (Domain forwarding) to
   `https://www.dungeonborne-community.com`, permanent (301), with the path preserved — otherwise
   `/downloads/DungeonborneLauncher.exe` would go to the home page. Check that the root also forwards over **https**.
3. Check: `curl.exe -sI https://dungeonborne-community.com/downloads/DungeonborneLauncher.exe` → 301 to the same path
   on `www`. The browser carries the `#start` anchor over by itself.

For the site (not this branch): `SITE_URL` in `build_seo.py` is currently the root — with forwarding, the canonical
address should become `https://www.dungeonborne-community.com/` (canonical, og:url, sitemap must not point to the
forwarding address). Launcher and game links to the root (`https://dungeonborne-community.com/#start`) need no change —
the forwarding handles them.

If NS is ever moved to Cloudflare: all records of this table are created there; for `play`, `dir…`, `status` —
**DNS only, grey cloud** (the proxy would break the pin and port 8443, and it does not proxy 127.0.0.1).

**status.** A browser will not accept a self-signed certificate: `status.` needs a separate nginx `server` on the VM
with a Let's Encrypt certificate (and `location /.well-known/acme-challenge/` on 80) and CORS for the site. This is
site/VM work, not this branch; the `server_name _` block with the pin stays for `play.` and the IP.

## Zero-downtime order

1. **DNS** — the owner adds the records. Check: `Resolve-DnsName dbserver.example.org`,
   `Resolve-DnsName dir.228918463-2-1.example.org` (→ 127.0.0.1). Players notice nothing.
2. **Check the new name** before switching: `curl.exe -k https://dbserver.example.org/.well-known/dbserver.json`
   returns the same manifest as the old address (`-k` — the certificate is self-signed; the launcher checks it by pin).
3. **server.toml on the VM** (`/opt/dungeonborne/server.toml`, make a copy first): `[web] public_url =
   "https://dbserver.example.org"`, `[relay] public_address = "dbserver.example.org"`;
   leave `browser_url` unchanged. Restart `dbserver` at a quiet time (database copy before deploying — as usual). Old
   launchers: manifest by the old name → everything else by the new one.
4. **Launcher** from the branch — a new version (`SERVERS` with the new address; both directory names in the hosts
   block). Release the usual way (signature, `launcher` manifest); old launchers update themselves.
5. **Mod ru.rebuild-private** with the new `maple_url` — once most players have updated the launcher; at the same
   moment set `[relay] min_client` = the version from step 4. Reason: a launcher older than step 4 whose router drops
   127.0.0.1 answers writes only the old name to hosts and would not reach the directory with the new mod.
6. **After a month** — remove the records `dbserver.example.org` and `dir.228918463-2-1.dbserver.example.org`,
   then `OLD_DIR_DOMAIN` from `hosts.rs` and the reference to the old IP address, if not needed.

Until step 6 the old domain **must** respond: old launchers get the manifest through it, and the old mod gets the directory.

## Outside the repository

| Where | What | Action |
|---|---|---|
| VM `/opt/dungeonborne/server.toml` | `public_url`, `relay.public_address` | step 3 |
| `<data>\deploy\vm\server.toml` | local copy (outdated: no `no_hosts`) | update from the VM after step 3 |
| `deploy/tls/backend.crt` + key | self-signed, pinned | do not change |
| nginx on the VM | `server_name _` | do not change for `play.`; for `status.` — its own block with Let's Encrypt |
| Discord application | redirect `http://203.0.113.10/auth/discord/callback` | do not change |
| Discord bot, `/opt/discord-bot` | may contain links to the old domain | check with `grep -r old-domain` on the VM |
| Site (`site/`, `SITE_URL`) | currently `https://dungeonborne-community.com/` | for site sessions: `SITE_URL` → `https://www.…/`, Pages project for the main site with the `www` domain |
| DNS `example.org` | old records | keep until step 6 |
