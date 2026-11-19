//! Bug reports from the game and the launcher → posts in the bug forum. Contract: docs/bug-report-api.md.
//! The client sends a report to the backend on behalf of the player; every `every_secs` the bot fetches new ones via the admin API
//! (`GET …?status=new`), creates a post with the logs and reports the link back to the backend (`POST …/{id}/posted`).
//! Published ids are stored in `bug_report:<id>`: if the acknowledgement didn't get through, the bot retries it but won't create a second post.

use crate::Data;
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use std::time::Duration;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE bug_report SCHEMAFULL;
DEFINE FIELD OVERWRITE thread ON bug_report TYPE string;
DEFINE FIELD OVERWRITE url ON bug_report TYPE string;
DEFINE FIELD OVERWRITE acked ON bug_report TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE at ON bug_report TYPE datetime DEFAULT time::now() READONLY;
"#;

/// Bot attachment limit per Discord message
const MAX_FILES_BYTES: u64 = 10 * 1024 * 1024 - 64 * 1024;

#[derive(Deserialize, Debug, Clone, Default)]
pub struct Versions {
    #[serde(default)]
    pub game: Option<String>,
    #[serde(default)]
    pub launcher: Option<String>,
    #[serde(default)]
    pub mods: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FileInfo {
    pub name: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Report {
    pub id: String,
    pub discord_id: String,
    #[serde(default)]
    pub created_at: Option<i64>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub battle_id: Option<String>,
    #[serde(default)]
    pub versions: Versions,
    /// Where in the game the form was opened (Setting, Login, Market…), if the report was started from the game
    #[serde(default)]
    pub scene: Option<String>,
    /// sha256 of Dungeonborne.exe: the game has no version string, the hash identifies the build
    #[serde(default)]
    pub exe_sha256: Option<String>,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub files: Vec<FileInfo>,
}

/// Post text (without attachments); `skipped` = files that didn't fit into the limit
pub fn post_text(r: &Report, skipped: &[&FileInfo]) -> String {
    let author = r.discord_id.parse::<u64>().map(|id| format!("<@{}>", id)).unwrap_or_else(|_| "?".into());
    let when = r.created_at.map(|t| format!(" · <t:{}:f>", t)).unwrap_or_default();
    let mut facts = Vec::new();
    if let Some(b) = r.battle_id.as_deref().filter(|b| !b.is_empty()) {
        facts.push(format!("**Battle:** {}", b));
    }
    for (name, v) in [("Game", &r.versions.game), ("Launcher", &r.versions.launcher), ("Mods", &r.versions.mods)] {
        if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
            facts.push(format!("{} {}", name, v));
        }
    }
    if let Some(h) = r.exe_sha256.as_deref().filter(|h| h.len() >= 8) {
        facts.push(format!("exe `{}`", &h[..8]));
    }
    if let Some(os) = r.os.as_deref().filter(|o| !o.is_empty()) {
        facts.push(os.to_string());
    }
    let desc: String = r.description.chars().take(1500).collect::<String>().lines().map(|l| format!("> {}\n", l)).collect();
    let source = if r.source.is_empty() { "launcher".to_string() } else { r.source.clone() };
    let source = match r.scene.as_deref().filter(|s| !s.is_empty()) {
        Some(scene) => format!("{} ({})", source, scene),
        None => source,
    };
    let mut text = format!("📋 Report from {} · {}{}\n{}\n\n{}", author, source, when, facts.join(" · "), desc);
    if !skipped.is_empty() {
        let list: Vec<String> = skipped.iter().map(|f| format!("{} ({} KB)", f.name, f.size / 1024)).collect();
        text.push_str(&format!("-# Not attached, too large: {}\n", list.join(", ")));
    }
    text.chars().take(2000).collect()
}

/// Which files to attach: in order, while the total fits into the limit
pub fn fit_files(files: &[FileInfo]) -> (Vec<&FileInfo>, Vec<&FileInfo>) {
    let (mut take, mut skip, mut total) = (Vec::new(), Vec::new(), 0u64);
    for f in files {
        if total + f.size <= MAX_FILES_BYTES {
            total += f.size;
            take.push(f);
        } else {
            skip.push(f);
        }
    }
    (take, skip)
}

fn tag_names(cfg: &crate::config::BugReportsCfg, r: &Report) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(t) = cfg.kind_tags.get(&r.kind) {
        tags.push(t.clone());
    }
    let lang = r.lang.as_deref().unwrap_or("").split('-').next().unwrap_or("");
    tags.push(cfg.lang_tags.get(lang).cloned().unwrap_or_else(|| cfg.default_lang_tag.clone()));
    tags
}

fn auth(data: &Data, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match data.cfg.bug_reports.as_ref().and_then(|c| c.bearer_env.as_ref()).and_then(|v| std::env::var(v).ok()) {
        Some(t) => req.bearer_auth(t.trim()),
        None => req,
    }
}

async fn ack(data: &Data, id: &str, thread: &str, url: &str) -> bool {
    let Some(cfg) = &data.cfg.bug_reports else { return false };
    let req = data.http.post(format!("{}/{}/posted", cfg.api, id)).json(&serde_json::json!({ "thread_id": thread, "url": url }));
    match auth(data, req).send().await {
        Ok(r) if r.status().is_success() => {
            let _ = data.db.db.query("UPDATE type::thing('bug_report', $id) SET acked = true").bind(("id", id.to_string())).await;
            true
        }
        Ok(r) => {
            eprintln!("[BugReports] ack {} → {}", id, r.status());
            false
        }
        Err(e) => {
            eprintln!("[BugReports] ack {}: {}", id, e);
            false
        }
    }
}

