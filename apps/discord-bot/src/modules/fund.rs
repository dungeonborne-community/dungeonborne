//! Server fundraising — one source for Discord, the site and the launcher.
//! - Month: Patreon subscriptions (API, automatic) + manual team amounts (`/fund add`, Boosty has no API) against `goal`.
//! - One-time goals: `/fund goal` creates, `/fund add … goal:` tops up, `/fund close` closes.
//! - Supporters: only with consent. With the `supporter_role` role a person runs `/supporter show` themselves (`hide` removes);
//!   people without Discord (Boosty) are added by the team with `/fund thank`. Name and month are shown, no amounts.
//! The result is one message in the channel and the `json_path` file (support.json, written atomically: temp file + rename),
//! which nginx on the VM serves as GET /dbserver/support. Patreon is polled every `every_secs`; commands
//! redraw immediately.

use crate::modules::patreon;
use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE fund_entry SCHEMAFULL;
DEFINE FIELD OVERWRITE amount ON fund_entry TYPE int;
DEFINE FIELD OVERWRITE source ON fund_entry TYPE string;
DEFINE FIELD OVERWRITE note ON fund_entry TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE month ON fund_entry TYPE string;
DEFINE FIELD OVERWRITE goal ON fund_entry TYPE option<string>;
DEFINE FIELD OVERWRITE by ON fund_entry TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE at ON fund_entry TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX OVERWRITE idx_fund_entry_month ON fund_entry COLUMNS month;
DEFINE TABLE OVERWRITE fund_goal SCHEMAFULL;
DEFINE FIELD OVERWRITE title_en ON fund_goal TYPE string;
DEFINE FIELD OVERWRITE title_ru ON fund_goal TYPE string;
DEFINE FIELD OVERWRITE target ON fund_goal TYPE int;
DEFINE FIELD OVERWRITE ends ON fund_goal TYPE option<datetime>;
DEFINE FIELD OVERWRITE open ON fund_goal TYPE bool DEFAULT true;
DEFINE FIELD OVERWRITE created ON fund_goal TYPE datetime DEFAULT time::now() READONLY;
DEFINE TABLE OVERWRITE supporter SCHEMAFULL;
DEFINE FIELD OVERWRITE name ON supporter TYPE string;
DEFINE FIELD OVERWRITE since ON supporter TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE visible ON supporter TYPE bool DEFAULT true;
-- month: total (with carry-over) and goal, for carrying over surplus; base and the last Boosty goal value
DEFINE TABLE OVERWRITE fund_month SCHEMALESS;
"#;

const MONTHS_EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const MONTHS_RU: [&str; 12] = ["январь", "февраль", "март", "апрель", "май", "июнь", "июль", "август", "сентябрь", "октябрь", "ноябрь", "декабрь"];

/// Commands redraw the message and the file immediately
static REFRESH: LazyLock<Notify> = LazyLock::new(Notify::new);

// ---------- calendar without external libraries (UTC) ----------

/// Days since 1970-01-01 → (year, month 1..12)
fn civil(days: i64) -> (i64, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m)
}

/// (year, month) → unix start of the month
fn month_start(y: i64, m: u32) -> i64 {
    let (y, m) = (if m <= 2 { y - 1 } else { y }, m as i64);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * 86_400
}

/// Current month: key "2026-11", end of month (unix), EN/RU labels
pub fn month_of(unix: i64) -> (String, i64, String, String) {
    let (y, m) = civil(unix.div_euclid(86_400));
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    (format!("{:04}-{:02}", y, m), month_start(ny, nm), MONTHS_EN[m as usize - 1].to_string(), MONTHS_RU[m as usize - 1].to_string())
}

fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// «Servers December» → «servers-december»
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(40).collect()
}

