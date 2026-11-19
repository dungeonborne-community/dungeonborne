//! Server changelog → patch-notes channel. Same source as the launcher and the site: the backend serves
//! `seasons/<season>/patch_notes.toml` at `GET /dbserver/patch_notes?lang=..` (format `dbnotes/1`, no auth).
//! The bot on the same VM reads it every `every_secs` and publishes new entries: the main language as a channel post,
//! translations in a "🌍 …" thread under it, links and language buttons (buttons are handled by translate.rs), subscriber broadcast.
//! Published entries are stored as `patch_note:<entry id>`; on the first run everything already present is silently marked
//! (except the last `backfill` entries) so the channel isn't flooded with history.

use crate::Data;
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use std::time::Duration;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE patch_note SCHEMAFULL;
DEFINE FIELD OVERWRITE message ON patch_note TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE at ON patch_note TYPE datetime DEFAULT time::now() READONLY;
"#;

/// Text characters per message: Discord allows 2000, leave room for translation links and the ping
const CHUNK: usize = 1750;

#[derive(Deserialize, Debug, Clone)]
pub struct Section {
    pub kind: String,
    pub items: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Update {
    pub id: String,
    #[serde(default)]
    pub version: String,
    pub date: String,
    pub title: String,
    pub lang: String,
    pub sections: Vec<Section>,
}

#[derive(Deserialize, Debug)]
pub struct Notes {
    pub updates: Vec<Update>,
}

fn heading(kind: &str, lang: &str) -> &'static str {
    let ru = lang == "ru";
    match kind {
        "new" => if ru { "✨ Новое" } else { "✨ New" },
        "fixed" => if ru { "🔧 Исправлено" } else { "🔧 Fixed" },
        "balance" => if ru { "⚖️ Баланс" } else { "⚖️ Balance" },
        _ => "•",
    }
}

/// Unix time of noon on date YYYY-MM-DD (UTC): the <t:…:D> tag shows the date in the reader's time zone
pub fn noon_unix(date: &str) -> Option<i64> {
    let p: Vec<i64> = date.split('-').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let (y, m, d) = (*p.first()?, *p.get(1)?, *p.get(2)?);
    // days since 1970-01-01 (Hinnant's algorithm)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe - 719468) * 86400 + 12 * 3600)
}

/// Entry → messages no longer than CHUNK: heading in the first, sections and items after; an item is never split
pub fn render(u: &Update) -> Vec<String> {
    let when = noon_unix(&u.date).map(|t| format!("<t:{}:D>", t)).unwrap_or_else(|| u.date.clone());
    let sub = if u.version.is_empty() || u.version == u.date { when } else { format!("{} · {}", u.version, when) };
    let mut chunks = vec![format!("# {}\n-# {}\n", u.title.trim(), sub)];
    for s in &u.sections {
        let head = format!("### {}\n", heading(&s.kind, &u.lang));
        let mut first = true;
        for item in &s.items {
            let line = format!("- {}\n", item.trim().replace('\n', " "));
            let piece = if first { format!("{}{}", head, line) } else { line };
            let cur = chunks.last_mut().unwrap();
            if cur.chars().count() + piece.chars().count() > CHUNK {
                // a new chunk continues the section with its heading
                let cont = if first { piece.clone() } else { format!("{}{}", head, piece) };
                chunks.push(cont.chars().take(CHUNK).collect());
            } else {
                cur.push_str(&piece);
            }
            first = false;
        }
    }
    chunks.into_iter().map(|c| c.trim_end().to_string()).collect()
}

async fn fetch(data: &Data, lang: &str) -> Option<Notes> {
    let cfg = data.cfg.patch_notes.as_ref()?;
    let resp = data.http.get(&cfg.url).query(&[("lang", lang)]).send().await.ok()?;
    if !resp.status().is_success() {
        eprintln!("[PatchNotes] {} ?lang={} → {}", cfg.url, lang, resp.status());
        return None;
    }
    match resp.json::<Notes>().await {
        Ok(n) => Some(n),
        Err(e) => {
            eprintln!("[PatchNotes] backend response is not dbnotes/1: {}", e);
            None
        }
    }
}

async fn seen(data: &Data, id: &str) -> bool {
    let r: Option<bool> = data
        .db
        .db
        .query("RETURN record::exists(type::thing('patch_note', $id))")
        .bind(("id", id.to_string()))
        .await
        .ok()
        .and_then(|mut q| q.take(0).ok().flatten());
    r.unwrap_or(true) // couldn't check — treat as published: better to skip than to repeat
}

