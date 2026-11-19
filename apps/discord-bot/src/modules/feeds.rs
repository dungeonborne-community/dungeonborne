//! Notifications: new YouTube videos and GitHub releases (Atom feeds, no keys needed), any RSS/Atom by url,
//! and Twitch stream starts (Helix API, needs a client id/secret).
//! What was already seen is remembered by the database: `feed_item:[feed, entry id]` (composite ID); `UPSERT … RETURN BEFORE`
//! atomically answers "is it new". On a feed's first run everything current is marked as seen without posts, so the channel
//! isn't flooded with old videos. Stream: `stream:<login>.live_id` — post only when a new broadcast starts (a bot restart doesn't duplicate it).

use crate::config::{FeedCfg, TwitchCfg};
use crate::Data;
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use std::time::Duration;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE feed_item SCHEMAFULL;
DEFINE FIELD OVERWRITE at ON feed_item TYPE datetime DEFAULT time::now() READONLY;
DEFINE EVENT OVERWRITE feed_item_retention ON feed_item WHEN $event = "CREATE" THEN {
    DELETE feed_item WHERE at < time::now() - 180d;
};
DEFINE TABLE OVERWRITE feed SCHEMAFULL;
DEFINE FIELD OVERWRITE seeded ON feed TYPE bool DEFAULT true;
DEFINE TABLE OVERWRITE stream SCHEMAFULL;
DEFINE FIELD OVERWRITE live_id ON stream TYPE option<string>;
"#;

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub link: String,
    pub author: String,
}

fn unescape(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix("<![CDATA[").and_then(|x| x.strip_suffix("]]>")).unwrap_or(s);
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'").replace("&amp;", "&")
}

fn tag<'a>(block: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{}", name);
    let mut from = 0;
    while let Some(pos) = block[from..].find(&open) {
        let start = from + pos;
        let after = &block[start + open.len()..];
        // <title> or <title type="…">, but not <titleX>
        if after.starts_with('>') || after.starts_with(' ') {
            let body_start = start + open.len() + after.find('>')? + 1;
            let close = format!("</{}>", name);
            let end = block[body_start..].find(&close)?;
            return Some(&block[body_start..body_start + end]);
        }
        from = start + open.len();
    }
    None
}

fn atom_link(block: &str) -> Option<String> {
    let mut rest = block;
    let mut first = None;
    while let Some(pos) = rest.find("<link") {
        let el = &rest[pos..rest[pos..].find('>').map(|e| pos + e)?];
        let href = el.split("href=\"").nth(1).and_then(|h| h.split('"').next()).map(unescape);
        if el.contains("rel=\"alternate\"") {
            return href;
        }
        if first.is_none() {
            first = href;
        }
        rest = &rest[pos + 5..];
    }
    first
}

/// Minimal parsing of Atom (<entry>) and RSS 2.0 (<item>): id, title, link, author
pub fn parse(xml: &str) -> Vec<Item> {
    let (open, close) = if xml.contains("<entry") { ("<entry", "</entry>") } else { ("<item", "</item>") };
    xml.split(open)
        .skip(1)
        .filter_map(|chunk| {
            let block = &chunk[..chunk.find(close)?];
            let link = tag(block, "link").filter(|l| !l.trim().is_empty()).map(unescape).or_else(|| atom_link(block))?;
            let id = tag(block, "id").or_else(|| tag(block, "guid")).map(unescape).unwrap_or_else(|| link.clone());
            let title = tag(block, "title").map(unescape).unwrap_or_default();
            let author = tag(block, "author").map(|a| tag(a, "name").map(unescape).unwrap_or_else(|| unescape(a))).unwrap_or_default();
            Some(Item { id, title, link, author })
        })
        .collect()
}

#[derive(Deserialize)]
struct Empty {}

