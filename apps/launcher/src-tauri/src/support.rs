//! Project support (16.11, decision of the owner): money for the servers. The manifest's optional `support.url` names a
//! JSON (`/dbserver/support`, written by the Discord bot; one source for Discord, the site and the launcher) with this
//! month's server goal and the donation links. The launcher shows only the monthly goal (`goals[0]`, kind "monthly") and
//! only the links it knows (`links.patreon`, `links.boosty`, https). No field, no answer or no monthly goal — no strip.

use std::collections::BTreeMap;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::manifest::Manifest;
use crate::net::{self, check_url};

const SUPPORT_LIMIT: usize = 256 * 1024;
/// Links the launcher offers, in this order
const LINKS: [&str; 2] = ["patreon", "boosty"];

#[derive(Debug, Clone, Serialize)]
pub struct Support {
    pub currency: String,
    pub goal: Goal,
    /// `[name, url]` in the order of `LINKS`; empty — no button
    pub links: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: BTreeMap<String, String>,
    #[serde(default)]
    pub raised: f64,
    pub target: f64,
    #[serde(default)]
    pub ends_at: i64,
    /// The month is paid (`raised >= target`); the bot sends it since 16.11, older data — computed here
    #[serde(default)]
    pub covered: bool,
    /// Over the target (the bot splits it into to_reserve / to_team; the launcher doesn't show the parts)
    #[serde(default)]
    pub surplus: f64,
    /// Raised per platform (`patreon`, `boosty`; the bot sends both, $0 included); the UI shows the ones present
    #[serde(default)]
    pub sources: BTreeMap<String, f64>,
}

#[derive(Deserialize)]
struct Wire {
    #[serde(default)]
    currency: String,
    #[serde(default)]
    goals: Vec<Goal>,
    #[serde(default)]
    links: BTreeMap<String, String>,
}

/// `None` — nothing to show: no monthly goal first, or a goal without a target
pub fn parse(body: &[u8]) -> Result<Option<Support>> {
    let w: Wire =
        serde_json::from_slice(body).map_err(|e| Error::new("support_invalid", e.to_string()))?;
    let Some(mut goal) = w.goals.into_iter().next() else {
        return Ok(None);
    };
    if goal.kind != "monthly"
        || !goal.target.is_finite()
        || goal.target <= 0.0
        || !goal.raised.is_finite()
        || goal.id.is_empty()
    {
        return Ok(None);
    }
    goal.covered |= goal.raised >= goal.target;
    let links = LINKS
        .iter()
        .filter_map(|k| {
            let u = check_url(w.links.get(*k)?).ok()?;
            (u.scheme() == "https").then(|| (k.to_string(), u.to_string()))
        })
        .collect();
    let currency = if w.currency.is_empty() {
        "USD".into()
    } else {
        w.currency
    };
    Ok(Some(Support {
        currency,
        goal,
        links,
    }))
}

/// `None` — the server has no support source, or it answers 404
pub async fn fetch(client: &Client, m: &Manifest) -> Result<Option<Support>> {
    let Some(r) = &m.support else { return Ok(None) };
    let url = check_url(&r.url)?;
    match net::get_bytes(client, &url, SUPPORT_LIMIT).await {
        Ok(body) => parse(&body),
        Err(e) if e.code == "http_status" && e.detail.ends_with("404 Not Found") => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shape the Discord bot writes (bot/data/support.json)
    const BOT: &str = r#"{"currency":"USD","updated":1794766321,
      "goals":[{"id":"servers-2026-11","kind":"monthly","title":{"en":"Servers — November","ru":"Серверы — ноябрь"},
        "raised":120.0,"target":220.0,"ends_at":1796083200,"sources":{"patreon":120.0}},
        {"id":"mic","kind":"one_time","title":{"ru":"Микрофон"},"raised":5,"target":50}],
      "supporters":[],"patrons":3,
      "links":{"patreon":"https://www.patreon.com/dungeonbornecommunity","boosty":"https://boosty.to/hipach","evil":"https://e.x","plain":"http://p.x"}}"#;

    #[test]
    fn monthly_goal_and_known_links() {
        let s = parse(BOT.as_bytes()).unwrap().unwrap();
        assert_eq!(s.currency, "USD");
        assert_eq!(s.goal.id, "servers-2026-11");
        assert_eq!((s.goal.raised, s.goal.target), (120.0, 220.0));
        assert_eq!(s.goal.title["ru"], "Серверы — ноябрь");
        assert!(!s.goal.covered);
        assert_eq!(s.goal.sources.get("patreon"), Some(&120.0));
        assert!(parse(br#"{"goals":[{"id":"x","kind":"monthly","target":1}]}"#).unwrap().unwrap().goal.sources.is_empty());
        let paid = BOT.replace(r#""raised":120.0"#, r#""raised":250.0,"surplus":30.0"#);
        let p = parse(paid.as_bytes()).unwrap().unwrap();
        assert!(p.goal.covered, "covered is computed when the bot doesn't say");
        assert_eq!(p.goal.surplus, 30.0);
        let names: Vec<&str> = s.links.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, ["patreon", "boosty"], "only known links, in order");
        assert_eq!(s.links[1].1, "https://boosty.to/hipach");
    }

    #[test]
    fn nothing_to_show() {
        assert!(parse(br#"{"goals":[]}"#).unwrap().is_none());
        assert!(
            parse(br#"{"goals":[{"id":"x","kind":"one_time","target":5}]}"#)
                .unwrap()
                .is_none(),
            "one-time goals are not shown"
        );
        assert!(
            parse(br#"{"goals":[{"id":"x","kind":"monthly","target":0}]}"#)
                .unwrap()
                .is_none()
        );
        assert!(parse(b"not json").is_err());
        let http_only = parse(br#"{"goals":[{"id":"x","kind":"monthly","target":1}],"links":{"patreon":"http://p.x"}}"#).unwrap().unwrap();
        assert!(http_only.links.is_empty(), "an http link gives no button");
    }
}