async fn mark(data: &Data, id: &str, message: &str) {
    let r = data
        .db
        .db
        .query("UPSERT type::thing('patch_note', $id) SET message = $m")
        .bind(("id", id.to_string()))
        .bind(("m", message.to_string()))
        .await
        .and_then(|q| q.check());
    if let Err(e) = r {
        eprintln!("[PatchNotes] mark {}: {}", id, e);
    }
}

async fn post(ctx: &serenity::Context, data: &Data, main: &Update, translations: &[(crate::config::AnnounceLang, Update)]) -> Result<String, crate::Error> {
    let cfg = data.cfg.patch_notes.as_ref().ok_or("module disabled")?;
    let channel = data.names.channel(&cfg.channel).ok_or_else(|| format!("channel '{}' not found", cfg.channel))?;
    let ping_role = cfg.ping.as_deref().and_then(|r| data.names.role(r));
    let mut chunks = render(main);
    let footer = format!("-# {}{}", cfg.footer, ping_role.map(|r| format!(" · <@&{}>", r)).unwrap_or_default());
    if let Some(last) = chunks.last_mut() {
        last.push('\n');
        last.push_str(&footer);
    }
    let is_news = channel.to_channel(&ctx.http).await.ok().and_then(|c| c.guild()).is_some_and(|c| c.kind == serenity::ChannelType::News);
    let mut sent = Vec::new();
    for (i, text) in chunks.iter().enumerate() {
        let last = i + 1 == chunks.len();
        let mentions = match (last, ping_role) {
            (true, Some(r)) => serenity::CreateAllowedMentions::new().roles(vec![r]),
            _ => serenity::CreateAllowedMentions::new(),
        };
        let m = channel
            .send_message(&ctx.http, serenity::CreateMessage::new().content(text).allowed_mentions(mentions).flags(serenity::MessageFlags::SUPPRESS_EMBEDS))
            .await?;
        if is_news {
            if let Err(e) = m.crosspost(&ctx.http).await {
                eprintln!("[PatchNotes] subscriber broadcast: {}", e);
            }
        }
        sent.push(m);
    }
    let first = sent.first().ok_or("empty entry")?;

    // translations: a thread under the first message, one message per chunk; the link and button lead to the start of the translation
    let langs: Vec<&(crate::config::AnnounceLang, Update)> = translations.iter().filter(|(l, u)| u.lang == l.code).collect();
    if !langs.is_empty() {
        let name = format!("🌍 {}", data.cfg.languages.iter().map(|l| format!("{} {}", l.flag, l.name)).collect::<Vec<_>>().join(" · "));
        let thread = channel
            .create_thread_from_message(&ctx.http, first.id, serenity::CreateThread::new(name).auto_archive_duration(serenity::AutoArchiveDuration::OneWeek))
            .await?;
        let mut links = Vec::new();
        let mut buttons = Vec::new();
        for (lang, u) in langs {
            let mut start = None;
            for text in render(u) {
                let m = thread
                    .send_message(&ctx.http, serenity::CreateMessage::new().content(text).allowed_mentions(serenity::CreateAllowedMentions::new()).flags(serenity::MessageFlags::SUPPRESS_EMBEDS))
                    .await?;
                start.get_or_insert(m.id);
            }
            let Some(start) = start else { continue };
            links.push(format!("{} [{}](https://discord.com/channels/{}/{}/{})", lang.flag, lang.name, data.guild, thread.id, start));
            buttons.push(
                serenity::CreateButton::new(format!("tr:{}:{}", thread.id, start))
                    .label(&lang.name)
                    .emoji(serenity::ReactionType::Unicode(lang.flag.clone()))
                    .style(serenity::ButtonStyle::Secondary),
            );
        }
        if !links.is_empty() {
            let rows = buttons.chunks(5).map(|c| serenity::CreateActionRow::Buttons(c.to_vec())).collect();
            let mut msg = first.clone();
            msg.edit(
                &ctx.http,
                serenity::EditMessage::new()
                    .content(format!("{}\n-# 🌍 **Translate:** {}", first.content, links.join(" · ")))
                    .components(rows)
                    .allowed_mentions(serenity::CreateAllowedMentions::new()),
            )
            .await?;
        }
    }
    Ok(format!("https://discord.com/channels/{}/{}/{}", data.guild, channel, first.id))
}