/// true — the entry is new (just marked)
pub async fn mark_seen(data: &Data, feed: &str, item: &str) -> bool {
    let r = data
        .db
        .db
        .query("UPSERT type::thing('feed_item', [$f, $i]) RETURN BEFORE")
        .bind(("f", feed.to_string()))
        .bind(("i", item.to_string()))
        .await;
    match r {
        Ok(mut r) => r.take::<Vec<Option<Empty>>>(0).map(|v| v.into_iter().flatten().next().is_none()).unwrap_or(false),
        Err(e) => {
            eprintln!("[Feeds] {}: {}", feed, e);
            false
        }
    }
}

/// true — the feed is polled for the first time (current entries must be marked silently)
pub async fn first_run(data: &Data, feed: &str) -> bool {
    let r = data.db.db.query("UPSERT type::thing('feed', $f) SET seeded = true RETURN BEFORE").bind(("f", feed.to_string())).await;
    r.ok().and_then(|mut r| r.take::<Vec<Option<Empty>>>(0).ok()).is_some_and(|v| v.into_iter().flatten().next().is_none())
}

/// How many new entries; the very first ones are not posted
pub async fn new_items(data: &Data, feed: &str, items: &[Item]) -> Vec<Item> {
    let seeding = first_run(data, feed).await;
    let mut fresh = Vec::new();
    for item in items.iter().rev() {
        if mark_seen(data, feed, &item.id).await && !seeding {
            fresh.push(item.clone());
        }
    }
    fresh
}

pub fn ping(data: &Data, role: &Option<String>) -> (String, serenity::CreateAllowedMentions) {
    match role.as_deref().and_then(|r| data.names.role(r)) {
        Some(r) => (format!("<@&{}> ", r), serenity::CreateAllowedMentions::new().roles(vec![r])),
        None => (String::new(), serenity::CreateAllowedMentions::new()),
    }
}

async fn poll_feed(ctx: &serenity::Context, data: &Data, cfg: &FeedCfg, url: &str, channel: serenity::ChannelId) {
    let Some(items) = fetch(data, url).await else { return };
    // newest first in the feed; post at most 5 at a time, from oldest to newest
    for item in new_items(data, &cfg.name, &items).await.into_iter().take(5) {
        let (prefix, mentions) = ping(data, &cfg.ping);
        let text = cfg.text.replace("{title}", &item.title).replace("{link}", &item.link).replace("{author}", &item.author);
        let msg = serenity::CreateMessage::new().content(format!("{}{}", prefix, text)).allowed_mentions(mentions);
        match channel.send_message(&ctx.http, msg).await {
            Ok(_) => println!("[Feeds] {}: {}", cfg.name, item.title),
            Err(e) => eprintln!("[Feeds] {} → channel: {}", cfg.name, e),
        }
    }
}

pub fn spawn(ctx: serenity::Context, data: Data) {
    for (i, cfg) in data.cfg.feed.iter().enumerate() {
        let Some(url) = cfg.feed_url() else {
            eprintln!("[Feeds] '{}': needs youtube, github or url", cfg.name);
            continue;
        };
        let Some(channel) = data.names.need_channel(&cfg.channel, "Feeds") else { continue };
        let (ctx, data) = (ctx.clone(), data.clone());
        let every = Duration::from_secs(cfg.every_mins.max(2) * 60);
        println!("[Feeds] '{}' every {} min → #{}", cfg.name, every.as_secs() / 60, channel);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3 * i as u64)).await; // feeds are not polled all at once
            loop {
                let cfg = &data.cfg.feed[i];
                poll_feed(&ctx, &data, cfg, &url, channel).await;
                tokio::time::sleep(every).await;
            }
        });
    }
    if let Some(cfg) = &data.cfg.twitch {
        if let Some(channel) = data.names.need_channel(&cfg.channel, "Twitch") {
            println!("[Twitch] {} every {} s → #{}", cfg.logins.join(", "), cfg.every_secs.max(60), channel);
            tokio::spawn(twitch_loop(ctx, data.clone(), channel));
        }
    }
}

#[derive(Deserialize)]
struct TwitchToken {
    access_token: String,
}

#[derive(Deserialize)]
struct Streams {
    data: Vec<StreamInfo>,
}

#[derive(Deserialize, Clone)]
pub struct StreamInfo {
    pub id: String,
    pub user_login: String,
    pub user_name: String,
    pub title: String,
    #[serde(default)]
    pub game_name: String,
}

