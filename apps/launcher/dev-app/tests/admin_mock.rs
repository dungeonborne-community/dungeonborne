//! ADMIN_API.md contract: dev-app client against the built-in mock (random port).

use std::sync::Arc;

use dbdev::admin::Client;
use dbdev::mock::{self, Mock};

async fn start(require_relay: bool) -> (String, tokio::task::JoinHandle<()>) {
    let shared = Mock::new(0, require_relay, Arc::new(|_| {}));
    let h = mock::serve(shared.clone()).await.unwrap();
    let port = shared.lock().unwrap().port;
    (format!("http://127.0.0.1:{port}"), h)
}

#[tokio::test]
async fn players_and_commands() {
    let (base, h) = start(false).await;
    let http = dbl::net::client();
    let c = Client { http: &http, base: &base, token: None };

    let health = c.health().await.unwrap();
    assert!(health.ok && health.mock);
    assert_eq!(health.auth_require, "none");

    let players = c.players().await.unwrap();
    assert_eq!(players.len(), 3, "offline player is not listed");
    assert!(players.windows(2).all(|w| w[0].online_since <= w[1].online_since));
    let dk = players.iter().find(|p| p.nick == "DK").unwrap().open_id.clone();

    let r = c.gm(&dk, "gold 1000").await.unwrap();
    assert_eq!((r.result_code, r.result_list.clone()), (0, vec!["gold = 1500".to_string()]));
    let r = c.gm(&dk, "/GOLD -5000").await.unwrap();
    assert_eq!((r.result_code, r.result_message.as_str()), (-503, "not enough gold"));
    let r = c.gm(&dk, "item 100101 2").await.unwrap();
    assert_eq!(r.result_code, 0);
    assert!(r.result_list[0].starts_with("item 100101 x2"));
    let r = c.gm(&dk, "frobnicate").await.unwrap();
    assert_eq!(r.result_code, -501);
    assert!(c.gm(&dk, "help").await.unwrap().result_list.len() >= 7);

    // targets: offline players are allowed (saved to the DB), unknown ones are not; no open_id = the only online
    let r = c.gm("76561198000000004", "gold 1").await.unwrap();
    assert_eq!((r.online, r.result_code), (Some(false), 0));
    let e = c.gm("nobody", "info").await.unwrap_err();
    assert_eq!(e.code, "admin.player_not_found");
    assert_eq!(c.gm("", "info").await.unwrap_err().code, "admin.bad_request", "three players online");

    // ban kicks the player out of the online list
    let noob = players.iter().find(|p| p.nick == "Noob").unwrap().open_id.clone();
    let r = c.gm(&noob, "ban DK cheating").await.unwrap();
    assert!(r.result_list[0].starts_with("banned 76561198000000903"), "{:?}", r.result_list);
    assert!(r.result_list[0].ends_with(": cheating"));
    assert_eq!(c.players().await.unwrap().len(), 2);
    assert_eq!(c.gm(&noob, "unban 76561198000000903").await.unwrap().result_code, 0);
    h.abort();
}

#[tokio::test]
async fn bearer_when_relay() {
    let (base, h) = start(true).await;
    let http = dbl::net::client();
    let no = Client { http: &http, base: &base, token: None };
    assert_eq!(no.health().await.unwrap().auth_require, "relay", "health needs no token");
    assert_eq!(no.players().await.unwrap_err().code, "admin.unauthorized");
    let bad = Client { http: &http, base: &base, token: Some("nope") };
    assert_eq!(bad.gm("76561198000000902", "help").await.unwrap_err().code, "admin.unauthorized");
    let gm = Client { http: &http, base: &base, token: Some(mock::GM_TOKEN) };
    assert_eq!(gm.players().await.unwrap_err().code, "admin.forbidden");
    let admin = Client { http: &http, base: &base, token: Some(mock::ADMIN_TOKEN) };
    assert_eq!(admin.players().await.unwrap().len(), 3);
    h.abort();
}

