//! Game server status in a channel: a single message. Polled every `every_secs`, but the message is edited only when
//! the data changed (and every 10 minutes as a heartbeat) — a few dozen Discord requests a day instead of 1440.
//! The online count comes from the backend's public status (`GET /dbserver/status` → online, capacity, maintenance, motd)
//! or, the old way, from the admin API (`GET /admin/v1/players` → array of players). No backend changes needed.
//! Maintenance is shown separately and doesn't count toward uptime.
//! Each poll writes a `status_sample` record (time series). The database does the rest:
//! - an event on `status_sample` keeps in `status_state:main` the moment since which the server has been continuously online,
//!   and deletes samples older than 7 days;
//! - the 24h peak online and uptime are computed by a single query (`math::max`, array filter `[WHERE …]`).

use crate::Data;
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use std::time::Duration;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE status_sample SCHEMAFULL;
DEFINE FIELD OVERWRITE up ON status_sample TYPE bool;
DEFINE FIELD OVERWRITE online ON status_sample TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE at ON status_sample TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX OVERWRITE idx_status_sample_at ON status_sample COLUMNS at;
DEFINE TABLE OVERWRITE status_state SCHEMALESS;
DEFINE TABLE OVERWRITE kv SCHEMALESS;
DEFINE EVENT OVERWRITE status_sample_rollup ON status_sample WHEN $event = "CREATE" THEN {
    UPSERT status_state:main SET up_since = IF $after.up THEN (up_since OR $after.at) ELSE NONE END, last_at = $after.at;
    DELETE status_sample WHERE at < time::now() - 7d;
};
"#;

const SUMMARY: &str = r#"
LET $day = (SELECT up, online FROM status_sample WHERE at > time::now() - 24h);
RETURN {
    peak: math::max($day[WHERE up = true].online) OR 0,
    uptime: IF count($day) > 0 THEN <float>count($day[WHERE up = true]) / count($day) ELSE NONE END,
    up_since: IF status_state:main.up_since != NONE THEN time::unix(status_state:main.up_since) ELSE NONE END,
};
"#;

const HEARTBEAT: Duration = Duration::from_secs(600);

#[derive(Deserialize, Debug, Default)]
pub struct Summary {
    pub peak: i64,
    pub uptime: Option<f64>,
    pub up_since: Option<i64>,
}

pub async fn record(data: &Data, online: Option<usize>) {
    let r = data
        .db
        .db
        .query("CREATE status_sample SET up = $up, online = $online")
        .bind(("up", online.is_some()))
        .bind(("online", online.unwrap_or(0) as i64))
        .await
        .and_then(|r| r.check());
    if let Err(e) = r {
        eprintln!("[Status] sample write: {}", e);
    }
}

pub async fn summary(data: &Data) -> Summary {
    match data.db.db.query(SUMMARY).await {
        Ok(mut r) => r.take::<Option<Summary>>(1).ok().flatten().unwrap_or_default(),
        Err(e) => {
            eprintln!("[Status] summary: {}", e);
            Summary::default()
        }
    }
}

/// What the backend returned. None — no answer (down, error, wrong JSON)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snapshot {
    pub online: usize,
    pub capacity: Option<u32>,
    pub maintenance: bool,
    pub motd: Option<String>,
    /// Lobby tabs (backend with the instances field); empty — old backend
    pub instances: Vec<Instance>,
}

/// A lobby tab from /dbserver/status → instances
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Instance {
    pub id: u32,
    pub name: String,
    /// closed | idle | gather | starting | waitroom | battle
    pub phase: String,
    #[serde(default)]
    pub map: Option<String>,
    #[serde(default)]
    pub queued: u32,
    #[serde(default)]
    pub in_battle: u32,
    /// Phase timer (unix); for closed — the opening time
    #[serde(default)]
    pub ends_at: Option<i64>,
}