// ---------- support.json format (the site and the launcher read it too) ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct I18n {
    pub en: String,
    pub ru: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Part {
    pub en: String,
    pub ru: String,
    pub usd: f64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Goal {
    pub id: String,
    pub kind: &'static str,
    pub title: I18n,
    pub raised: f64,
    pub target: f64,
    pub ends_at: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub breakdown: Vec<Part>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub sources: std::collections::BTreeMap<String, f64>,
    /// raised ≥ target
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub covered: bool,
    /// How much above the goal
    #[serde(skip_serializing_if = "is_zero")]
    pub surplus: f64,
    /// From the surplus — to the server reserve (until the reserve is full)
    #[serde(skip_serializing_if = "is_zero")]
    pub to_reserve: f64,
    /// From the surplus — thanks to the team (the reserve is already full)
    #[serde(skip_serializing_if = "is_zero")]
    pub to_team: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Supporter {
    pub name: String,
    pub since: i64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Links {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patreon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boosty: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Support {
    pub currency: String,
    pub updated: i64,
    pub goals: Vec<Goal>,
    pub supporters: Vec<Supporter>,
    pub patrons: u32,
    /// Server reserve: accumulates from surplus up to cap, covers months that raised less than the goal
    pub reserve: Reserve,
    pub links: Links,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Reserve {
    pub amount: f64,
    pub cap: f64,
}

/// Reserve at the start of the month from past months' results: (total, goal) in cents, in order.
/// A month's shortfall is covered by the reserve; surplus tops it up to cap, the rest goes to the team.
pub fn reserve_after(months: &[(i64, i64)], cap: i64) -> i64 {
    let mut r = 0i64;
    for &(total, target) in months {
        r -= r.min((target - total).max(0));
        r += (total - target).max(0).min((cap - r).max(0));
    }
    r
}

/// This month's surplus: (to reserve, to team) given the reserve `before`
pub fn split_surplus(surplus: i64, before: i64, cap: i64) -> (i64, i64) {
    let to_reserve = surplus.max(0).min((cap - before).max(0));
    (to_reserve, surplus.max(0) - to_reserve)
}

fn usd(cents: i64) -> f64 {
    (cents as f64) / 100.0
}

/// Without `updated`: whether anything substantive changed
fn key(s: &Support) -> String {
    let mut c = s.clone();
    c.updated = 0;
    serde_json::to_string(&c).unwrap_or_default()
}

// ---------- data collection ----------

#[derive(Deserialize)]
struct SumRow {
    k: Option<String>,
    c: Option<i64>,
}

#[derive(Deserialize)]
struct GoalRow {
    id: String,
    title_en: String,
    title_ru: String,
    target: i64,
    ends: Option<i64>,
}

#[derive(Deserialize)]
struct SupRow {
    name: String,
    since: i64,
}

/// State from the database + the last Patreon read (cents, number of patrons, campaign link)
pub async fn build(data: &Data, patreon: Option<&patreon::Funding>, boosty: Option<i64>, now: i64) -> Result<Support, Error> {
    let cfg = data.cfg.fund.as_ref().ok_or("fund disabled")?;
    let (month, ends, m_en, m_ru) = month_of(now);
    let mut r = data
        .db
        .db
        .query("SELECT source AS k, math::sum(amount) AS c FROM fund_entry WHERE month = $m AND goal = NONE GROUP BY k")
        .query("SELECT goal AS k, math::sum(amount) AS c FROM fund_entry WHERE goal != NONE GROUP BY k")
        .query("SELECT record::id(id) AS id, title_en, title_ru, target, IF ends THEN time::unix(ends) ELSE NONE END AS ends, created FROM fund_goal WHERE open = true ORDER BY created")
        .query("SELECT name, time::unix(since) AS since FROM supporter WHERE visible = true ORDER BY since")
        .bind(("m", month.clone()))
        .await?;
    let by_source: Vec<SumRow> = r.take(0)?;
    let by_goal: Vec<SumRow> = r.take(1)?;
    let goals: Vec<GoalRow> = r.take(2)?;
    let sups: Vec<SupRow> = r.take(3)?;

    let mut sources = std::collections::BTreeMap::new();
    let mut raised = 0i64;
    if let Some(p) = patreon {
        raised += p.cents as i64;
        sources.insert("patreon".to_string(), usd(p.cents as i64));
    }
    for row in &by_source {
        let c = row.c.unwrap_or(0);
        raised += c;
        *sources.entry(row.k.clone().unwrap_or_else(|| "other".into())).or_insert(0.0) += usd(c);
    }
    if cfg.boosty_blog.is_some() {
        // Boosty goal growth since the start of the month (0 if Boosty hasn't been read yet)
        let b = match boosty {
            Some(cur) => boosty_month(data, &month, cur).await?,
            None => 0,
        };
        raised += b;
        *sources.entry("boosty".into()).or_insert(0.0) += usd(b);
        sources.entry("patreon".into()).or_insert(0.0);
    }
    // the month's total is stored: the server reserve is computed from past months
    let target_c = (cfg.goal * 100.0).round() as i64;
    let cap = (cfg.reserve_cap.unwrap_or(cfg.goal) * 100.0).round() as i64;
    let total = raised;
    let before = reserve_before(data, &month, cap).await?;
    let surplus = (total - target_c).max(0);
    let (to_reserve, to_team) = split_surplus(surplus, before, cap);
    data.db
        .db
        .query("UPSERT type::thing('fund_month', $m) SET total = $t, target = $g")
        .bind(("m", month.clone()))
        .bind(("t", total))
        .bind(("g", target_c))
        .await?
        .check()?;
    let mut out = vec![Goal {
        id: format!("servers-{}", month),
        kind: "monthly",
        title: I18n { en: format!("Servers — {}", m_en), ru: format!("Серверы — {}", m_ru) },
        raised: usd(total),
        target: cfg.goal,
        ends_at: Some(ends),
        breakdown: cfg.breakdown.iter().map(|b| Part { en: b.en.clone(), ru: b.ru.clone(), usd: b.usd }).collect(),
        sources,
        covered: target_c > 0 && total >= target_c,
        surplus: usd(surplus),
        to_reserve: usd(to_reserve),
        to_team: usd(to_team),
    }];
    for g in goals {
        let c = by_goal.iter().find(|r| r.k.as_deref() == Some(g.id.as_str())).and_then(|r| r.c).unwrap_or(0);
        out.push(Goal {
            id: g.id,
            kind: "one_time",
            title: I18n { en: g.title_en, ru: g.title_ru },
            raised: usd(c),
            target: usd(g.target),
            ends_at: g.ends,
            breakdown: vec![],
            sources: Default::default(),
            covered: g.target > 0 && c >= g.target,
            surplus: 0.0,
            to_reserve: 0.0,
            to_team: 0.0,
        });
    }
    Ok(Support {
        currency: cfg.currency.clone(),
        updated: now,
        goals: out,
        supporters: sups.into_iter().map(|s| Supporter { name: s.name, since: s.since }).collect(),
        patrons: patreon.map(|p| p.patrons).unwrap_or(0),
        reserve: Reserve { amount: usd(before + to_reserve), cap: usd(cap) },
        links: if cfg.coming_soon {
            Links { patreon: None, boosty: None }
        } else {
            Links { patreon: cfg.patreon_link.clone().or_else(|| patreon.and_then(|p| p.url.clone())), boosty: cfg.boosty_link.clone() }
        },
    })
}

/// «2026-11» → «2026-10», «2027-01» → «2026-12»
pub fn prev_month(m: &str) -> String {
    let (y, mo) = m.split_once('-').and_then(|(y, mo)| Some((y.parse::<i64>().ok()?, mo.parse::<u32>().ok()?))).unwrap_or((1970, 1));
    if mo <= 1 { format!("{:04}-12", y - 1) } else { format!("{:04}-{:02}", y, mo - 1) }
}

/// Reserve at the start of the month: past months in order (the "2026-11" key sorts like a date)
async fn reserve_before(data: &Data, month: &str, cap: i64) -> Result<i64, Error> {
    let mut r = data
        .db
        .db
        .query("SELECT record::id(id) AS m, total ?? 0 AS t, target ?? 0 AS g FROM fund_month WHERE record::id(id) < $m ORDER BY m")
        .bind(("m", month.to_string()))
        .await?;
    #[derive(Deserialize)]
    struct Row {
        t: i64,
        g: i64,
    }
    let rows: Vec<Row> = r.take(0)?;
    Ok(reserve_after(&rows.iter().map(|x| (x.t, x.g)).collect::<Vec<_>>(), cap))
}

/// Boosty goal growth over the month. The month's base is the last value of the previous month (or the current one if there is no history).
/// The goal was reset (sum below the base) — the base drops to the current sum.
async fn boosty_month(data: &Data, month: &str, cur: i64) -> Result<i64, Error> {
    let mut r = data
        .db
        .db
        .query("LET $prev = (SELECT VALUE boosty_last FROM ONLY type::thing('fund_month', $p));
                UPSERT type::thing('fund_month', $m) SET
                    boosty_base = math::min([boosty_base ?? ($prev ?? $cur), $cur]),
                    boosty_last = $cur
                RETURN VALUE boosty_base;")
        .bind(("p", prev_month(month)))
        .bind(("m", month.to_string()))
        .bind(("cur", cur))
        .await?;
    let base: Vec<i64> = r.take(1)?;
    Ok((cur - base.first().copied().unwrap_or(cur)).max(0))
}

/// Public Boosty goal: GET api.boosty.to/v1/target/<blog>/ → currencyCurrentSums.USD (Boosty converts from ₽ itself)
pub fn parse_boosty(body: &str, target_id: Option<u64>) -> Option<i64> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let t = v["data"].as_array()?.iter().find(|t| {
        t["type"].as_str() == Some("money") && target_id.is_none_or(|id| t["id"].as_u64() == Some(id))
    })?;
    let usd = t["currencyCurrentSums"]["USD"].as_f64()?;
    Some((usd * 100.0).round() as i64)
}

async fn fetch_boosty(http: &reqwest::Client, blog: &str, target_id: Option<u64>) -> Result<i64, String> {
    let url = format!("https://api.boosty.to/v1/target/{}/", blog);
    let r = http
        .get(&url)
        .header(reqwest::header::USER_AGENT, "DungeonborneCommunityBot (discord, 1)")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("{} → {}", url, r.status()));
    }
    let body = r.text().await.map_err(|e| e.to_string())?;
    parse_boosty(&body, target_id).ok_or_else(|| "the blog has no money goal (or the response format changed)".into())
}

/// Atomically: temp file next to it + rename, so the site never reads half a JSON
pub fn write_json(path: &str, s: &Support) -> std::io::Result<()> {
    let body = serde_json::to_vec_pretty(s).map_err(std::io::Error::other)?;
    let tmp = format!("{}.tmp", path);
    std::fs::write(&tmp, body)?;
    // on the VM nginx (www-data) reads via the directory group (setgid): file 0640, otherwise with umask 077 it would be 403
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o640))?;
    }
    std::fs::rename(&tmp, path)
}

