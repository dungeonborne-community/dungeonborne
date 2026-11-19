//! Smoke check against a running dbserver-rs backend (not the mock): manifest parses, login can start.
//! Leaves only an in-memory pending login that expires by itself.
//!   cargo test -p dungeonborne-launcher --test backend_live -- --ignored
//! DBL_BACKEND overrides the base URL (default http://127.0.0.1:8090).

use dbl::{auth, manifest};

#[tokio::test]
#[ignore]
async fn manifest_and_login_start() {
    let base = std::env::var("DBL_BACKEND").unwrap_or_else(|_| "http://127.0.0.1:8090".into());
    let client = dbl::net::client();
    let m = manifest::fetch(&client, &format!("{base}/.well-known/dbserver.json")).await.unwrap();
    println!("server {} providers {:?} relay {:?}", m.id, m.auth.as_ref().map(|a| &a.providers), m.relay);
    let api = m.auth.expect("auth in manifest").api;
    let start = auth::start(&client, &api).await.unwrap();
    assert!(start.browser_url.contains(&start.login_id));
    assert!(matches!(auth::poll(&client, &api, &start.login_id).await.unwrap(), auth::Poll::Pending));
}

/// Patch notes as the launcher reads them from a running backend. DBL_NOTES=none — the backend has no notes file.
#[tokio::test]
#[ignore]
async fn patch_notes() {
    let base = std::env::var("DBL_BACKEND").unwrap_or_else(|_| "http://127.0.0.1:8090".into());
    let client = dbl::net::client();
    let m = manifest::fetch(&client, &format!("{base}/.well-known/dbserver.json")).await.unwrap();
    let notes = dbl::notes::fetch(&client, &m, "en").await.unwrap();
    if std::env::var("DBL_NOTES").as_deref() == Ok("none") {
        assert!(m.patch_notes.is_none() && notes.is_none(), "no file: no field, no notes, no error");
        return;
    }
    let latest = m.patch_notes.expect("patch_notes in manifest").latest;
    let n = notes.expect("notes");
    println!("notes: latest {latest}, {} updates, first «{}»", n.updates.len(), n.updates[0].title);
    assert_eq!(n.latest, latest, "the manifest and the notes agree on the newest entry");
    assert!(n.updates[0].sections.iter().all(|s| !s.items.is_empty()));
    println!("first entry in {:?}", n.updates[0].lang);
}

/// The battle tunnel switch as the launcher calls it. Needs a session token of a running backend: DBL_TOKEN
/// (dev login, see docs/LAUNCHER_SPEC.md §14). Leaves the tunnel off.
#[tokio::test]
#[ignore]
async fn battle_tunnel_switch() {
    let base = std::env::var("DBL_BACKEND").unwrap_or_else(|_| "http://127.0.0.1:8090".into());
    let token = std::env::var("DBL_TOKEN").expect("DBL_TOKEN");
    let client = dbl::net::client();
    let m = manifest::fetch(&client, &format!("{base}/.well-known/dbserver.json")).await.unwrap();
    let api = m.auth.expect("auth in manifest").api;
    let on = auth::net_set_udp(&client, &api, &token, true, "backend_live test").await.unwrap();
    assert_eq!(on["udp_tunnel"], true);
    assert_eq!(auth::net_get(&client, &api, &token).await.unwrap()["udp_tunnel"], true);
    let off = auth::net_set_udp(&client, &api, &token, false, "backend_live test").await.unwrap();
    assert_eq!(off["udp_tunnel"], false);
    assert_eq!(auth::net_get(&client, &api, &dbl::net::unix_now().to_string()).await.unwrap_err().code, "api.expired", "a wrong token is refused");
}

