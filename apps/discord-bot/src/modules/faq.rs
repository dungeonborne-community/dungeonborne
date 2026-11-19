//! `/faq topic:<question> [user] [language]` — answer with a section from the FAQ channel. Single source: the bot's message
//! in the FAQ channel (published by setup_server.py) and translations in the "🌍 …" thread under it; sections are "### Question" + a paragraph.
//! Helpers and moderators answer in the channel (and can mention a person); everyone else sees the answer only themselves.
//! Sections are cached for 5 minutes so that autocomplete while typing doesn't hit Discord on every keystroke.

use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(300);

/// FAQ section: title and text
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub title: String,
    pub body: String,
}

/// (when loaded, sections by language: [0] is the main text, then translations in the order of languages)
static CACHE: Mutex<Option<(Instant, Vec<Vec<Section>>)>> = Mutex::new(None);

/// "### Title" + lines up to an empty line; the translation links line and the tail after the sections are dropped
pub fn parse(text: &str) -> Vec<Section> {
    let mut out = Vec::new();
    let mut cur: Option<Section> = None;
    for line in text.lines() {
        if let Some(t) = line.strip_prefix("### ") {
            out.extend(cur.take());
            cur = Some(Section { title: t.trim().to_string(), body: String::new() });
        } else if line.trim().is_empty() || line.starts_with("-# ") {
            out.extend(cur.take());
        } else if let Some(s) = cur.as_mut() {
            if !s.body.is_empty() {
                s.body.push('\n');
            }
            s.body.push_str(line);
        }
    }
    out.extend(cur);
    out
}

async fn load(ctx: &serenity::Context, data: &Data) -> Result<Vec<Vec<Section>>, Error> {
    if let Some((at, s)) = CACHE.lock().unwrap().as_ref() {
        if at.elapsed() < TTL {
            return Ok(s.clone());
        }
    }
    let cfg = data.cfg.faq.as_ref().ok_or("модуль faq выключен")?;
    let channel = data.names.channel(&cfg.channel).ok_or_else(|| format!("канал '{}' не найден", cfg.channel))?;
    let me = ctx.cache.current_user().id;
    let msgs = channel.messages(&ctx.http, serenity::GetMessages::new().limit(50)).await?;
    // the main text is the earliest bot message with sections
    let main = msgs.iter().rev().find(|m| m.author.id == me && m.content.contains("\n### ")).ok_or("в канале FAQ нет сообщения бота с разделами ###")?;
    let mut langs = vec![parse(&main.content)];
    if let Some(thread) = &main.thread {
        let mut tr: Vec<serenity::Message> = thread.id.messages(&ctx.http, serenity::GetMessages::new().limit(50)).await?;
        tr.retain(|m| m.author.id == me && m.content.contains("### "));
        tr.reverse();
        langs.extend(tr.iter().map(|m| parse(&m.content)));
    }
    *CACHE.lock().unwrap() = Some((Instant::now(), langs.clone()));
    Ok(langs)
}

/// Reply language: explicit, otherwise by channel (Cyrillic in the channel name means Russian), otherwise the main one
fn lang_index(data: &Data, explicit: Option<&str>, channel_name: &str) -> usize {
    let codes: Vec<&str> = data.cfg.languages.iter().map(|l| l.code.as_str()).collect();
    let pick = |code: &str| codes.iter().position(|c| *c == code).map(|i| i + 1);
    match explicit {
        Some("en") => 0,
        Some(code) => pick(code).unwrap_or(0),
        None if channel_name.chars().any(|c| ('а'..='я').contains(&c.to_lowercase().next().unwrap_or(c))) => pick("ru").unwrap_or(0),
        None => 0,
    }
}