// ---------- Discord ----------

fn money(v: f64, cur: &str) -> String {
    patreon::money((v * 100.0).round().max(0.0) as u64, cur)
}

pub fn render(cfg: &crate::config::FundCfg, s: &Support) -> (serenity::CreateEmbed, Vec<serenity::CreateActionRow>) {
    let cur = &s.currency;
    let month = &s.goals[0];
    let mut desc = String::new();
    if let Some(t) = &cfg.text {
        desc.push_str(t);
        desc.push_str("\n\n");
    }
    let to_c = |v: f64| (v * 100.0).round().max(0.0) as u64;
    desc.push_str(&format!("**{}** / {} · {}", money(month.raised, cur), money(month.target, cur), patreon::bar(to_c(month.raised), to_c(month.target))));
    let src = |k: &str| month.sources.get(k).copied();
    let mut parts: Vec<String> = vec![];
    for (k, label) in [("patreon", "Patreon"), ("boosty", "Boosty"), ("other", "Other / Другое")] {
        if let Some(v) = src(k) {
            parts.push(format!("{} {}", label, money(v, cur)));
        }
    }
    if !parts.is_empty() {
        desc.push_str(&format!("\n-# {}", parts.join(" · ")));
    }
    if month.covered {
        let (r, t) = (money(month.to_reserve, cur), money(month.to_team, cur));
        if month.to_reserve > 0.0 && month.to_team > 0.0 {
            desc.push_str(&format!("\n✅ **This month is covered!** Extra: {} to the server reserve, {} thanks the team.\n✅ **Месяц оплачен!** Сверх счёта: {} — в резерв серверов, {} — благодарность команде.", r, t, r, t));
        } else if month.to_reserve > 0.0 {
            desc.push_str(&format!("\n✅ **This month is covered!** Extra {} goes to the server reserve.\n✅ **Месяц оплачен!** Лишние {} пойдут в резерв серверов.", r, r));
        } else if month.to_team > 0.0 {
            desc.push_str(&format!("\n✅ **This month is covered!** The reserve is full: extra {} thanks the team.\n✅ **Месяц оплачен!** Резерв полон: лишние {} — благодарность команде.", t, t));
        } else {
            desc.push_str("\n✅ **This month is covered!** / **Месяц оплачен!**");
        }
    }
    if !month.breakdown.is_empty() {
        desc.push('\n');
        for p in &month.breakdown {
            desc.push_str(&format!("\n-# {} / {} — {}", p.en, p.ru, money(p.usd, cur)));
        }
    }
    let mut embed = serenity::CreateEmbed::new()
        .title(&cfg.title)
        .description(desc)
        .color(0xE0A84A)
        .footer(serenity::CreateEmbedFooter::new("Donations give nothing in the game / Донат ничего не даёт в игре · Updated / Обновлено"))
        .timestamp(serenity::Timestamp::now());
    let once: Vec<String> = s.goals[1..]
        .iter()
        .map(|g| {
            let mut line = format!("**{} / {}** — {} / {} · {}", g.title.en, g.title.ru, money(g.raised, cur), money(g.target, cur), patreon::bar(to_c(g.raised), to_c(g.target)));
            if let Some(t) = g.ends_at {
                line.push_str(&format!(" · <t:{}:R>", t));
            }
            line
        })
        .collect();
    if !once.is_empty() {
        embed = embed.field("One-time goals / Разовые цели", once.join("\n").chars().take(1024).collect::<String>(), false);
    }
    embed = embed.field("Patrons / Подписчики", s.patrons.to_string(), true);
    embed = embed.field("Server reserve / Резерв серверов", format!("{} / {}", money(s.reserve.amount, cur), money(s.reserve.cap, cur)), true);
    if !s.supporters.is_empty() {
        let names: Vec<&str> = s.supporters.iter().map(|x| x.name.as_str()).collect();
        let mut text = names.join(" · ");
        if text.chars().count() > 1000 {
            text = text.chars().take(1000).collect::<String>() + "…";
        }
        embed = embed.field("Thank you / Спасибо", text, false);
    }
    let mut buttons = vec![];
    if let Some(l) = &s.links.patreon {
        buttons.push(serenity::CreateButton::new_link(l).label("Patreon").emoji('💜'));
    }
    if let Some(l) = &s.links.boosty {
        buttons.push(serenity::CreateButton::new_link(l).label("Boosty").emoji('🧡'));
    }
    if cfg.coming_soon {
        embed = embed.field("Patreon · Boosty", "Coming soon / Скоро", true);
    }
    let rows = if buttons.is_empty() { vec![] } else { vec![serenity::CreateActionRow::Buttons(buttons)] };
    (embed, rows)
}