/// Bug report against a running backend with the `dev` login (probe/auth_test/server_launcher_dev.toml): a test
/// account logs in through the dev page, sends a report, reads its status. With DBL_ADMIN (admin API base, e.g.
/// http://127.0.0.1:9092/admin/v1; DBL_ADMIN_BEARER = an admin session when `[auth] require = "relay"`) it also plays the
/// bot: lists new reports, marks this one posted and sees the link come back.
///   cargo test -p dungeonborne-launcher --test backend_live -- --ignored bug_report
#[tokio::test]
#[ignore]
async fn bug_report() {
    use dbl::bugreport;
    let base = std::env::var("DBL_BACKEND").unwrap_or_else(|_| "http://127.0.0.1:8090".into());
    let client = dbl::net::client();
    let m = manifest::fetch(&client, &format!("{base}/.well-known/dbserver.json")).await.unwrap();
    let api = m.auth.as_ref().expect("auth in manifest").api.clone();

    // dev login: the start page carries `state` in a hidden field
    let start = auth::start(&client, &api).await.unwrap();
    let page = client.get(&start.browser_url).send().await.unwrap().text().await.unwrap();
    let state = page.split("name=\"state\" value=\"").nth(1).and_then(|s| s.split('"').next()).expect("dev login page");
    let cb = reqwest::Url::parse_with_params(&format!("{base}/auth/dev/callback"), &[("state", state), ("name", "BugReportTest")]).unwrap();
    client.get(cb).send().await.unwrap().error_for_status().unwrap();
    let token = match auth::poll(&client, &api, &start.login_id).await.unwrap() {
        auth::Poll::Ok { session_token, .. } => session_token,
        other => panic!("{other:?}"),
    };

    let report = bugreport::report(&bugreport::Fields {
        source: "launcher",
        kind: "launcher",
        title: "Launcher test report (backend_live)",
        description: "Sent by the launcher's backend_live test.\nSecond line.",
        game_label: m.game.label.as_deref(),
        modset: &bugreport::manifest_modset(&m.mods),
        exe_sha256: &m.game.exe_sha256,
        os: &bugreport::os_name(),
        lang: "ru",
    });
    // DBL_REAL_LOGS=<game folder>: the real log tails of this PC, as the launcher collects them (only to a local backend)
    let files = match std::env::var("DBL_REAL_LOGS") {
        Ok(root) => {
            let src = bugreport::sources(&dbl::store::data_dir().join("logs"), Some(std::path::Path::new(&root)));
            let f = bugreport::collect(&src);
            println!("real logs: {:?}", f.iter().map(|(n, b)| (n.as_str(), b.len())).collect::<Vec<_>>());
            f
        }
        Err(_) => vec![("launcher.log".to_string(), b"1 [bug] test line\n".to_vec()), ("relay.log".to_string(), b"1 port=443 up=1\n".to_vec())],
    };
    let expected: Vec<String> = files.iter().map(|f| f.0.clone()).collect();
    let sent = auth::bug_report(&client, &api, &token, &report, files).await;
    let id = match sent {
        Ok(id) => id,
        // a test account sending more than 3 an hour (the test ran before): the server's limit works
        Err(e) if e.code == "api.rate_limited" => {
            println!("rate limited, retry in {} s", e.detail);
            auth::logout(&client, &api, &token).await.unwrap();
            return;
        }
        Err(e) => panic!("{e}"),
    };
    println!("report {id}");
    let st = auth::bug_report_status(&client, &api, &token, &id).await.unwrap();
    assert_eq!(st["status"], "new");

    if let Ok(admin_base) = std::env::var("DBL_ADMIN") {
        let admin = std::env::var("DBL_ADMIN_BEARER").unwrap_or_default();
        let list: serde_json::Value = client
            .get(format!("{admin_base}/bug-reports?status=new&limit=10"))
            .bearer_auth(&admin)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let mine = list.as_array().unwrap().iter().find(|r| r["id"] == id.as_str()).expect("the report in status=new").clone();
        println!("admin view: {mine}");
        assert_eq!(mine["source"], "launcher");
        assert_eq!(mine["versions"]["launcher"], env!("CARGO_PKG_VERSION"));
        let names: Vec<&str> = mine["files"].as_array().unwrap().iter().filter_map(|f| f["name"].as_str()).collect();
        assert_eq!(names, expected);
        let url = format!("https://discord.com/channels/1/2/{id}");
        let r = client
            .post(format!("{admin_base}/bug-reports/{id}/posted"))
            .bearer_auth(&admin)
            .json(&serde_json::json!({ "thread_id": "2", "url": url }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 204);
        let st = auth::bug_report_status(&client, &api, &token, &id).await.unwrap();
        assert_eq!(st["status"], "posted");
        assert_eq!(st["url"], url.as_str());
    }
    auth::logout(&client, &api, &token).await.unwrap();
}
