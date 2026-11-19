use crate::config::ServerConfig;
use axum::{
    extract::{ConnectInfo, Query},
    routing::{any, post},
    Json, Router,
};
use axum_server::tls_rustls::RustlsConfig;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;

/// Handler for POST /tdm/v1/route (telemetry)
async fn handle_tdm_route() -> Json<Value> {
    Json(json!({
        "error_code": 0,
        "error_msg": "OK",
        "data": {
            "route": {},
            "config": { "enabled": false }
        }
    }))
}

/// Handler for POST /v2/conf/get_conf
async fn handle_get_conf(Query(params): Query<HashMap<String, String>>) -> Json<Value> {
    let seq = params.get("seq").cloned().unwrap_or_else(|| "1".to_string());
    Json(json!({
        "ret": 0,
        "msg": "",
        "seq": seq,
        "data": {
            "conf_version": "1",
            "conf": {},
            "switches": {},
            "switch": {},
            "region_strategy": {
                "show_user_agreement": 0,
                "finish_user_agreement": 1,
                "confirm_area": 1,
                "eu_user_agree_status": 1,
                "select_region_and_age": 1
            },
            "show_user_agreement": 0,
            "finish_user_agreement": 1,
            "eu_user_agree_status": 1,
            "agree_privacy_policy": 1,
            "user_agreed_game_tos": 1,
            "user_agreed_game_pp": 1,
            "user_agreed_li_tos": 1,
            "user_agreed_li_pp": 1,
            "user_agreed_li_dt": 1,
            "privacy_auth_sucess": 1,
            "terms_of_service": "",
            "privacy_policy": "",
            "protocal_version": "1",
            "terms_update_time": 0,
            "privacy_update_time": 0
        }
    }))
}

/// Handler for POST /v2/auth/get_bindlist_by_uid
async fn handle_get_bindlist(Query(params): Query<HashMap<String, String>>) -> Json<Value> {
    let seq = params.get("seq").cloned().unwrap_or_else(|| "1".to_string());
    Json(json!({
        "ret": 0,
        "msg": "",
        "seq": seq,
        "data": {
            "bind_list": [],
            "accounts": [],
            "can_bind": 1
        }
    }))
}

/// Handler for POST /v2/auth/login (extracts the player's steamid and user_name dynamically!)
async fn handle_auth_login(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(params): Query<HashMap<String, String>>,
    body: Option<Json<Value>>,
) -> Json<Value> {
    let seq = params.get("seq").cloned().unwrap_or_else(|| "1".to_string());

    // Extract not only steam_id and user_name but also the channelid the client sent
    let (steam_id, user_name, channel_id) = if let Some(Json(ref val)) = body {
        let sid = val["channel_info"]["steamid"]
            .as_str()
            .or_else(|| val["openid"].as_str())
            .unwrap_or("76561198000000901")
            .to_string();
        let uname = val["channel_info"]["user_name"]
            .as_str()
            .unwrap_or("Player")
            .to_string();
            
        // Try to get channelid from the request. If it is missing, fall back to 21 (Steam)
        let cid = val["channelid"].as_u64().unwrap_or(21); 
        
        (sid, uname, cid)
    } else {
        ("76561198000000901".to_string(), "Player".to_string(), 21)
    };

    // Via the launcher relay the identity is the session account; the SteamID claimed by the client is ignored (src/auth/registry.rs)
    let (steam_id, user_name) = match crate::auth::registry::admit(&peer) {
        Ok(Some(acc)) => (acc.open_id.clone(), acc.display_name.clone()),
        Ok(None) => (steam_id, user_name),
        Err(()) => {
            println!("[AuthHTTP] login {} not via the launcher relay ([auth] require = relay): denied", peer);
            return Json(json!({ "ret": 1, "msg": "login via launcher required", "seq": seq }));
        }
    };

    println!("[AuthHTTP] Login request from ID={} ({}) via Channel={}", steam_id, user_name, channel_id);

    let extra_json = "{\"get_status_ret\":0,\"need_notify_ret\":0,\"get_status_rsp\":{\"adult_check_status\":1,\"region\":\"702\",\"need_realname_auth\":0,\"is_eea\":0},\"need_notify_rsp\":{\"has_bind_li\":false,\"need_notify\":false}}";

    let status_rsp = json!({
        "adult_check_status": 1,
        "parent_certificate_status": 0,
        "eu_user_agree_status": 1,
        "adult_age": 18,
        "game_grade": 0,
        "certificate_type": 0,
        "region": "702",
        "email": "",
        "realname_auth_status": 1,
        "need_realname_auth": 0,
        "is_eea": 0,
        "is_dma": 0,
        "is_need_li_cert": 0
    });

    let notify_rsp = json!({
        "has_bind_li": false,
        "need_notify": false,
        "user_agreed_game_dma": "0"
    });

    // Use the dynamic channel_id
    let bind_list = json!([{
        "channelid": channel_id,
        "openid": steam_id,
        "user_name": user_name
    }]);

    Json(json!({
        "ret": 0,
        "msg": "success",
        "seq": seq,
        "openid": steam_id,
        "token": "LOCAL_FAKE_TOKEN",
        "token_expire_time": 4102444800u64,
        "first_login": 0,
        "reg_channel_dis": "",
        "user_name": user_name,
        "picture_url": "",
        "need_name_auth": false,
        "channel_info": "{}",
        "bind_list": bind_list,
        "confirm_code": "",
        "confirm_code_expire_time": 0,
        "channelid": channel_id, // <-- CHANGED
        "gender": 0,
        "birthday": "1990-01-01",
        "pf_key": "",
        "legal_doc": "",
        "email": "",
        "del_account_status": 0,
        "del_account_info": "",
        "del_li_account_status": 0,
        "transfer_code": "",
        "transfer_code_expire_time": 0,
        "is_dma": 0,
        "get_status_ret": 0,
        "get_status_rsp": status_rsp,
        "need_notify_ret": 0,
        "need_notify_rsp": notify_rsp,
        "extra_json": extra_json,
        "data": {
            "openid": steam_id,
            "token": "LOCAL_FAKE_TOKEN",
            "token_expire_time": 4102444800u64,
            "first_login": 0,
            "reg_channel_dis": "",
            "user_name": user_name,
            "picture_url": "",
            "need_name_auth": false,
            "channel_info": "{}",
            "bind_list": bind_list,
            "confirm_code": "",
            "confirm_code_expire_time": 0,
            "channelid": channel_id, // <-- CHANGED
            "gender": 0,
            "birthday": "1990-01-01",
            "pf_key": "",
            "legal_doc": "",
            "email": "",
            "del_account_status": 0,
            "del_account_info": "",
            "del_li_account_status": 0,
            "transfer_code": "",
            "transfer_code_expire_time": 0,
            "is_dma": 0,
            "get_status_ret": 0,
            "get_status_rsp": status_rsp,
            "need_notify_ret": 0,
            "need_notify_rsp": notify_rsp,
            "extra_json": extra_json,
            "is_login": 1,
            "login_auth_success": 1,
            "account_uid": steam_id,
            "channel_openid": steam_id,
            "access_token": "LOCAL_FAKE_ACCESS_TOKEN",
            "session_id": "LOCAL_FAKE_SESSION",
            "account_type": channel_id, // <-- CHANGED
            "login_type": channel_id,   // <-- CHANGED
            "login_channel": channel_id // <-- CHANGED
        }
    }))
}