async fn tick(ctx: &serenity::Context, data: &Data, first_run: &mut bool) {
    let Some(cfg) = &data.cfg.patch_notes else { return };
    let Some(main) = fetch(data, &cfg.main_lang).await else { return };
    let mut translations: Vec<(crate::config::AnnounceLang, Notes)> = Vec::new();
    for l in data.cfg.languages.iter().filter(|l| l.code != cfg.main_lang) {
        if let Some(n) = fetch(data, &l.code).await {
            translations.push((l.clone(), n));
        }
    }
    // new = only above the newest already published entry (history won't resurface even if a mark got lost);
    // publish the oldest first
    let mut fresh = Vec::new();
    for u in &main.updates {
        if seen(data, &u.id).await {
            break;
        }
        fresh.push(u.clone());
    }
    fresh.reverse();
    if *first_run {
        *first_run = false;
        // first run: history is not published, except the last backfill entries
        let keep = cfg.backfill.min(fresh.len());
        let skip = fresh.len() - keep;
        for u in &fresh[..skip] {
            mark(data, &u.id, "").await;
        }
        if skip > 0 {
            println!("[PatchNotes] first run: {} entries marked without publishing", skip);
        }
        fresh.drain(..skip);
    }
    for u in fresh {
        if u.lang != cfg.main_lang {
            eprintln!("[PatchNotes] entry {} has no {} translation — publishing as is ({})", u.id, cfg.main_lang, u.lang);
        }
        let tr: Vec<(crate::config::AnnounceLang, Update)> = translations
            .iter()
            .filter_map(|(l, n)| n.updates.iter().find(|x| x.id == u.id).filter(|x| x.lang != u.lang).map(|x| (l.clone(), x.clone())))
            .collect();
        match post(ctx, data, &u, &tr).await {
            Ok(link) => {
                mark(data, &u.id, &link).await;
                println!("[PatchNotes] {} → {}", u.id, link);
            }
            Err(e) => {
                eprintln!("[PatchNotes] publish {}: {}", u.id, e);
                break; // retry next time, the order is preserved
            }
        }
    }
}

pub fn spawn(ctx: serenity::Context, data: Data) {
    let Some(cfg) = &data.cfg.patch_notes else { return };
    let every = Duration::from_secs(cfg.every_secs.max(30));
    println!("[PatchNotes] {} every {} s → #{}", cfg.url, every.as_secs(), cfg.channel);
    tokio::spawn(async move {
        // first run — while the database has no marks yet
        let known: Option<i64> = data.db.db.query("RETURN array::len(SELECT VALUE record::id(id) FROM patch_note)").await.ok().and_then(|mut q| q.take(0).ok().flatten());
        // couldn't read — not a first run: otherwise the history would go to the channel
        let mut first_run = known == Some(0);
        loop {
            tick(&ctx, &data, &mut first_run).await;
            tokio::time::sleep(every).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upd(items: usize, len: usize) -> Update {
        Update {
            id: "v1".into(),
            version: "2026.11.12.3".into(),
            date: "2026-11-12".into(),
            title: "Update of 12 November".into(),
            lang: "en".into(),
            sections: vec![
                Section { kind: "new".into(), items: (0..items).map(|i| format!("{} {}", i, "x".repeat(len))).collect() },
                Section { kind: "fixed".into(), items: vec!["Bug".into()] },
            ],
        }
    }

    #[tokio::test]
    async fn seen_after_mark() {
        let (data, dir) = crate::modules::testing::data("patchnotes", "").await;
        assert!(!seen(&data, "2026.11.07").await);
        mark(&data, "2026.11.07", "").await;
        assert!(seen(&data, "2026.11.07").await);
        assert!(!seen(&data, "2026.11.08").await);
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn date_to_noon_utc() {
        assert_eq!(noon_unix("1970-01-01"), Some(12 * 3600));
        assert_eq!(noon_unix("2026-11-12"), Some(1_794_484_800));
    }

    #[test]
    fn short_update_is_one_message() {
        let c = render(&upd(2, 10));
        assert_eq!(c.len(), 1);
        assert!(c[0].starts_with("# Update of 12 November\n-# 2026.11.12.3 · <t:1794484800:D>\n### ✨ New\n- 0 xxxxxxxxxx\n"));
        assert!(c[0].ends_with("### 🔧 Fixed\n- Bug"));
    }

    #[test]
    fn long_update_splits_on_items_and_repeats_heading() {
        let c = render(&upd(8, 400));
        assert!(c.len() > 1);
        assert!(c.iter().all(|x| x.chars().count() <= CHUNK));
        assert!(c[1].starts_with("### ✨ New\n- "));
    }
}