async fn topics(ctx: Context<'_>, partial: &str) -> serenity::CreateAutocompleteResponse {
    let langs = load(ctx.serenity_context(), ctx.data()).await.unwrap_or_default();
    let p = partial.to_lowercase();
    // search across all languages; the value is the section index
    let mut choices: Vec<serenity::AutocompleteChoice> = Vec::new();
    if let Some(main) = langs.first() {
        for (i, s) in main.iter().enumerate() {
            let hit = langs.iter().filter_map(|l| l.get(i)).any(|t| t.title.to_lowercase().contains(&p) || t.body.to_lowercase().contains(&p));
            if hit {
                let label: String = s.title.chars().take(100).collect();
                choices.push(serenity::AutocompleteChoice::new(label, i.to_string()));
            }
        }
    }
    serenity::CreateAutocompleteResponse::new().set_choices(choices.into_iter().take(25).collect())
}

async fn languages(ctx: Context<'_>, _partial: &str) -> serenity::CreateAutocompleteResponse {
    let mut choices = vec![serenity::AutocompleteChoice::new("English", "en")];
    choices.extend(ctx.data().cfg.languages.iter().map(|l| serenity::AutocompleteChoice::new(format!("{} {}", l.flag, l.name), l.code.clone())));
    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

fn can_post_publicly(data: &Data, member: Option<&serenity::Member>) -> bool {
    let Some(cfg) = &data.cfg.faq else { return false };
    let Some(m) = member else { return false };
    m.permissions.is_some_and(|p| p.administrator() || p.manage_messages())
        || cfg.public_roles.iter().filter_map(|r| data.names.role(r)).any(|r| m.roles.contains(&r))
}

/// Answer with a FAQ section / Ответ разделом FAQ
#[poise::command(slash_command, guild_only)]
pub async fn faq(
    ctx: Context<'_>,
    #[description = "Question / Вопрос"]
    #[autocomplete = "topics"]
    topic: String,
    #[description = "Who to answer (helpers) / Кому ответить"] user: Option<serenity::User>,
    #[description = "Language / Язык"]
    #[autocomplete = "languages"]
    language: Option<String>,
) -> Result<(), Error> {
    let data = ctx.data();
    let cfg = data.cfg.faq.as_ref().ok_or("модуль faq выключен")?;
    let langs = load(ctx.serenity_context(), data).await?;
    let main = langs.first().ok_or("FAQ пуст")?;
    // section index from autocomplete, or a text search if typed by hand
    let idx = topic.parse::<usize>().ok().filter(|i| *i < main.len()).or_else(|| {
        let p = topic.to_lowercase();
        main.iter().position(|s| s.title.to_lowercase().contains(&p))
    });
    let Some(idx) = idx else {
        ctx.send(poise::CreateReply::default().content("❓ —").ephemeral(true)).await?;
        return Ok(());
    };
    let channel_name = ctx.guild_channel().await.map(|c| c.name).unwrap_or_default();
    let li = lang_index(data, language.as_deref(), &channel_name);
    let section = langs.get(li).and_then(|l| l.get(idx)).or_else(|| main.get(idx)).ok_or("раздел не найден")?;
    let faq_link = data.names.channel(&cfg.channel).map(|c| format!("<#{}>", c)).unwrap_or_default();

    let member = match ctx {
        poise::Context::Application(a) => a.interaction.member.as_deref().cloned(),
        _ => None,
    };
    let public = can_post_publicly(data, member.as_ref());
    let to = user.as_ref().filter(|_| public).map(|u| format!("<@{}> ", u.id)).unwrap_or_default();
    let text = format!("{}### {}\n{}\n-# ❓ {}", to, section.title, section.body, faq_link);
    let mentions = serenity::CreateAllowedMentions::new().users(user.iter().map(|u| u.id).collect::<Vec<_>>());
    ctx.send(poise::CreateReply::default().content(text).ephemeral(!public).allowed_mentions(mentions)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse, Section};

    #[test]
    fn sections_from_faq_message() {
        let text = "# ❓ FAQ\n### Error 13\nWait 30 s.\n### CURLCode 28\nUse WARP.\nOr VPN.\n\n**Still stuck?** Post in help.\n-# 🌍 🇷🇺 [Русский](x)";
        assert_eq!(
            parse(text),
            vec![
                Section { title: "Error 13".into(), body: "Wait 30 s.".into() },
                Section { title: "CURLCode 28".into(), body: "Use WARP.\nOr VPN.".into() },
            ]
        );
    }
}