/// AUTH_HTTP_TRACE=1: method, path, Host and response of every game request (login analysis without hosts, 11.11)
async fn trace_request(req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let host = req.headers().get("host").and_then(|v| v.to_str().ok()).unwrap_or("-").to_string();
    let line = format!("{} {} host={}", req.method(), req.uri(), host);
    let resp = next.run(req).await;
    println!("[AuthHTTP] {} -> {}", line, resp.status().as_u16());
    resp
}

/// Universal stub for the remaining methods (/v2/profile/get_bind_info, /v2/notice/*, /v2/lbs/ipregion)
async fn handle_generic_ok(Query(params): Query<HashMap<String, String>>) -> Json<Value> {
    let seq = params.get("seq").cloned().unwrap_or_else(|| "1".to_string());
    Json(json!({
        "ret": 0,
        "code": 0,
        "msg": "success",
        "seq": seq,
        "region": "RU",
        "alpha2": "RU",
        "data": {
            "region": "RU",
            "alpha2": "RU",
            "notice_list": []
        }
    }))
}

/// Starts the LevelInfinite HTTPS (443) and HTTP (80) auth server (replaces LoginService.exe)
pub async fn run_auth_http_server(cfg: ServerConfig) -> std::io::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // TLS_CERT / TLS_KEY: the local stand's own certificate (ws stand, tools/stand/run_local.ps1 — 18.11); otherwise
    // cert.pem / key.pem in ASSETS_DIR, as on the server
    let path = |env: &str, file: &str| {
        std::env::var_os(env)
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| cfg.assets_dir.join(file))
    };
    let cert_path = path("TLS_CERT", "cert.pem");
    let key_path = path("TLS_KEY", "key.pem");

    let tls_config = RustlsConfig::from_pem_file(&cert_path, &key_path)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "TLS for the login: {} / {}: {e} (TLS_CERT / TLS_KEY, or cert.pem and key.pem in ASSETS_DIR; \
                 the local stand: cargo ws stand --apply)",
                cert_path.display(),
                key_path.display()
            )
        });

    let app = Router::new()
        .route("/tdm/v1/route", post(handle_tdm_route))
        .route("/v2/conf/get_conf", post(handle_get_conf))
        .route("/v2/auth/get_bindlist_by_uid", post(handle_get_bindlist))
        .route("/v2/auth/login", post(handle_auth_login))
        .route("/v2/profile/get_bind_info", post(handle_generic_ok))
        .route("/v2/notice/get_notice_content", post(handle_generic_ok))
        .route("/v2/lbs/ipregion", post(handle_generic_ok))
        .fallback(any(handle_generic_ok));
    // 11.11: AUTH_HTTP_TRACE=1: every game request goes to the log (method, path, Host, response); for login analysis without hosts
    let app = if std::env::var("AUTH_HTTP_TRACE").is_ok_and(|v| v == "1") {
        app.layer(axum::middleware::from_fn(trace_request))
    } else {
        app
    };

    let https_addr: SocketAddr = format!("{}:443", cfg.bind_ip).parse().unwrap();
    let http_addr: SocketAddr = format!("{}:80", cfg.bind_ip).parse().unwrap();

    println!("[AuthHTTP] Listening on https://{} and http://{}", https_addr, http_addr);

    let app_http = app.clone();
    tokio::spawn(async move {
        let _ = axum_server::bind(http_addr)
            .serve(app_http.into_make_service_with_connect_info::<SocketAddr>())
            .await;
    });

    axum_server::bind_rustls(https_addr, tls_config)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
}