#[tokio::test]
async fn browser_and_rebinding_refused() {
    let (base, h) = start(false).await;
    let http = reqwest::Client::new();
    let status = |r: reqwest::Response| r.status().as_u16();

    let r = http.get(format!("{base}/admin/v1/players")).header("Origin", "https://evil.example").send().await.unwrap();
    assert_eq!(status(r), 403);
    let r = http.get(format!("{base}/admin/v1/players")).header("Host", "evil.example").send().await.unwrap();
    assert_eq!(status(r), 403);
    let r = http.post(format!("{base}/admin/v1/gm")).header("Content-Type", "text/plain").body(r#"{"open_id":"","cmd":"help"}"#).send().await.unwrap();
    assert_eq!(status(r), 415);
    let r = http.post(format!("{base}/admin/v1/gm")).json(&serde_json::json!({"cmd": 1})).send().await.unwrap();
    assert_eq!(status(r), 400);
    let r = http.get(format!("{base}/admin/v1/nope")).send().await.unwrap();
    assert_eq!(status(r), 404);
    h.abort();
}

#[tokio::test]
async fn client_refuses_non_loopback() {
    let http = dbl::net::client();
    let c = Client { http: &http, base: "https://example.org", token: Some("secret") };
    assert_eq!(c.players().await.unwrap_err().code, "admin_not_loopback");
}

/// Shapes of the backend draft (backups/src/admin_api.rs, 02.11): bare array with nulls, text-only errors.
#[tokio::test]
async fn backend_draft_shapes() {
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    use axum::Json;
    let app = axum::Router::new()
        .route(
            "/admin/v1/players",
            get(|| async { Json(serde_json::json!([{"open_id": "1", "user_id": null, "nick": null, "hall_role_guid": null}])) }),
        )
        .route("/admin/v1/gm", post(|| async { (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "no admin role"}))) }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let h = tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    let http = dbl::net::client();
    let c = Client { http: &http, base: &base, token: None };
    let p = c.players().await.unwrap();
    assert_eq!((p[0].open_id.as_str(), p[0].user_id, p[0].nick.as_str()), ("1", 0, ""));
    let e = c.gm("1", "info").await.unwrap_err();
    assert_eq!((e.code.as_str(), e.detail.as_str()), ("admin.forbidden", "no admin role"));
    h.abort();
}

#[tokio::test]
async fn host_commands() {
    let shared = Mock::new(0, false, Arc::new(|_| {}));
    let h = mock::serve(shared.clone()).await.unwrap();
    let base = format!("http://127.0.0.1:{}", shared.lock().unwrap().port);
    let http = dbl::net::client();
    let c = Client { http: &http, base: &base, token: None };

    assert!(c.host_status().await.unwrap().last_command.is_null());
    let r = c.host_command("portal").await.unwrap();
    assert_eq!((r.sent.as_str(), r.host_phase.as_str()), ("portal", Some("ready")));
    assert_eq!(r.result["result"], "ok: portals 3");
    let r = c.host_command("portal 76561198000000903").await.unwrap();
    assert_eq!(r.result["cmd"], "portal 76561198000000903");
    assert!(c.host_command("exits").await.unwrap().result["result"].as_str().unwrap().starts_with("ok"));
    assert!(c.host_command("probe").await.unwrap().result.is_null(), "probe writes no result");
    assert_eq!(c.host_status().await.unwrap().last_command["cmd"], "exits");

    // refused before sending; the server refuses the same with 400
    assert_eq!(c.host_command("quit").await.unwrap_err().code, "bad_host_cmd");
    let raw = http.post(format!("{base}/admin/v1/host/command")).json(&serde_json::json!({"cmd": "quit"})).send().await.unwrap();
    assert_eq!(raw.status().as_u16(), 400);

    shared.lock().unwrap().host = Some(serde_json::json!({"phase": "lobby"}));
    assert_eq!(c.host_command("exits").await.unwrap().result["result"], "rejected_not_ready");
    shared.lock().unwrap().host = None;
    assert_eq!(c.host_command("portal").await.unwrap_err().code, "admin.host_unavailable");
    h.abort();
}