#[derive(Deserialize)]
struct Posted {
    thread: String,
    url: String,
    acked: bool,
}

async fn publish(ctx: &serenity::Context, data: &Data, r: &Report) -> Result<(String, String), crate::Error> {
    let cfg = data.cfg.bug_reports.as_ref().ok_or("module disabled")?;
    let forum = data.names.channel(&cfg.forum).ok_or_else(|| format!("forum '{}' not found", cfg.forum))?;
    let (take, skip) = fit_files(&r.files);
    let mut msg = serenity::CreateMessage::new().content(post_text(r, &skip)).allowed_mentions(
        serenity::CreateAllowedMentions::new().users(r.discord_id.parse::<u64>().ok().map(serenity::UserId::new).into_iter().collect::<Vec<_>>()),
    );
    for f in take {
        let resp = auth(data, data.http.get(format!("{}/{}/files/{}", cfg.api, r.id, f.name))).send().await?.error_for_status()?;
        msg = msg.add_file(serenity::CreateAttachment::bytes(resp.bytes().await?.to_vec(), f.name.clone()));
    }
    let wanted = tag_names(cfg, r);
    let tags: Vec<serenity::ForumTagId> = forum
        .to_channel(&ctx.http)
        .await?
        .guild()
        .map(|g| g.available_tags.into_iter().filter(|t| wanted.contains(&t.name)).map(|t| t.id).collect())
        .unwrap_or_default();
    let title: String = if r.title.trim().is_empty() { format!("Report {}", r.id) } else { r.title.trim().chars().take(100).collect() };
    let mut post = serenity::CreateForumPost::new(title, msg);
    for t in tags {
        post = post.add_applied_tag(t);
    }
    let thread = forum.create_forum_post(&ctx.http, post).await?;
    let url = format!("https://discord.com/channels/{}/{}", data.guild, thread.id);
    Ok((thread.id.to_string(), url))
}

async fn tick(ctx: &serenity::Context, data: &Data) {
    let Some(cfg) = &data.cfg.bug_reports else { return };
    let resp = match auth(data, data.http.get(format!("{}?status=new&limit=10", cfg.api))).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => return eprintln!("[BugReports] {} → {}", cfg.api, r.status()),
        Err(_) => return, // backend is down: wait quietly
    };
    let reports: Vec<Report> = match resp.json().await {
        Ok(v) => v,
        Err(e) => return eprintln!("[BugReports] backend response doesn't match the contract: {}", e),
    };
    for r in reports {
        // already published? only retry the acknowledgement
        let seen: Option<Posted> = data
            .db
            .db
            .query("SELECT thread, url, acked FROM ONLY type::thing('bug_report', $id)")
            .bind(("id", r.id.clone()))
            .await
            .ok()
            .and_then(|mut q| q.take(0).ok().flatten());
        if let Some(p) = seen {
            if !p.acked {
                ack(data, &r.id, &p.thread, &p.url).await;
            }
            continue;
        }
        match publish(ctx, data, &r).await {
            Ok((thread, url)) => {
                let saved = data
                    .db
                    .db
                    .query("CREATE type::thing('bug_report', $id) SET thread = $t, url = $u")
                    .bind(("id", r.id.clone()))
                    .bind(("t", thread.clone()))
                    .bind(("u", url.clone()))
                    .await
                    .and_then(|q| q.check());
                if let Err(e) = saved {
                    eprintln!("[BugReports] save {}: {}", r.id, e);
                }
                println!("[BugReports] {} → {}", r.id, url);
                ack(data, &r.id, &thread, &url).await;
            }
            Err(e) => eprintln!("[BugReports] publish {}: {}", r.id, e),
        }
    }
}

pub fn spawn(ctx: serenity::Context, data: Data) {
    let Some(cfg) = &data.cfg.bug_reports else { return };
    let every = Duration::from_secs(cfg.every_secs.max(10));
    println!("[BugReports] {} every {} s → #{}", cfg.api, every.as_secs(), cfg.forum);
    tokio::spawn(async move {
        loop {
            tick(&ctx, &data).await;
            tokio::time::sleep(every).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        serde_json::from_value(serde_json::json!({
            "id": "r1", "discord_id": "42", "created_at": 1763150400, "source": "game", "kind": "crash",
            "title": "Stash empty", "description": "line one\nline two", "battle_id": "48213",
            "versions": {"game": "1.0.3", "mods": "0.4.2"}, "os": "Windows 11", "lang": "ru-RU",
            "files": [{"name": "relay.log", "size": 9_000_000}, {"name": "launcher.log", "size": 2_000_000}]
        }))
        .unwrap()
    }

    #[test]
    fn text_and_file_budget() {
        let r = report();
        let (take, skip) = fit_files(&r.files);
        assert_eq!((take.len(), skip[0].name.as_str()), (1, "launcher.log"));
        let t = post_text(&r, &skip);
        assert!(t.starts_with("📋 Report from <@42> · game · <t:1763150400:f>\n**Battle:** 48213 · Game 1.0.3 · Mods 0.4.2 · Windows 11\n\n> line one\n> line two\n"));
        assert!(t.contains("Not attached, too large: launcher.log (1953 KB)"));
    }
}
