---
name: dungeonborne-hosts-alternative
description: "11.11–12.11: works without hosts (stand 12.11: login, directory, lobby); enabled by [relay] no_hosts in server.toml; how to do without editing hosts — where the game takes addresses from (ServerConfig.json in pak, INTL encrypted), libcurl INTL reads *_proxy"
metadata:
  visibility: team
  node_type: memory
  type: project
---

The launcher writes 10 domains → 127.0.0.1 into hosts (launcher/src-tauri/src/hosts.rs); antiviruses complain (HostsFileHijack). Analysis 11.11 (the user asked to get rid of hosts):

- `Dungeonborne/Content/AdditionalData/ServerConfig.json` (pakchunk0, a normal .pak, repak): ds_config/marquee/banner on f.tgf123.com, http_report sg.api.tgf123.com, smartlink, tdm, `maple_url` (dir.…dmpplat.com). Our mod ru.rebuild-private already replaces it (DebugConfig.json with maple_url=127.0.0.1) — the other addresses can be done the same way.
- INTL login (sg.intlgame.com): the INTL config is encrypted (`[/Script/INTLSDK.Settings] bEnableConfigIniEncryption=True`, `Binaries/Win64/INTLConfig.ini.new` — ciphertext); a mod cannot change it.
- `INTLFoundation.dll` contains libcurl with `http_proxy/ALL_PROXY/NO_PROXY` → login probably goes through a proxy from the game's environment variables. Not verified by a run.
- Catch: "Run the game as administrator" (ShellExecuteEx runas, on by default) — the elevated process most likely does not inherit the launcher's environment variables.
- The game fonts are there too, in .pak as `.ufont` (plain TTF/OTF): Libre Baskerville + Noto Serif (Cyrillic), OFL.

**Experiment 11.11 (launcher branch `no-hosts`, cf2afa1, prod, hosts kept):** proxy 127.0.0.1:27778 + http(s)_proxy/ALL_PROXY for the game. As administrator (runas) — the variables did not arrive, the proxy saw nothing. Without elevation — only ACE went through the proxy (`down.anticheatexpert.com`, and the proxy wrongly sent it to our server); INTL login, game HTTPS, dir 8085 and lobby 6601 — directly via hosts. Conclusion: an env proxy does not work for the game, do not merge the branch. Dir returns the lobby as 127.0.0.1 (backend dir.rs) and maple_url=127.0.0.1 — dmpplat domains in hosts are no longer needed; hosts is unavoidable only for intlgame.com (login).

**Plain INTL config (11.11, analysis of INTLCore.dll, no cryptography — the user forbade touching the cipher):** INIConfig::GetFilePathByProject sets the cipher flags (+0x18/+0x19) only if `INTLConfig.ini.new` exists; otherwise it writes "Encrypted %s does not exist!!, attempting to use %s" and reads `INTLConfig.ini` as plain text (branch 0x21a0de, no decryption). Keys: INTL_URL (default https://sg.intlgame.com), ACCOUNT_URL, GAME_ID=30007 (from a capture), SDK_KEY, ACCOUNT_APP_ID/ACCOUNT_SDK_KEY. There is no original config text → a draft with a few keys, the rest default; not verified by a run.

**GCloud dir without hosts (11.11, verified by the orchestrator on the stand):** the dir address = `maple_url` in ServerConfig.json (original `dir.228918463-2-1.dmpplat.com`), GCloud picks the port itself (RpcAddressSvrPortList 8085|8080). TDir DMPConnector::parseUrl extracts the service id from the name `dir.<id>-2-1.<domain>` — a bare IP/localhost gives `service_url error`. `"maple_url": "dir.228918463-2-1.localtest.me"` works (public DNS → 127.0.0.1). A verbose GCloud log is enabled by the key `"force_sdk_log": true` in ServerConfig.json (Cache/GCloudSDKLog). Risk: localtest.me is someone else's domain; DNS with rebinding protection (routers, some ISPs) may not return 127.0.0.1 → better an own domain with A 127.0.0.1.

**How to apply:** next step (was) — an experiment on the stand: the game with HTTPS_PROXY/ALL_PROXY to the launcher's CONNECT proxy, NO_PROXY=127.0.0.1, ServerConfig via a mod; launching the game — only with the user's consent. Related to [[dungeonborne-ru-isp-freeze]], [[dungeonborne-launcher-auth]].

**12.11 verified on the stand without a single hosts line:** INTL login (plain INTLConfig.ini → launcher relay), directory `dir.228918463-2-1.dbserver.example.org` (A 127.0.0.1), lobby. Enabled by the server flag `[relay] no_hosts = true` (backend 55d63f5 → `relay.no_hosts` in the manifest), launcher 1.7.1 already supports it, players' ru.rebuild-private mod already has this address. Dev config: probe/auth_test/server_launcher_nohosts.toml. The stand's battle server (Steam install) will not log in without hosts by itself — hosts is needed for matches on the stand.