/// Helix client: obtains the app token itself and refreshes it when Twitch answers 401
pub struct Twitch {
    id: String,
    secret: String,
    token: Option<String>,
}

impl Twitch {
    /// Keys from .env; none — None (the module logs it and doesn't poll Twitch)
    pub fn from_env(id_env: &str, secret_env: &str) -> Option<Self> {
        let id = std::env::var(id_env).ok().filter(|v| !v.trim().is_empty())?;
        let secret = std::env::var(secret_env).ok().filter(|v| !v.trim().is_empty())?;
        Some(Self { id: id.trim().into(), secret: secret.trim().into(), token: None })
    }

    async fn token(&mut self, data: &Data) -> Option<String> {
        if self.token.is_none() {
            let resp = data
                .http
                .post("https://id.twitch.tv/oauth2/token")
                .query(&[("client_id", self.id.as_str()), ("client_secret", self.secret.as_str()), ("grant_type", "client_credentials")])
                .send()
                .await
                .ok()?;
            if !resp.status().is_success() {
                eprintln!("[Twitch] token: {}", resp.status());
                return None;
            }
            self.token = resp.json::<TwitchToken>().await.ok().map(|t| t.access_token);
        }
        self.token.clone()
    }

    /// Which of the logins are live now (up to 100 logins per request). None — error, the poll is skipped.
    pub async fn live(&mut self, data: &Data, logins: &[String]) -> Option<Vec<StreamInfo>> {
        if logins.is_empty() {
            return Some(Vec::new());
        }
        let token = self.token(data).await?;
        let query: Vec<(&str, &str)> = logins.iter().take(100).map(|l| ("user_login", l.as_str())).collect();
        match data.http.get("https://api.twitch.tv/helix/streams").query(&query).header("Client-Id", &self.id).bearer_auth(token).send().await {
            Ok(r) if r.status() == reqwest::StatusCode::UNAUTHORIZED => {
                self.token = None; // expired — a new one on the next poll
                None
            }
            Ok(r) if r.status().is_success() => r.json::<Streams>().await.ok().map(|s| s.data),
            Ok(r) => {
                eprintln!("[Twitch] streams: {}", r.status());
                None
            }
            Err(e) => {
                eprintln!("[Twitch] streams: {}", e);
                None
            }
        }
    }
}

/// Fetch and parse an Atom/RSS feed
pub async fn fetch(data: &Data, url: &str) -> Option<Vec<Item>> {
    match data.http.get(url).send().await {
        Ok(r) if r.status().is_success() => Some(parse(&r.text().await.ok()?)),
        Ok(r) => {
            eprintln!("[Feeds] {} → {}", url, r.status());
            None
        }
        Err(e) => {
            eprintln!("[Feeds] {}: {}", url, e);
            None
        }
    }
}

/// New broadcast: live_id changed to a non-empty one. The end of a broadcast resets live_id.
pub async fn stream_started(data: &Data, login: &str, live_id: Option<&str>) -> bool {
    #[derive(Deserialize)]
    struct Before {
        live_id: Option<String>,
    }
    let r = data
        .db
        .db
        .query("UPSERT type::thing('stream', $l) SET live_id = $id RETURN BEFORE")
        .bind(("l", login.to_lowercase()))
        .bind(("id", live_id.map(str::to_string)))
        .await;
    let before = r.ok().and_then(|mut r| r.take::<Vec<Option<Before>>>(0).ok()).and_then(|v| v.into_iter().flatten().next());
    live_id.is_some() && before.and_then(|b| b.live_id).as_deref() != live_id
}