impl Snapshot {
    /// For deciding whether to edit the message: without timers (they tick on their own via <t:…:R>) so it isn't edited every minute
    fn key(&self) -> (usize, Option<u32>, bool, Option<String>, Vec<(u32, String, u32, u32)>) {
        let inst = self.instances.iter().map(|i| (i.id, i.phase.clone(), i.queued, i.in_battle)).collect();
        (self.online, self.capacity, self.maintenance, self.motd.clone(), inst)
    }
}

/// GET /dbserver/status response (extra fields are ignored)
#[derive(Deserialize)]
struct PublicStatus {
    online: usize,
    #[serde(default)]
    capacity: Option<u32>,
    #[serde(default)]
    maintenance: bool,
    #[serde(default)]
    motd: Option<String>,
    /// {"en": "...", "ru": "..."} — the message per language
    #[serde(default)]
    motd_i18n: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default)]
    instances: Vec<Instance>,
}

pub fn parse_public(body: &str) -> Option<Snapshot> {
    let s: PublicStatus = serde_json::from_str(body).ok()?;
    // EN and RU from motd_i18n (bilingual like the rest of the card), otherwise the plain motd
    let tr = |k: &str| s.motd_i18n.as_ref().and_then(|m| m.get(k)).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let motd = match (tr("en"), tr("ru")) {
        (Some(en), Some(ru)) if en != ru => Some(format!("{}
{}", en, ru)),
        (Some(one), _) | (None, Some(one)) => Some(one),
        (None, None) => s.motd.map(|m| m.trim().to_string()).filter(|m| !m.is_empty()),
    };
    Some(Snapshot { online: s.online, capacity: s.capacity, maintenance: s.maintenance, motd, instances: s.instances })
}

/// One lobby line: bold name — phase (bilingual) with counters · map
pub fn instance_line(i: &Instance) -> String {
    let timer = |t: Option<i64>| t.map(|t| format!(" <t:{}:R>", t)).unwrap_or_default();
    let state = match i.phase.as_str() {
        "closed" => format!("🔒 closed / закрыто{}", timer(i.ends_at)),
        "idle" => "💤 idle / пусто".to_string(),
        "gather" => format!("📝 queue / набор {}", i.queued),
        "starting" | "waitroom" => format!("⏳ starting / старт · {}", i.queued.max(i.in_battle)),
        "battle" => format!("⚔️ in battle / в бою {}", i.in_battle),
        other => other.to_string(),
    };
    let mut line = format!("**{}** — {}", i.name, state);
    if i.phase == "battle" && i.queued > 0 {
        line.push_str(&format!(" · queue / набор {}", i.queued));
    }
    if let Some(map) = i.map.as_deref().filter(|m| !m.is_empty()) {
        line.push_str(&format!(" · {}", map));
    }
    line
}

async fn fetch(data: &Data, http: &reqwest::Client) -> Option<Snapshot> {
    let cfg = data.cfg.status.as_ref()?;
    let (url, public) = match (&cfg.status_url, &cfg.players_url) {
        (Some(u), _) => (u, true),
        (None, Some(u)) => (u, false),
        (None, None) => return None,
    };
    let mut req = http.get(url);
    if !public {
        if let Some(token) = cfg.bearer_env.as_ref().and_then(|v| std::env::var(v).ok()) {
            req = req.bearer_auth(token.trim());
        }
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[Status] {}: {}", url, e);
            return None;
        }
    };
    if !resp.status().is_success() {
        eprintln!("[Status] {} → {}", url, resp.status());
        return None;
    }
    let body = resp.text().await.ok()?;
    if public {
        parse_public(&body)
    } else {
        let players: Vec<serde_json::Value> = serde_json::from_str(&body).ok()?;
        Some(Snapshot { online: players.len(), ..Default::default() })
    }
}

fn render(data: &Data, snap: Option<&Snapshot>, s: &Summary) -> serenity::CreateEmbed {
    let cfg = data.cfg.status.as_ref().unwrap();
    let (state, color) = match snap {
        Some(x) if x.maintenance => ("🛠️ Maintenance / Техработы", 0xE67E22),
        Some(_) => ("🟢 Online", 0x2ECC71),
        None => ("🔴 Offline", 0xED4245),
    };
    let players = match snap {
        Some(x) => match x.capacity.or(cfg.capacity) {
            Some(cap) => format!("**{}** / {}", x.online, cap),
            None => format!("**{}**", x.online),
        },
        None => "—".into(),
    };
    let mut embed = serenity::CreateEmbed::new()
        .title(&cfg.title)
        .color(color)
        .field("Status / Статус", state, true)
        .field("Players / Игроки", players, true)
        .field("Peak 24h / Пик за сутки", s.peak.to_string(), true)
        .timestamp(serenity::Timestamp::now())
        .footer(serenity::CreateEmbedFooter::new("Updated / Обновлено"));
    if let Some(up) = s.uptime {
        embed = embed.field("Uptime 24h", format!("{:.1}%", up * 100.0), true);
    }
    if let (Some(since), Some(x)) = (s.up_since, snap) {
        if !x.maintenance {
            embed = embed.field("Online since / В сети с", format!("<t:{}:R>", since), true);
        }
    }
    if let Some(x) = snap.filter(|x| !x.instances.is_empty()) {
        let lines: Vec<String> = x.instances.iter().map(instance_line).collect();
        let mut text = lines.join("
");
        if text.chars().count() > 1024 {
            text = text.chars().take(1020).collect::<String>() + "…";
        }
        embed = embed.field("Lobbies / Лобби", text, false);
    }
    if let Some(motd) = snap.and_then(|x| x.motd.as_deref()) {
        embed = embed.field("Message / Сообщение", motd, false);
    }
    if let Some(addr) = &cfg.address {
        embed = embed.field("Address / Адрес", format!("`{}`", addr), false);
    }
    embed
}

async fn status_message(data: &Data) -> Option<serenity::MessageId> {
    let mut r = data.db.db.query("SELECT VALUE value FROM ONLY kv:status_message").await.ok()?;
    let id: Option<String> = r.take(0).ok()?;
    id?.parse::<u64>().ok().map(serenity::MessageId::new)
}

/// Edit our message; if it's gone (deleted, first run) — send a new one and remember it
async fn publish(ctx: &serenity::Context, data: &Data, channel: serenity::ChannelId, embed: serenity::CreateEmbed) {
    if let Some(id) = status_message(data).await {
        if channel.edit_message(&ctx.http, id, serenity::EditMessage::new().embed(embed.clone())).await.is_ok() {
            return;
        }
    }
    match channel.send_message(&ctx.http, serenity::CreateMessage::new().embed(embed)).await {
        Ok(msg) => {
            let _ = data.db.db.query("UPSERT kv:status_message SET value = $v").bind(("v", msg.id.to_string())).await;
        }
        Err(e) => eprintln!("[Status] send: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::testing;

    #[test]
    fn parses_public_status() {
        let body = r#"{"online":17,"capacity":64,"backend":"dbserver-rs 0.1.0","motd":"  ","motd_i18n":null,"maintenance":false,"rooms":4,"players_in_rooms":5}"#;
        assert_eq!(parse_public(body), Some(Snapshot { online: 17, capacity: Some(64), maintenance: false, motd: None, instances: vec![] }));
        let with = r#"{"online":5,"capacity":64,"maintenance":false,"motd":"","updated":1763214000,"instances":[
            {"id":3,"name":"Соло · Хард","mode":"solo","party_size":1,"difficulty":"hard","phase":"battle","level_id":40003019,"map":"Склеп (Hard)","queued":0,"in_battle":4,"seats":18,"ends_at":1763215000},
            {"id":6,"name":"Трио","mode":"trio","party_size":3,"difficulty":null,"phase":"closed","level_id":null,"map":null,"queued":0,"in_battle":0,"seats":18,"ends_at":1763216000}]}"#;
        let snap = parse_public(with).unwrap();
        assert_eq!(snap.instances.len(), 2);
        assert_eq!(instance_line(&snap.instances[0]), "**Соло · Хард** — ⚔️ in battle / в бою 4 · Склеп (Hard)");
        assert_eq!(instance_line(&snap.instances[1]), "**Трио** — 🔒 closed / закрыто <t:1763216000:R>");
        let mut later = snap.clone();
        later.instances[0].ends_at = Some(1);
        assert_eq!(snap.key(), later.key(), "a timer is no reason to edit the message");
        let m = parse_public(r#"{"online":0,"maintenance":true,"motd":"Back at 18:00"}"#).unwrap();
        assert!(m.maintenance);
        assert_eq!(m.motd.as_deref(), Some("Back at 18:00"));
        let i18n = parse_public(r#"{"online":1,"maintenance":true,"motd":"Техработы","motd_i18n":{"en":"Maintenance","ru":"Техработы"}}"#).unwrap();
        assert_eq!(i18n.motd.as_deref(), Some("Maintenance
Техработы"));
        assert_eq!(parse_public("<html>"), None);
    }

    #[tokio::test]
    async fn samples_rollup_into_summary() {
        let (data, dir) = testing::data("status", "").await;
        assert!(status_message(&data).await.is_none());
        let empty = summary(&data).await;
        assert_eq!((empty.peak, empty.uptime, empty.up_since), (0, None, None));

        record(&data, Some(5)).await;
        record(&data, None).await;
        let down = summary(&data).await;
        assert_eq!(down.up_since, None, "the online streak resets after going down");

        record(&data, Some(7)).await;
        record(&data, Some(3)).await;
        let s = summary(&data).await;
        assert_eq!(s.peak, 7);
        assert!((s.uptime.unwrap() - 0.75).abs() < 1e-9, "{:?}", s.uptime);
        let since = s.up_since.expect("online since the third sample");
        assert!((serenity::Timestamp::now().unix_timestamp() - since).abs() < 60);

        data.db.db.query("UPSERT kv:status_message SET value = '123'").await.unwrap();
        assert_eq!(status_message(&data).await, Some(serenity::MessageId::new(123)));
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

pub fn spawn(ctx: serenity::Context, data: Data) {
    let Some(cfg) = &data.cfg.status else { return };
    let Some(channel) = data.names.need_channel(&cfg.channel, "Status") else { return };
    let Some(url) = cfg.status_url.clone().or_else(|| cfg.players_url.clone()) else {
        eprintln!("[Status] no status_url or players_url — module disabled");
        return;
    };
    let http = if cfg.accept_invalid_certs {
        match reqwest::Client::builder().danger_accept_invalid_certs(true).timeout(Duration::from_secs(10)).build() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[Status] HTTP client: {}", e);
                return;
            }
        }
    } else {
        data.http.clone()
    };
    let every = Duration::from_secs(cfg.every_secs.max(15));
    println!("[Status] {} every {} s → #{}", url, every.as_secs(), channel);
    tokio::spawn(async move {
        let mut was: Option<&'static str> = None;
        // the message is edited only when the data changes (and once per HEARTBEAT, to show the bot is alive)
        let mut shown: Option<(Option<(usize, Option<u32>, bool, Option<String>, Vec<(u32, String, u32, u32)>)>, i64, Option<i64>, Option<i64>)> = None;
        let mut last_edit = std::time::Instant::now();
        loop {
            let snap = fetch(&data, &http).await;
            let now = match &snap {
                Some(x) if x.maintenance => "under maintenance",
                Some(_) => "online",
                None => "unavailable",
            };
            if was != Some(now) {
                println!("[Status] server {}", now);
                was = Some(now);
            }
            // maintenance doesn't count toward uptime: for players the server is unavailable
            record(&data, snap.as_ref().filter(|x| !x.maintenance).map(|x| x.online)).await;
            let s = summary(&data).await;
            let sig = (snap.as_ref().map(Snapshot::key), s.peak, s.uptime.map(|u| (u * 1000.0).round() as i64), s.up_since);
            if shown.as_ref() != Some(&sig) || last_edit.elapsed() >= HEARTBEAT {
                publish(&ctx, &data, channel, render(&data, snap.as_ref(), &s)).await;
                shown = Some(sig);
                last_edit = std::time::Instant::now();
            }
            tokio::time::sleep(every).await;
        }
    });
}