async fn message_id(data: &Data) -> Option<serenity::MessageId> {
    let mut r = data.db.db.query("SELECT VALUE value FROM ONLY kv:patreon_message").await.ok()?;
    let id: Option<String> = r.take(0).ok()?;
    id?.parse::<u64>().ok().map(serenity::MessageId::new)
}

async fn publish(ctx: &serenity::Context, data: &Data, channel: serenity::ChannelId, embed: serenity::CreateEmbed, rows: Vec<serenity::CreateActionRow>) {
    if let Some(id) = message_id(data).await {
        let edit = serenity::EditMessage::new().embed(embed.clone()).components(rows.clone());
        if channel.edit_message(&ctx.http, id, edit).await.is_ok() {
            return;
        }
    }
    match channel.send_message(&ctx.http, serenity::CreateMessage::new().embed(embed).components(rows)).await {
        Ok(msg) => {
            let _ = data.db.db.query("UPSERT kv:patreon_message SET value = $v").bind(("v", msg.id.to_string())).await;
        }
        Err(e) => eprintln!("[Fund] send: {}", e),
    }
}

pub fn spawn(ctx: serenity::Context, data: Data) {
    let Some(cfg) = &data.cfg.fund else { return };
    let Some(channel) = data.names.need_channel(&cfg.channel, "Fund") else { return };
    let token = std::env::var(&cfg.token_env).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    if token.is_none() {
        eprintln!("[Fund] no {} in .env — Patreon is not counted, manual amounts only", cfg.token_env);
    }
    let every = Duration::from_secs(cfg.every_secs.max(300));
    println!(
        "[Fund] goal ${} per month, Patreon every {} s → #{}{}",
        cfg.goal,
        every.as_secs(),
        channel,
        cfg.json_path.as_deref().map(|p| format!(", file {}", p)).unwrap_or_default()
    );
    tokio::spawn(async move {
        let mut last_patreon: Option<patreon::Funding> = None;
        let mut fetched: Option<Instant> = None;
        let mut last_boosty: Option<i64> = None;
        let mut boosty_at: Option<Instant> = None;
        let mut shown = String::new();
        loop {
            if let Some(t) = &token {
                if fetched.is_none_or(|f| f.elapsed() >= every) {
                    match patreon::fetch(&data.http, t).await {
                        Ok(f) => last_patreon = Some(f),
                        Err(e) => eprintln!("[Fund] Patreon: {}", e),
                    }
                    fetched = Some(Instant::now());
                }
            }
            let fcfg = data.cfg.fund.as_ref().unwrap();
            if let Some(blog) = &fcfg.boosty_blog {
                // Boosty — every 15 minutes (public page, no token)
                if boosty_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(900)) {
                    match fetch_boosty(&data.http, blog, fcfg.boosty_target_id).await {
                        Ok(c) => last_boosty = Some(c),
                        Err(e) => eprintln!("[Fund] Boosty: {}", e),
                    }
                    boosty_at = Some(Instant::now());
                }
            }
            match build(&data, last_patreon.as_ref(), last_boosty, now_unix()).await {
                Ok(s) => {
                    let cfg = data.cfg.fund.as_ref().unwrap();
                    if let Some(path) = &cfg.json_path {
                        if let Err(e) = write_json(path, &s) {
                            eprintln!("[Fund] writing {}: {}", path, e);
                        }
                    }
                    let k = key(&s);
                    if k != shown {
                        let (embed, rows) = render(cfg, &s);
                        publish(&ctx, &data, channel, embed, rows).await;
                        shown = k;
                    }
                }
                Err(e) => eprintln!("[Fund] build: {}", e),
            }
            // every 5 minutes, so the month rolls over and the file updates; commands wake it immediately
            tokio::select! {
                _ = REFRESH.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(300)) => {}
            }
        }
    });
}

