//! End-to-end check of the launcher contract against the mock server. Needs a running mock:
//!   cargo run -p dbl-mock-server -- --demo-mod
//!   cargo test -p dungeonborne-launcher --test mock_e2e -- --ignored --test-threads=1
//! The relay listens on 20000+port so it does not clash with a local backend on 443/8085/6601.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use dbl::{auth, manifest, profile, relay};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MANIFEST: &str = "http://127.0.0.1:8787/.well-known/dbserver.json";

async fn login(client: &reqwest::Client, api: &str, nick: &str) -> (String, dbl::store::Account) {
    let start = auth::start(client, api).await.unwrap();
    assert!(matches!(auth::poll(client, api, &start.login_id).await.unwrap(), auth::Poll::Pending));
    // Stand-in for the player clicking "allow" in the browser.
    let cb = format!("http://127.0.0.1:8787/auth/discord/callback?login_id={}&nick={nick}&decision=ok", start.login_id);
    client.get(cb).send().await.unwrap().error_for_status().unwrap();
    match auth::poll(client, api, &start.login_id).await.unwrap() {
        auth::Poll::Ok { session_token, expires_at, account } => (session_token, account.into_account(expires_at).unwrap()),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
#[ignore]
async fn login_relay_refresh_logout() {
    let client = dbl::net::client();
    let m = manifest::fetch(&client, MANIFEST).await.unwrap();
    let api = m.auth.as_ref().unwrap().api.clone();
    let r = m.relay.clone().unwrap();
    assert!(r.cert_sha256.is_some(), "mock pins its relay certificate");

    let (token, account) = login(&client, &api, "E2E").await;
    assert_eq!(account.open_id.len(), 17);
    let (_, again) = login(&client, &api, "E2E").await;
    assert_eq!(again.open_id, account.open_id, "same Discord account -> same open_id");

    let logs = Arc::new(Mutex::new(Vec::<relay::ConnLog>::new()));
    let l2 = logs.clone();
    let sink: relay::Sink = Arc::new(move |c| l2.lock().unwrap().push(c));
    let cfg = |token: &str| relay::Config {
        address: r.address.clone(),
        port: r.port,
        cert_sha256: r.cert_sha256.clone(),
        token: token.to_string(),
        modset: "0".repeat(64),
    };
    let ports: Vec<(u16, u16)> = relay::PORTS.iter().map(|&p| (20000 + p, p)).collect();

    // Good token: bytes come back through the echoing mock relay.
    let rl = relay::start_mapped(cfg(&token), sink.clone(), &ports).await.unwrap();
    for (local, _) in [ports[1], ports[2], ports[3]] {
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", local)).await.unwrap();
        s.write_all(b"hello relay").await.unwrap();
        let mut buf = [0u8; 11];
        tokio::time::timeout(Duration::from_secs(5), s.read_exact(&mut buf)).await.unwrap().unwrap();
        assert_eq!(&buf, b"hello relay");
        s.shutdown().await.unwrap();
        let mut rest = Vec::new();
        s.read_to_end(&mut rest).await.unwrap();
    }
    // the same port on [::1]: without hosts the game reaches the server as https://localhost, which Windows resolves to ::1
    if let Ok(mut s) = tokio::net::TcpStream::connect(("::1", ports[1].0)).await {
        s.write_all(b"hello ipv6!").await.unwrap();
        let mut buf = [0u8; 11];
        tokio::time::timeout(Duration::from_secs(5), s.read_exact(&mut buf)).await.unwrap().unwrap();
        assert_eq!(&buf, b"hello ipv6!");
        s.shutdown().await.unwrap();
        s.read_to_end(&mut Vec::new()).await.unwrap();
    } else {
        println!("no IPv6 on this machine: [::1] not checked");
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    rl.stop().await;

    // Bad token: the mock refuses, the local socket is closed.
    let rl = relay::start_mapped(cfg("bogus"), sink.clone(), &ports).await.unwrap();
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", ports[1].0)).await.unwrap();
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), s.read_to_end(&mut buf)).await.unwrap();
    assert!(buf.is_empty());
    tokio::time::sleep(Duration::from_millis(300)).await;
    rl.stop().await;

    // Wrong pin: TLS fails, nothing reaches the mock.
    let mut bad = cfg(&token);
    bad.cert_sha256 = Some("ab".repeat(32));
    let rl = relay::start_mapped(bad, sink.clone(), &ports).await.unwrap();
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", ports[1].0)).await.unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(5), s.read_to_end(&mut Vec::new())).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    rl.stop().await;

    let logs = logs.lock().unwrap().clone();
    for l in &logs {
        println!("{l:?}");
    }
    assert!(logs.iter().filter(|l| l.reason == "closed" && l.up == 11 && l.down == 11).count() >= 3);
    assert!(logs.iter().any(|l| l.reason == "rejected: expired"));
    assert!(logs.iter().any(|l| l.reason.starts_with("tls:")));
    let dump = format!("{logs:?}");
    assert!(!dump.contains(&token), "token must never be logged");

    // Refresh rotates the token; logout kills it.
    let refreshed = auth::refresh(&client, &api, &token).await.unwrap();
    assert_ne!(refreshed.session_token, token);
    assert_eq!(auth::refresh(&client, &api, &token).await.unwrap_err().code, "api.expired");
    auth::logout(&client, &api, &refreshed.session_token).await.unwrap();
    assert_eq!(auth::refresh(&client, &api, &refreshed.session_token).await.unwrap_err().code, "api.expired");
}

/// Builds a fake install in a temp folder, applies the mock's profile (demo UE4SS mod, Goldberg), restores it.
#[tokio::test]
#[ignore]
async fn profile_apply_restore() {
    let client = dbl::net::client();
    let m = manifest::fetch(&client, MANIFEST).await.unwrap();
    let demo = m.mods.iter().find(|x| x.id == "mock.demo").expect("start the mock with --demo-mod");

    let root = std::env::temp_dir().join(format!("dbl-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let win64 = root.join("Dungeonborne/Binaries/Win64");
    let paks = root.join("Dungeonborne/Content/Paks");
    std::fs::create_dir_all(win64.join("ue4ss/Mods/DblDemo")).unwrap();
    std::fs::create_dir_all(paks.join("LogicMods")).unwrap();
    std::fs::write(win64.join("Dungeonborne.exe"), b"not really a game").unwrap();
    std::fs::write(win64.join("cacert.pem"), b"-----BEGIN CERTIFICATE-----\nORIG\n-----END CERTIFICATE-----\n").unwrap();
    std::fs::write(win64.join("ue4ss/Mods/DblDemo/old.lua"), b"players own copy").unwrap();
    std::fs::write(paks.join("pakchunk0-Windows.pak"), b"vanilla").unwrap();
    std::fs::write(paks.join("zz_Loot_P.pak"), b"foreign mod").unwrap();
    std::fs::write(paks.join("LogicMods/x.pak"), b"bp mod").unwrap();
    std::fs::write(win64.join("INTLConfig.ini.new"), b"encrypted intl config").unwrap();

    let inst = profile::Install::open(root.to_str().unwrap()).unwrap();
    let exe = profile::sha256_file(&inst.exe()).unwrap();
    let prepared = profile::prepare(&client, demo, MANIFEST, &exe, std::sync::Arc::new(|_, _| {})).await.unwrap();
    let set = vec![prepared];
    profile::resolve(&set).unwrap();
    let ca = b"-----BEGIN CERTIFICATE-----\nSERVER\n-----END CERTIFICATE-----\n";
    let plan = profile::Plan {
        server_id: &m.id,
        mods: &set,
        ca_pem: Some(ca),
        goldberg: Some(("76561197960265729", "E2E")),
        isolate_unlisted: true,
        intl_url: Some("https://localhost"),
        client_settings: None,
    };
    let _ = profile::restore(&inst);
    profile::apply(&inst, &plan).unwrap();

    assert!(profile::has_journal(&inst));
    assert!(win64.join("ue4ss/Mods/DblDemo/Scripts/main.lua").is_file());
    assert!(win64.join("ue4ss/Mods/DblDemo/enabled.txt").is_file());
    assert!(!win64.join("ue4ss/Mods/DblDemo/old.lua").exists(), "player's folder moved aside");
    assert!(!paks.join("zz_Loot_P.pak").exists() && !paks.join("LogicMods").exists(), "unlisted paks isolated");
    assert!(paks.join("pakchunk0-Windows.pak").exists(), "vanilla stays");
    assert!(std::fs::read_to_string(win64.join("cacert.pem")).unwrap().contains("SERVER"));
    assert_eq!(std::fs::read_to_string(win64.join("steam_settings/force_steamid.txt")).unwrap(), "76561197960265729");
    assert!(!win64.join("INTLConfig.ini.new").exists(), "the encrypted INTL config steps aside: the SDK reads the open one");
    assert!(std::fs::read_to_string(win64.join("INTLConfig.ini")).unwrap().contains("INTL_URL = https://localhost"));

    let diff = profile::restore(&inst).unwrap();
    assert!(diff.is_empty(), "{diff:?}");
    assert!(!profile::has_journal(&inst));
    assert_eq!(std::fs::read(win64.join("ue4ss/Mods/DblDemo/old.lua")).unwrap(), b"players own copy");
    assert!(!win64.join("ue4ss/Mods/DblDemo/Scripts").exists());
    assert_eq!(std::fs::read(paks.join("zz_Loot_P.pak")).unwrap(), b"foreign mod");
    assert!(paks.join("LogicMods/x.pak").exists());
    assert!(!std::fs::read_to_string(win64.join("cacert.pem")).unwrap().contains("SERVER"));
    assert!(!win64.join("steam_settings").exists());
    assert_eq!(std::fs::read(win64.join("INTLConfig.ini.new")).unwrap(), b"encrypted intl config");
    assert!(!win64.join("INTLConfig.ini").exists() && !win64.join("_dbl_disabled").exists());
    assert!(!root.join("Dungeonborne/Content/_dbl_disabled").exists());
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn relay_reports_busy_port() {
    let held = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = held.local_addr().unwrap().port();
    let cfg = relay::Config { address: "127.0.0.1".into(), port: 1, cert_sha256: None, token: "x".into(), modset: String::new() };
    let err = relay::start_mapped(cfg, Arc::new(|_| {}), &[(port, 443)]).await.err().unwrap();
    assert_eq!(err.code, "port_busy");
    assert!(err.detail.contains(&format!("pid {}", std::process::id())), "{}", err.detail);
}

/// Section 10 of the spec: `/me` after a relay drop, bans, `not_in_guild` with an invite link.
#[tokio::test]
#[ignore]
async fn me_ban_and_guild() {
    let client = dbl::net::client();
    let m = manifest::fetch(&client, MANIFEST).await.unwrap();
    let api = m.auth.unwrap().api;

    let (token, _) = login(&client, &api, "Banme").await;
    auth::me(&client, &api, &token).await.unwrap();
    client.get("http://127.0.0.1:8787/mock/ban?nick=Banme").send().await.unwrap();
    assert_eq!(auth::me(&client, &api, &token).await.unwrap_err().code, "api.expired");

    for (decision, nick, reason) in [("ok", "Banme", "banned"), ("guild", "Guest", "not_in_guild")] {
        let start = auth::start(&client, &api).await.unwrap();
        let cb = format!(
            "http://127.0.0.1:8787/auth/discord/callback?login_id={}&nick={nick}&decision={decision}",
            start.login_id
        );
        client.get(cb).send().await.unwrap();
        match auth::poll(&client, &api, &start.login_id).await.unwrap() {
            auth::Poll::Denied { reason: Some(r), message, link } => {
                assert_eq!(r, reason);
                assert!(message.is_some());
                assert_eq!(link.is_some(), reason == "not_in_guild");
            }
            other => panic!("{other:?}"),
        }
    }
}

/// Bug report (docs/specs/bug-report-api.md): multipart upload, status until the mock "bot" posts it (10 s),
/// 3 reports an hour per player.
#[tokio::test]
#[ignore]
async fn bug_report_post_status_limit() {
    use dbl::bugreport;
    let client = dbl::net::client();
    let m = manifest::fetch(&client, MANIFEST).await.unwrap();
    let api = m.auth.unwrap().api;
    // a fresh account per run: the limit counts reports of the last hour
    let nick = format!("Bug{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() % 1_000_000_000);
    let (token, _) = login(&client, &api, &nick).await;

    let report = bugreport::report(&bugreport::Fields {
        source: "launcher",
        kind: "game",
        title: "Inventory is not saved after the raid",
        description: "Picked up an item, exited through the blue portal, the stash is empty.",
        game_label: m.game.label.as_deref(),
        modset: &bugreport::manifest_modset(&m.mods),
        exe_sha256: &m.game.exe_sha256,
        os: &bugreport::os_name(),
        lang: "en",
    });
    let files = || vec![("launcher.log".to_string(), b"1 [bug] line\n".to_vec()), ("Dungeonborne.log".to_string(), vec![b'x'; 3 << 20])];
    let id = auth::bug_report(&client, &api, &token, &report, files()).await.unwrap();
    assert!(bugreport::valid_id(&id), "{id}");
    assert_eq!(auth::bug_report_status(&client, &api, &token, &id).await.unwrap()["status"], "new");
    tokio::time::sleep(Duration::from_secs(11)).await;
    let st = auth::bug_report_status(&client, &api, &token, &id).await.unwrap();
    assert_eq!(st["status"], "posted");
    assert!(st["url"].as_str().unwrap().starts_with("https://discord.com/"));

    auth::bug_report(&client, &api, &token, &report, vec![]).await.unwrap();
    auth::bug_report(&client, &api, &token, &report, vec![]).await.unwrap();
    let e = auth::bug_report(&client, &api, &token, &report, files()).await.unwrap_err();
    assert_eq!(e.code, "api.rate_limited");
    assert!(e.detail.parse::<u64>().unwrap() > 0);

    // the server refuses before reading 3 MB: the answer may be lost with the connection, the probe finds it
    for _ in 0..3 {
        assert_eq!(auth::bug_report(&client, &api, "bogus", &report, files()).await.unwrap_err().code, "api.expired");
        assert_eq!(auth::bug_report(&client, &api, &token, &report, files()).await.unwrap_err().code, "api.rate_limited");
    }
}