async fn twitch_loop(ctx: serenity::Context, data: Data, channel: serenity::ChannelId) {
    let cfg: &TwitchCfg = data.cfg.twitch.as_ref().unwrap();
    let Some(mut twitch) = Twitch::from_env(&cfg.client_id_env, &cfg.client_secret_env) else {
        eprintln!("[Twitch] no {} / {} in .env", cfg.client_id_env, cfg.client_secret_env);
        return;
    };
    loop {
        if let Some(live) = twitch.live(&data, &cfg.logins).await {
            for login in &cfg.logins {
                let s = live.iter().find(|s| s.user_login.eq_ignore_ascii_case(login));
                if stream_started(&data, login, s.map(|s| s.id.as_str())).await {
                    let s = s.unwrap();
                    let (prefix, mentions) = ping(&data, &cfg.ping);
                    let text = cfg
                        .text
                        .replace("{name}", &s.user_name)
                        .replace("{login}", &s.user_login)
                        .replace("{title}", &s.title)
                        .replace("{game}", &s.game_name);
                    let msg = serenity::CreateMessage::new().content(format!("{}{}", prefix, text)).allowed_mentions(mentions);
                    if let Err(e) = channel.send_message(&ctx.http, msg).await {
                        eprintln!("[Twitch] {}: {}", login, e);
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(cfg.every_secs.max(60))).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::testing;

    const YOUTUBE: &str = r#"<?xml version="1.0"?><feed xmlns:yt="http://www.youtube.com/xml/schemas/2015">
 <title>Канал</title>
 <entry><id>yt:video:AAA</id><yt:videoId>AAA</yt:videoId><title>Патч 1.2 &amp; новости</title>
  <link rel="alternate" href="https://www.youtube.com/watch?v=AAA"/><author><name>Dungeonborne RU</name></author></entry>
 <entry><id>yt:video:BBB</id><title>Старое видео</title><link rel="alternate" href="https://www.youtube.com/watch?v=BBB"/></entry>
</feed>"#;

    const GITHUB: &str = r#"<feed><entry><id>tag:github.com,2008:Repository/1/v0.3.0</id><updated>2026-11-01</updated>
 <link rel="alternate" type="text/html" href="https://github.com/o/r/releases/tag/v0.3.0"/><title>v0.3.0</title>
 <author><name>dev</name></author></entry></feed>"#;

    const RSS: &str = r#"<rss><channel><title>t</title><item><title><![CDATA[Пост <1>]]></title><link>https://ex.org/1</link><guid>g1</guid></item></channel></rss>"#;

    #[test]
    fn parse_feeds() {
        let yt = parse(YOUTUBE);
        assert_eq!(yt.len(), 2);
        assert_eq!(yt[0], Item {
            id: "yt:video:AAA".into(),
            title: "Патч 1.2 & новости".into(),
            link: "https://www.youtube.com/watch?v=AAA".into(),
            author: "Dungeonborne RU".into()
        });
        let gh = parse(GITHUB);
        assert_eq!((gh[0].title.as_str(), gh[0].link.as_str()), ("v0.3.0", "https://github.com/o/r/releases/tag/v0.3.0"));
        let rss = parse(RSS);
        assert_eq!((rss[0].id.as_str(), rss[0].title.as_str(), rss[0].link.as_str()), ("g1", "Пост <1>", "https://ex.org/1"));
    }

    #[tokio::test]
    async fn seen_and_streams() {
        let (data, dir) = testing::data("feeds", "").await;
        let items = parse(YOUTUBE);
        assert!(new_items(&data, "yt", &items).await.is_empty(), "first poll — mark silently");
        assert!(new_items(&data, "yt", &items).await.is_empty());
        let mut more = vec![Item { id: "yt:video:CCC".into(), title: "Новое".into(), link: "l".into(), author: String::new() }];
        more.extend(items);
        let fresh = new_items(&data, "yt", &more).await;
        assert_eq!(fresh.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["yt:video:CCC"]);
        assert_eq!(new_items(&data, "gh", &parse(GITHUB)).await.len(), 0, "another feed has its own first run");

        assert!(!stream_started(&data, "Dev", None).await);
        assert!(stream_started(&data, "Dev", Some("s1")).await);
        assert!(!stream_started(&data, "dev", Some("s1")).await, "same broadcast (and login case doesn't matter)");
        assert!(!stream_started(&data, "dev", None).await);
        assert!(stream_started(&data, "dev", Some("s2")).await);
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