// ---------- commands ----------

fn is_ru(ctx: &Context<'_>) -> bool {
    ctx.locale().is_some_and(|l| l.starts_with("ru"))
}

async fn reply(ctx: Context<'_>, en: String, ru: String) -> Result<(), Error> {
    let text = if is_ru(&ctx) { ru } else { en };
    ctx.send(poise::CreateReply::default().content(text).ephemeral(true)).await?;
    Ok(())
}

#[derive(poise::ChoiceParameter, PartialEq, Clone, Copy)]
pub enum Source {
    #[name = "Boosty"]
    Boosty,
    #[name = "Patreon (manual)"]
    Patreon,
    #[name = "Other / Другое"]
    Other,
}

impl Source {
    fn key(self) -> &'static str {
        match self {
            Source::Boosty => "boosty",
            Source::Patreon => "patreon",
            Source::Other => "other",
        }
    }
}

async fn open_goals(ctx: Context<'_>, partial: &str) -> serenity::CreateAutocompleteResponse {
    let rows: Vec<(String, String)> = match ctx
        .data()
        .db
        .db
        .query("SELECT VALUE [record::id(id), title_en] FROM fund_goal WHERE open = true")
        .await
    {
        Ok(mut r) => r.take::<Vec<(String, String)>>(0).unwrap_or_default(),
        Err(_) => vec![],
    };
    let p = partial.to_lowercase();
    let choices = rows
        .into_iter()
        .filter(|(id, t)| id.contains(&p) || t.to_lowercase().contains(&p))
        .take(25)
        .map(|(id, t)| serenity::AutocompleteChoice::new(format!("{} ({})", t, id), id))
        .collect();
    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

/// Server funding: goals and donations / Сбор на серверы
#[poise::command(slash_command, guild_only, subcommands("add", "goal", "close", "thank", "unthank"), subcommand_required, default_member_permissions = "MANAGE_GUILD")]
pub async fn fund(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Add a donation (Boosty etc.) to this month or a goal / Добавить сумму в месяц или цель
#[poise::command(slash_command, guild_only)]
async fn add(
    ctx: Context<'_>,
    #[description = "Amount in $ / Сумма в $"]
    #[min = 0.01]
    amount: f64,
    #[description = "Where it came from / Откуда"] source: Source,
    #[description = "One-time goal; empty = this month / Разовая цель; пусто — в месяц"]
    #[autocomplete = "open_goals"]
    goal: Option<String>,
    #[description = "Note for the team / Заметка для команды"] note: Option<String>,
) -> Result<(), Error> {
    let cents = (amount * 100.0).round() as i64;
    let (month, ..) = month_of(now_unix());
    if let Some(g) = &goal {
        let exists: Option<bool> = ctx
            .data()
            .db
            .db
            .query("RETURN record::exists(type::thing('fund_goal', $g))")
            .bind(("g", g.clone()))
            .await?
            .take(0)?;
        if exists != Some(true) {
            return Err(format!("no goal '{}' / нет цели '{}'", g, g).into());
        }
    }
    ctx.data()
        .db
        .db
        .query("CREATE fund_entry SET amount = $c, source = $s, note = $n, month = $m, goal = $g, by = $by")
        .bind(("c", cents))
        .bind(("s", source.key()))
        .bind(("n", note.unwrap_or_default()))
        .bind(("m", month.clone()))
        .bind(("g", goal.clone()))
        .bind(("by", ctx.author().id.to_string()))
        .await?
        .check()?;
    REFRESH.notify_one();
    let to = goal.clone().unwrap_or(month);
    reply(ctx, format!("✅ +${:.2} ({}) → {}", amount, source.key(), to), format!("✅ +${:.2} ({}) → {}", amount, source.key(), to)).await
}

/// New one-time goal / Новая разовая цель
#[poise::command(slash_command, guild_only)]
async fn goal(
    ctx: Context<'_>,
    #[description = "Title in English"] title_en: String,
    #[description = "Название по-русски"] title_ru: String,
    #[description = "Target in $ / Цель в $"]
    #[min = 1]
    target: f64,
    #[description = "Days until the end; empty = no end / Дней до конца; пусто — без срока"]
    #[min = 1]
    days: Option<u32>,
) -> Result<(), Error> {
    let id = slug(&title_en);
    if id.is_empty() {
        return Err("title_en needs latin letters or digits / в title_en нужны латинские буквы или цифры".into());
    }
    let mut r = ctx
        .data()
        .db
        .db
        .query("IF record::exists(type::thing('fund_goal', $id)) THEN false ELSE (CREATE type::thing('fund_goal', $id) SET title_en = $en, title_ru = $ru, target = $t, ends = IF $d THEN time::now() + type::duration(string::concat(<string>$d, 'd')) ELSE NONE END RETURN NONE) == [] END")
        .bind(("id", id.clone()))
        .bind(("en", title_en.clone()))
        .bind(("ru", title_ru))
        .bind(("t", (target * 100.0).round() as i64))
        .bind(("d", days))
        .await?;
    let created: Option<bool> = r.take(0)?;
    if created != Some(true) {
        return Err(format!("goal '{}' already exists / цель '{}' уже есть", id, id).into());
    }
    REFRESH.notify_one();
    reply(ctx, format!("🎯 Goal `{}` created: ${:.0}", id, target), format!("🎯 Цель `{}` создана: ${:.0}", id, target)).await
}

/// Close a one-time goal / Закрыть разовую цель
#[poise::command(slash_command, guild_only)]
async fn close(
    ctx: Context<'_>,
    #[description = "Goal / Цель"]
    #[autocomplete = "open_goals"]
    goal: String,
) -> Result<(), Error> {
    let changed: Vec<bool> = ctx
        .data()
        .db
        .db
        .query("UPDATE type::thing('fund_goal', $g) SET open = false RETURN VALUE open")
        .bind(("g", goal.clone()))
        .await?
        .take(0)?;
    if changed.is_empty() {
        return Err(format!("no goal '{}' / нет цели '{}'", goal, goal).into());
    }
    REFRESH.notify_one();
    reply(ctx, format!("✅ Goal `{}` closed", goal), format!("✅ Цель `{}` закрыта", goal)).await
}

/// Add a supporter without Discord, with consent / Добавить поддержавшего без Discord, с согласия
#[poise::command(slash_command, guild_only)]
async fn thank(ctx: Context<'_>, #[description = "Name to show / Имя для списка"] name: String) -> Result<(), Error> {
    let name: String = name.trim().chars().take(40).collect();
    let id = format!("manual-{}", slug(&name));
    ctx.data()
        .db
        .db
        .query("UPSERT type::thing('supporter', $id) SET name = $n, visible = true")
        .bind(("id", id))
        .bind(("n", name.clone()))
        .await?
        .check()?;
    REFRESH.notify_one();
    reply(ctx, format!("💜 {} added to the list", name), format!("💜 {} в списке", name)).await
}

/// Remove a name from the list / Убрать имя из списка
#[poise::command(slash_command, guild_only)]
async fn unthank(ctx: Context<'_>, #[description = "Name / Имя"] name: String) -> Result<(), Error> {
    ctx.data()
        .db
        .db
        .query("UPDATE supporter SET visible = false WHERE name = $n")
        .bind(("n", name.trim().to_string()))
        .await?
        .check()?;
    REFRESH.notify_one();
    reply(ctx, format!("Removed {}", name), format!("{} убран из списка", name)).await
}

/// Supporters list: show or hide yourself / Список поддержавших: показать или скрыть себя
#[poise::command(slash_command, guild_only, subcommands("show", "hide"), subcommand_required)]
pub async fn supporter(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Show my name in the thank-you list (Discord, site) / Показать моё имя в списке благодарностей
#[poise::command(slash_command, guild_only)]
async fn show(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let cfg = data.cfg.fund.as_ref().ok_or("fund off")?;
    let role = cfg.supporter_role.as_deref().and_then(|r| data.names.role(r));
    let member = ctx.author_member().await.ok_or("no member")?;
    if let Some(role) = role {
        if !member.roles.contains(&role) {
            return Err((if is_ru(&ctx) {
                "список — для тех, кто поддерживает проект (роль выдаётся через Patreon/Boosty или командой)"
            } else {
                "the list is for project supporters (the role comes from Patreon/Boosty or the team)"
            })
            .into());
        }
    }
    let name: String = member.display_name().chars().take(40).collect();
    data.db
        .db
        .query("UPSERT type::thing('supporter', $id) SET name = $n, visible = true")
        .bind(("id", member.user.id.to_string()))
        .bind(("n", name.clone()))
        .await?
        .check()?;
    REFRESH.notify_one();
    reply(
        ctx,
        format!("💜 Thank you! **{}** is now in the list in Discord and on the site. `/supporter hide` removes it.", name),
        format!("💜 Спасибо! **{}** теперь в списке в Discord и на сайте. `/supporter hide` — убрать.", name),
    )
    .await
}

/// Hide my name from the list / Убрать моё имя из списка
#[poise::command(slash_command, guild_only)]
async fn hide(ctx: Context<'_>) -> Result<(), Error> {
    ctx.data()
        .db
        .db
        .query("UPDATE type::thing('supporter', $id) SET visible = false")
        .bind(("id", ctx.author().id.to_string()))
        .await?
        .check()?;
    REFRESH.notify_one();
    reply(ctx, "Done — your name is hidden.".into(), "Готово — имя скрыто.".into()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::testing;

    #[test]
    fn calendar() {
        // 2026-11-16 12:00 UTC
        let (m, ends, en, ru) = month_of(1_794_830_400);
        assert_eq!((m.as_str(), en.as_str(), ru.as_str()), ("2026-11", "November", "ноябрь"));
        assert_eq!(ends, 1_796_083_200, "2026-12-01 00:00 UTC");
        let (m, ends, ..) = month_of(1_798_675_200); // 2026-12-31
        assert_eq!(m, "2026-12");
        assert_eq!(ends, 1_798_761_600, "2027-01-01");
        assert_eq!(month_of(0).0, "1970-01");
        assert_eq!(slug("Servers: New PC!"), "servers-new-pc");
        assert_eq!(slug("Новый"), "");
    }

    #[test]
    fn boosty_parse_and_months() {
        let body = r#"{"data":[{"type":"money","currentSum":0,"targetSum":257,"currencyTargetSums":{"USD":257,"RUB":20000},"description":"На аренду сервера DungeonBorne","id":841879,"currencyCurrentSums":{"RUB":3900,"EUR":45.6,"USD":50.12}}]}"#;
        assert_eq!(parse_boosty(body, None), Some(5012));
        assert_eq!(parse_boosty(body, Some(841879)), Some(5012));
        assert_eq!(parse_boosty(body, Some(1)), None);
        assert_eq!(parse_boosty("{}", None), None);
        assert_eq!(prev_month("2026-11"), "2026-10");
        assert_eq!(prev_month("2027-01"), "2026-12");
    }

    #[test]
    fn reserve_math() {
        let cap = 22_000;
        // November $280 (+60 to the reserve), December $200 (−20 from the reserve), January $500 (reserve up to 220, the rest to the team)
        assert_eq!(reserve_after(&[(28_000, 22_000)], cap), 6_000);
        assert_eq!(reserve_after(&[(28_000, 22_000), (20_000, 22_000)], cap), 4_000);
        assert_eq!(reserve_after(&[(28_000, 22_000), (20_000, 22_000), (50_000, 22_000)], cap), 22_000);
        assert_eq!(reserve_after(&[(10_000, 22_000)], cap), 0, "the reserve doesn't go negative");
        assert_eq!(split_surplus(6_000, 0, cap), (6_000, 0));
        assert_eq!(split_surplus(30_000, 4_000, cap), (18_000, 12_000));
        assert_eq!(split_surplus(5_000, 22_000, cap), (0, 5_000));
        assert_eq!(split_surplus(0, 0, cap), (0, 0));
    }

    #[tokio::test]
    async fn boosty_delta_and_reserve() {
        let extra = "[fund]\nchannel = \"1\"\ngoal = 220\nboosty_blog = \"example-blog\"\n";
        let (data, dir) = testing::data("fund_reserve", extra).await;
        let nov = 1_794_830_400; // 2026-11-16
        let dec = 1_796_083_200 + 86_400; // 2026-12-02
        // November: Boosty had already raised $100 before the first read — that's the base, it doesn't count for the month
        let s = build(&data, None, Some(10_000), nov).await.unwrap();
        assert_eq!(s.goals[0].sources.get("boosty"), Some(&0.0));
        assert_eq!(s.goals[0].sources.get("patreon"), Some(&0.0), "Patreon is shown even at zero");
        assert_eq!(s.reserve, Reserve { amount: 0.0, cap: 220.0 });
        // in November another $200 on Boosty + Patreon $80 = $280 of $220 → $60 to the reserve
        let p = patreon::Funding { cents: 8000, patrons: 2, currency: "USD".into(), url: None };
        let s = build(&data, Some(&p), Some(30_000), nov + 3600).await.unwrap();
        let m = &s.goals[0];
        assert_eq!((m.raised, m.covered, m.surplus, m.to_reserve, m.to_team), (280.0, true, 60.0, 60.0, 0.0));
        assert_eq!(s.reserve.amount, 60.0);
        // December: the Boosty base is the last November value; the $60 reserve carried over and isn't part of raised
        let s = build(&data, None, Some(31_000), dec).await.unwrap();
        let d = &s.goals[0];
        assert_eq!(d.id, "servers-2026-12");
        assert_eq!(d.sources.get("boosty"), Some(&10.0));
        assert_eq!((d.raised, d.covered), (10.0, false));
        assert_eq!(s.reserve.amount, 60.0);
        // December with a big surplus: $160 fills the reserve up to $220, the rest goes to the team
        let big = patreon::Funding { cents: 50_000, patrons: 9, currency: "USD".into(), url: None };
        let s = build(&data, Some(&big), Some(31_000), dec + 60).await.unwrap();
        let d = &s.goals[0];
        assert_eq!((d.raised, d.surplus, d.to_reserve, d.to_team), (510.0, 290.0, 160.0, 130.0));
        assert_eq!(s.reserve.amount, 220.0);
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["goals"][0]["to_team"], 130.0);
        assert_eq!(v["reserve"]["cap"], 220.0);
        assert!(v["goals"][0].get("carried_in").is_none());
        let _ = render(data.cfg.fund.as_ref().unwrap(), &s);
        // the Boosty goal was reset — no negative
        let s = build(&data, None, Some(0), dec + 120).await.unwrap();
        assert_eq!(s.goals[0].sources.get("boosty"), Some(&0.0));
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn builds_support_json() {
        let extra = "[fund]\nchannel = \"1\"\ngoal = 220\nboosty_link = \"https://boosty.to/x\"\nsupporter_role = \"Sup\"\n\
                     breakdown = [{ en = \"Game servers\", ru = \"Игровые серверы\", usd = 200 }, { en = \"Domain\", ru = \"Домен\", usd = 20 }]\n";
        let (data, dir) = testing::data("fund", extra).await;
        let now = 1_794_830_400;
        let db = &data.db.db;
        db.query("CREATE fund_entry SET amount = 3000, source = 'boosty', month = '2026-11', goal = NONE").await.unwrap().check().unwrap();
        db.query("CREATE fund_entry SET amount = 999, source = 'boosty', month = '2026-10', goal = NONE").await.unwrap().check().unwrap();
        db.query("CREATE fund_goal:new_pc SET title_en = 'New PC', title_ru = 'Новый ПК', target = 50000").await.unwrap().check().unwrap();
        db.query("CREATE fund_goal:old SET title_en = 'Old', title_ru = 'Старая', target = 100, open = false").await.unwrap().check().unwrap();
        db.query("CREATE fund_entry SET amount = 12550, source = 'other', month = '2026-11', goal = 'new_pc'").await.unwrap().check().unwrap();
        db.query("CREATE supporter:1 SET name = 'Anna', since = d'2026-11-01T00:00:00Z'").await.unwrap().check().unwrap();
        db.query("CREATE supporter:2 SET name = 'Hidden', visible = false").await.unwrap().check().unwrap();

        let p = patreon::Funding { cents: 9000, patrons: 3, currency: "USD".into(), url: Some("https://patreon.com/x".into()) };
        let s = build(&data, Some(&p), None, now).await.unwrap();
        assert_eq!(s.goals.len(), 2, "month + one open one-time goal");
        let m = &s.goals[0];
        assert_eq!((m.id.as_str(), m.kind, m.raised, m.target), ("servers-2026-11", "monthly", 120.0, 220.0));
        assert_eq!(m.sources.get("patreon"), Some(&90.0));
        assert_eq!(m.sources.get("boosty"), Some(&30.0), "October's 9.99 doesn't count");
        assert_eq!(m.breakdown.len(), 2);
        assert_eq!(m.title.ru, "Серверы — ноябрь");
        let g = &s.goals[1];
        assert_eq!((g.id.as_str(), g.kind, g.raised, g.target), ("new_pc", "one_time", 125.5, 500.0));
        assert_eq!(s.supporters, vec![Supporter { name: "Anna".into(), since: 1_793_491_200 }]);
        assert_eq!(s.patrons, 3);
        assert_eq!(s.links.patreon.as_deref(), Some("https://patreon.com/x"));
        assert_eq!(s.links.boosty.as_deref(), Some("https://boosty.to/x"));
        let soon = toml::from_str::<crate::config::Config>(&format!("[discord]
guild_id = 1
{}coming_soon = true
", extra)).unwrap();
        assert!(soon.fund.unwrap().coming_soon);

        // the file is written whole and reads as JSON
        let path = format!("{}/support.json", dir);
        write_json(&path, &s).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["goals"][0]["raised"], 120.0);
        assert!(v["goals"][1].get("breakdown").is_none());
        assert!(!std::path::Path::new(&format!("{}.tmp", path)).exists());

        // without Patreon — manual amounts only
        let s2 = build(&data, None, None, now).await.unwrap();
        assert_eq!(s2.goals[0].raised, 30.0);
        assert_eq!(key(&s), key(&Support { updated: 1, ..s.clone() }), "the update time is no reason to edit the message");
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
