//! The same contract against a running backend (ADMIN_API.md §7):
//!   $env:ADMIN_URL='http://127.0.0.1:9092'; $env:ADMIN_TOKEN='<bearer, if require = relay>'
//!   cargo test -p dungeonborne-dev --test admin_live -- --ignored
//! Only `help` and `info` are sent (nothing is changed); both need someone online (the backend runs every
//! command in some player's context).

use dbdev::admin::Client;

#[tokio::test]
#[ignore]
async fn live_backend() {
    let base = std::env::var("ADMIN_URL").unwrap_or_else(|_| "http://127.0.0.1:9092".into());
    let token = std::env::var("ADMIN_TOKEN").ok();
    let http = dbl::net::client();
    let c = Client { http: &http, base: &base, token: token.as_deref() };
    // /health is optional (ADMIN_API.md §5)
    if let Ok(health) = c.health().await {
        assert!(health.ok);
        assert!(!health.mock, "this is the mock, not the backend");
        assert!(matches!(health.auth_require.as_str(), "none" | "relay"));
    }
    // read-only: no host command is sent
    let host = c.host_status().await.expect("GET /admin/v1/host/status");
    assert!(!host.root.is_empty());
    let players = c.players().await.expect("GET /admin/v1/players");
    assert_eq!(c.gm("76561190000000000", "info").await.unwrap_err().code, "admin.player_not_found");
    if players.is_empty() {
        // no open_id and nobody online → 400
        assert_eq!(c.gm("", "info").await.unwrap_err().code, "admin.bad_request");
        eprintln!("nobody online: info skipped");
        return;
    }
    let p = &players[0];
    assert_eq!(c.gm(&p.open_id, "help").await.expect("help").result_code, 0);
    let info = c.gm(&p.open_id, "info").await.expect("info");
    assert_eq!(info.result_code, 0);
    assert_eq!(info.open_id.as_deref(), Some(p.open_id.as_str()));
    assert!(info.result_list[0].contains(&format!("user_id={}", p.user_id)), "{:?}", info.result_list);
}
