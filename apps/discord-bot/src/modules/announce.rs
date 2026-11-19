//! `/announce`: a post to a news channel following the channel guidelines:
//! form (English text + translations) → a preview only the author sees (Post,
//! Edit and Cancel buttons) → publishing: one role pinged in the last line, a "🌍 …" thread with translations under the post,
//! language links and buttons (handled by translate.rs), crossposting to followers for an announcement channel,
//! copies in language channels (`mirror` in bot.toml): the whole translation with a link to the original. If the original is deleted (manually
//! or by timer), `on_delete` finds the copies by that link and deletes them; the same goes for posts from post_announcement.py.
//! The text supports {#channel} and {@role} (as in bot.toml) and Discord timestamps <t:UNIX:F>.
//! A draft is an `announce_draft:<id>` record; Post takes it atomically
//! (`UPDATE … WHERE status = 'draft' RETURN BEFORE`), so a double click won't publish the post twice.

use crate::config::{AnnounceCfg, AnnounceLang};
use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;
use serde::Deserialize;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE announce_draft SCHEMAFULL;
DEFINE FIELD OVERWRITE author ON announce_draft TYPE string;
DEFINE FIELD OVERWRITE channel ON announce_draft TYPE string;
DEFINE FIELD OVERWRITE ping ON announce_draft TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE texts ON announce_draft TYPE array<string> DEFAULT [];
DEFINE FIELD OVERWRITE expire ON announce_draft TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE status ON announce_draft TYPE string DEFAULT "draft";
DEFINE FIELD OVERWRITE at ON announce_draft TYPE datetime DEFAULT time::now() READONLY;
DEFINE EVENT OVERWRITE announce_retention ON announce_draft WHEN $event = "CREATE" THEN {
    DELETE announce_draft WHERE at < time::now() - 1d;
};
"#;

/// Form field: post text + (optionally) rules after the RULES_MARK line; Discord allows up to 4000 per field
const MAX_TEXT: u16 = 4000;
/// Post text: the Discord message limit minus room for the translation links line
const MAX_POST: usize = 1800;
/// After this line in the form field come the rules: the bot puts them in a card (embed) under the text
pub const RULES_MARK: &str = "---rules---";
const NO_PING: &str = "—";
const EVERYONE: &str = "@everyone";

#[derive(Deserialize, Clone)]
struct Draft {
    author: String,
    channel: String,
    ping: String,
    texts: Vec<String>,
    #[serde(default)]
    expire: String,
}

async fn channel_names(ctx: Context<'_>, partial: &str) -> serenity::CreateAutocompleteResponse {
    let choices = ctx
        .data()
        .cfg
        .announce
        .iter()
        .flat_map(|a| &a.channel)
        .filter(|c| c.name.to_lowercase().contains(&partial.to_lowercase()))
        .map(|c| serenity::AutocompleteChoice::new(c.name.clone(), c.name.clone()))
        .collect();
    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

async fn ping_names(ctx: Context<'_>, partial: &str) -> serenity::CreateAutocompleteResponse {
    let mut names = vec![NO_PING.to_string()];
    if let Some(a) = &ctx.data().cfg.announce {
        let mut roles: Vec<String> = a.channel.iter().flat_map(|c| c.pings.clone()).collect();
        roles.dedup();
        names.extend(roles);
    }
    names.push(EVERYONE.to_string());
    let choices = names
        .into_iter()
        .filter(|n| n.to_lowercase().contains(&partial.to_lowercase()))
        .map(|n| serenity::AutocompleteChoice::new(n.clone(), n))
        .collect();
    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

/// Пост в канал новостей: форма → предпросмотр → публикация с переводами
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_EVENTS")]
pub async fn announce(
    ctx: Context<'_>,
    #[description = "Channel / Канал"]
    #[autocomplete = "channel_names"]
    channel: String,
    #[description = "Ping one role (— no ping) / Пинг одной роли"]
    #[autocomplete = "ping_names"]
    ping: Option<String>,
    #[description = "Delete the post after: 12h, 7d (empty = keep) / Удалить через"] delete_after: Option<String>,
) -> Result<(), Error> {
    let data = ctx.data();
    let cfg = data.cfg.announce.as_ref().ok_or("модуль announce выключен в bot.toml")?;
    let ch = cfg.channel.iter().find(|c| c.name == channel).ok_or("этого канала нет в [announce] bot.toml")?;
    let ping = ping.filter(|p| p != NO_PING).unwrap_or_default();
    if !ping.is_empty() && ping != EVERYONE && !ch.pings.contains(&ping) {
        return Err(format!("в {} можно пинговать только: {}", ch.name, ch.pings.join(", ")).into());
    }
    let id = new_draft(data, ctx.author().id, &ch.name, &ping, delete_after.as_deref().unwrap_or("")).await?;
    let template = ch.template.clone().unwrap_or_default();
    let modal = form(&id, &data.cfg.languages, &[template]);
    let poise::Context::Application(actx) = ctx else { return Err("только слеш-команда".into()) };
    actx.interaction.create_response(ctx.http(), serenity::CreateInteractionResponse::Modal(modal)).await?;
    actx.has_sent_initial_response.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

async fn new_draft(data: &Data, author: serenity::UserId, channel: &str, ping: &str, expire: &str) -> Result<String, Error> {
    let mut r = data
        .db
        .db
        .query("CREATE announce_draft SET author = $a, channel = $c, ping = $p, expire = $e RETURN VALUE record::id(id)")
        .bind(("a", author.to_string()))
        .bind(("c", channel.to_string()))
        .bind(("p", ping.to_string()))
        .bind(("e", expire.trim().to_string()))
        .await?
        .check()?;
    let id: Option<String> = r.take(0)?;
    id.ok_or_else(|| "черновик не создан".into())
}

fn form(id: &str, langs: &[AnnounceLang], values: &[String]) -> serenity::CreateModal {
    let mut rows = vec![serenity::CreateActionRow::InputText(
        serenity::CreateInputText::new(serenity::InputTextStyle::Paragraph, "English (main post)", "t0")
            .max_length(MAX_TEXT)
            .required(true)
            .value(values.first().cloned().unwrap_or_default()),
    )];
    // a Discord form has up to 5 fields: English + up to 4 translations
    for (i, l) in langs.iter().take(4).enumerate() {
        let mut field = serenity::CreateInputText::new(serenity::InputTextStyle::Paragraph, format!("{} {}", l.flag, l.name), format!("t{}", i + 1))
            .max_length(MAX_TEXT)
            .required(false);
        if let Some(v) = values.get(i + 1).filter(|v| !v.is_empty()) {
            field = field.value(v.clone());
        }
        rows.push(serenity::CreateActionRow::InputText(field));
    }
    serenity::CreateModal::new(format!("an:form:{}", id), "Announcement / Анонс").components(rows)
}

fn input(mi: &serenity::ModalInteraction, id: &str) -> String {
    mi.data
        .components
        .iter()
        .flat_map(|row| &row.components)
        .find_map(|c| match c {
            serenity::ActionRowComponent::InputText(t) if t.custom_id == id => t.value.clone(),
            _ => None,
        })
        .unwrap_or_default()
}

async fn load(data: &Data, id: &str) -> Option<Draft> {
    let mut r = data
        .db
        .db
        .query("SELECT author, channel, ping, texts, expire FROM ONLY type::thing('announce_draft', $id) WHERE status = 'draft'")
        .bind(("id", id.to_string()))
        .await
        .ok()?;
    r.take::<Option<Draft>>(0).ok()?
}

/// Post text with ping: the role is appended to the last `-# …` line (or as a separate small line)
pub fn with_ping(text: &str, mention: &str) -> String {
    let text = text.trim_end();
    if mention.is_empty() {
        return text.to_string();
    }
    match text.rsplit_once('\n') {
        Some((head, last)) if last.starts_with("-# ") => format!("{}\n{} · {}", head, last, mention),
        None if text.starts_with("-# ") => format!("{} · {}", text, mention),
        _ => format!("{}\n-# {}", text, mention),
    }
}

/// "text\n---rules---\n### Title\nrules" → (text, Some((title, rules)))
pub fn split_rules(text: &str) -> (String, Option<(Option<String>, String)>) {
    let Some((post, rules)) = text.split_once(RULES_MARK) else { return (text.trim_end().to_string(), None) };
    let rules = rules.trim();
    if rules.is_empty() {
        return (post.trim_end().to_string(), None);
    }
    let (title, body) = match rules.split_once('\n') {
        Some((first, rest)) if first.starts_with('#') => (Some(first.trim_start_matches('#').trim().to_string()), rest.trim().to_string()),
        None if rules.starts_with('#') => (Some(rules.trim_start_matches('#').trim().to_string()), String::new()),
        _ => (None, rules.to_string()),
    };
    (post.trim_end().to_string(), Some((title, body)))
}

fn rules_card(card: &(Option<String>, String)) -> serenity::CreateEmbed {
    let mut e = serenity::CreateEmbed::new().description(card.1.chars().take(4000).collect::<String>()).color(0xE0A84A);
    if let Some(t) = &card.0 {
        e = e.title(t.chars().take(250).collect::<String>());
    }
    e
}

/// Message: text + rules card. Link previews are suppressed only without a card, since the flag would hide the card too
fn post_message(content: &str, card: Option<&(Option<String>, String)>) -> serenity::CreateMessage {
    let msg = serenity::CreateMessage::new().content(content);
    match card {
        Some(c) => msg.embed(rules_card(c)),
        None => msg.flags(serenity::MessageFlags::SUPPRESS_EMBEDS),
    }
}

fn mention_of(data: &Data, ping: &str) -> String {
    match ping {
        "" => String::new(),
        EVERYONE => "@everyone".into(),
        role => data.names.role(role).map(|r| format!("<@&{}>", r)).unwrap_or_default(),
    }
}

fn preview(data: &Data, id: &str, d: &Draft, ru: bool) -> serenity::CreateInteractionResponseMessage {
    let langs = data.cfg.languages.as_slice();
    let filled: Vec<String> = langs
        .iter()
        .zip(d.texts.iter().skip(1))
        .filter(|(_, t)| !t.trim().is_empty())
        .map(|(l, _)| format!("{} {}", l.flag, l.name))
        .collect();
    let missing: Vec<String> = langs
        .iter()
        .enumerate()
        .filter(|(i, _)| d.texts.get(i + 1).map_or(true, |t| t.trim().is_empty()))
        .map(|(_, l)| format!("{} {}", l.flag, l.name))
        .collect();
    let everyone = d.ping == EVERYONE;
    let mut head = if ru {
        format!("**Предпросмотр** → {} · пинг: {} · переводы: {}", d.channel, if d.ping.is_empty() { "нет" } else { &d.ping }, if filled.is_empty() { "нет".into() } else { filled.join(", ") })
    } else {
        format!("**Preview** → {} · ping: {} · translations: {}", d.channel, if d.ping.is_empty() { "none" } else { &d.ping }, if filled.is_empty() { "none".into() } else { filled.join(", ") })
    };
    if !missing.is_empty() {
        head += &if ru { format!("\n⚠️ Нет перевода: {} — его можно добавить позже в ветку 🌍", missing.join(", ")) } else { format!("\n⚠️ Missing: {} — you can add it later in the 🌍 thread", missing.join(", ")) };
    }
    if !d.expire.is_empty() {
        head += &if ru { format!("\n🗑 Пост удалится через {}", d.expire) } else { format!("\n🗑 The post will be deleted after {}", d.expire) };
    }
    if everyone {
        head += if ru { "\n🚨 **@everyone** — только для вайпа, долгого падения или угрозы безопасности" } else { "\n🚨 **@everyone** — only for a wipe, a long outage or a security threat" };
    }
    let first = data.names.render(d.texts.first().map(String::as_str).unwrap_or(""), None, &data.server_name);
    let (first, card) = split_rules(&first);
    if first.chars().count() > MAX_POST {
        head += &if ru { format!("\n⛔ Текст поста {} символов — нужно не больше {}", first.chars().count(), MAX_POST) } else { format!("\n⛔ Post text is {} characters — max {}", first.chars().count(), MAX_POST) };
    }
    let body = with_ping(&first, &if d.ping.is_empty() { String::new() } else { format!("`{}`", d.ping) });
    let mut content = format!("{}\n━━━━━━━━━━━━\n{}", head, body);
    if content.chars().count() > 2000 {
        content = content.chars().take(1990).collect::<String>() + "…";
    }
    let post = serenity::CreateButton::new(format!("an:post:{}", id))
        .label(if everyone { if ru { "Опубликовать с @everyone" } else { "Post with @everyone" } } else if ru { "Опубликовать" } else { "Post" })
        .style(if everyone { serenity::ButtonStyle::Danger } else { serenity::ButtonStyle::Success });
    let edit = serenity::CreateButton::new(format!("an:edit:{}", id)).label(if ru { "Изменить" } else { "Edit" }).style(serenity::ButtonStyle::Secondary);
    let cancel = serenity::CreateButton::new(format!("an:cancel:{}", id)).label(if ru { "Отмена" } else { "Cancel" }).style(serenity::ButtonStyle::Secondary);
    let mut reply = serenity::CreateInteractionResponseMessage::new()
        .content(content)
        .ephemeral(true)
        .allowed_mentions(serenity::CreateAllowedMentions::new())
        .components(vec![serenity::CreateActionRow::Buttons(vec![post, edit, cancel])]);
    if let Some(c) = &card {
        reply = reply.embed(rules_card(c));
    }
    reply
}

/// Form submitted → save the texts → preview
pub async fn on_modal(ctx: &serenity::Context, data: &Data, mi: &serenity::ModalInteraction) {
    let Some(id) = mi.data.custom_id.strip_prefix("an:form:") else { return };
    let langs = data.cfg.languages.len().min(4);
    let texts: Vec<String> = (0..=langs).map(|i| input(mi, &format!("t{}", i)).trim().to_string()).collect();
    let saved = data
        .db
        .db
        .query("UPDATE type::thing('announce_draft', $id) SET texts = $t WHERE status = 'draft' AND author = $a")
        .bind(("id", id.to_string()))
        .bind(("t", texts))
        .bind(("a", mi.user.id.to_string()))
        .await
        .and_then(|r| r.check());
    if let Err(e) = saved {
        eprintln!("[Announce] draft {}: {}", id, e);
    }
    let ru = mi.locale.starts_with("ru");
    let response = match load(data, id).await {
        Some(d) => serenity::CreateInteractionResponse::Message(preview(data, id, &d, ru)),
        None => serenity::CreateInteractionResponse::Message(
            serenity::CreateInteractionResponseMessage::new().content(if ru { "Черновик устарел — запусти /announce заново" } else { "Draft expired — run /announce again" }).ephemeral(true),
        ),
    };
    if let Err(e) = mi.create_response(&ctx.http, response).await {
        eprintln!("[Announce] preview: {}", e);
    }
}

/// Preview buttons: an:post / an:edit / an:cancel
pub async fn on_component(ctx: &serenity::Context, data: &Data, mci: &serenity::ComponentInteraction) {
    let mut parts = mci.data.custom_id.splitn(3, ':').skip(1);
    let (Some(action), Some(id)) = (parts.next(), parts.next()) else { return };
    let ru = mci.locale.starts_with("ru");
    let update = |text: String| {
        serenity::CreateInteractionResponse::UpdateMessage(serenity::CreateInteractionResponseMessage::new().content(text).components(vec![]))
    };
    let response = match action {
        "edit" => match load(data, id).await {
            Some(d) if d.author == mci.user.id.to_string() => {
                let langs = data.cfg.languages.clone();
                serenity::CreateInteractionResponse::Modal(form(id, &langs, &d.texts))
            }
            _ => update(if ru { "Черновик устарел — запусти /announce заново".into() } else { "Draft expired — run /announce again".into() }),
        },
        "cancel" => {
            let _ = data
                .db
                .db
                .query("UPDATE type::thing('announce_draft', $id) SET status = 'cancelled' WHERE author = $a")
                .bind(("id", id.to_string()))
                .bind(("a", mci.user.id.to_string()))
                .await;
            update(if ru { "Отменено.".into() } else { "Cancelled.".into() })
        }
        "post" => {
            let _ = mci.create_response(&ctx.http, update(if ru { "⏳ Публикую…".into() } else { "⏳ Posting…".into() })).await;
            let text = match post(ctx, data, id, mci.user.id).await {
                Ok(url) => if ru { format!("✅ Опубликовано: {}", url) } else { format!("✅ Posted: {}", url) },
                Err(e) => format!("⚠️ {}", e),
            };
            let _ = mci.edit_response(&ctx.http, serenity::EditInteractionResponse::new().content(text).components(vec![])).await;
            return;
        }
        _ => return,
    };
    if let Err(e) = mci.create_response(&ctx.http, response).await {
        eprintln!("[Announce] button {}: {}", action, e);
    }
}

async fn post(ctx: &serenity::Context, data: &Data, id: &str, user: serenity::UserId) -> Result<String, Error> {
    let cfg: &AnnounceCfg = data.cfg.announce.as_ref().ok_or("модуль announce выключен")?;
    // take the draft: author only, and only once
    let mut r = data
        .db
        .db
        .query("UPDATE type::thing('announce_draft', $id) SET status = 'posted' WHERE status = 'draft' AND author = $a RETURN BEFORE")
        .bind(("id", id.to_string()))
        .bind(("a", user.to_string()))
        .await?
        .check()?;
    let d: Option<Draft> = r.take::<Vec<Draft>>(0)?.into_iter().next();
    let d = d.ok_or("черновик уже опубликован, отменён или устарел")?;
    let channel = data.names.channel(&d.channel).ok_or_else(|| format!("канал '{}' не найден", d.channel))?;
    let mention = mention_of(data, &d.ping);
    let mentions = match d.ping.as_str() {
        "" => serenity::CreateAllowedMentions::new(),
        EVERYONE => serenity::CreateAllowedMentions::new().everyone(true),
        role => serenity::CreateAllowedMentions::new().roles(data.names.role(role).into_iter().collect::<Vec<_>>()),
    };
    let render = |t: &str| data.names.render(t, None, &data.server_name);
    let (main_post, main_card) = split_rules(&render(&d.texts[0]));
    if main_post.chars().count() > MAX_POST {
        return Err(format!("текст поста длиннее {} символов — сократи или вынеси правила после {}", MAX_POST, RULES_MARK).into());
    }
    let main_text = with_ping(&main_post, &mention);
    let mut msg = channel.send_message(&ctx.http, post_message(&main_text, main_card.as_ref()).allowed_mentions(mentions)).await?;

    let translations: Vec<(&AnnounceLang, &String)> = data.cfg.languages.iter().zip(d.texts.iter().skip(1)).filter(|(_, t)| !t.trim().is_empty()).collect();
    let ch_cfg = cfg.channel.iter().find(|c| c.name == d.channel);
    let thread_name = format!("🌍 {}", data.cfg.languages.iter().map(|l| format!("{} {}", l.flag, l.name)).collect::<Vec<_>>().join(" · "));
    let thread = channel
        .create_thread_from_message(&ctx.http, msg.id, serenity::CreateThread::new(thread_name).auto_archive_duration(serenity::AutoArchiveDuration::OneWeek))
        .await?;
    let mut links = Vec::new();
    let mut buttons = Vec::new();
    for (lang, text) in translations {
        let (tr_post, tr_card) = split_rules(&render(text));
        let tr = thread
            .send_message(&ctx.http, post_message(&tr_post, tr_card.as_ref()).allowed_mentions(serenity::CreateAllowedMentions::new()))
            .await?;
        links.push(format!("{} [{}](https://discord.com/channels/{}/{}/{})", lang.flag, lang.name, data.guild, thread.id, tr.id));
        if let Some(target) = ch_cfg.and_then(|c| c.mirror.get(&lang.code)) {
            match data.names.channel(target) {
                Some(mch) => {
                    let text = format!("{}\n{}", tr_post, original_line(&msg.link()));
                    if let Err(e) = mch.send_message(&ctx.http, post_message(&text, tr_card.as_ref()).allowed_mentions(serenity::CreateAllowedMentions::new())).await {
                        eprintln!("[Announce] copy {} → {}: {}", lang.code, target, e);
                    }
                }
                None => eprintln!("[Announce] copy {}: channel '{}' not found", lang.code, target),
            }
        }
        buttons.push(
            serenity::CreateButton::new(format!("tr:{}:{}", thread.id, tr.id))
                .label(&lang.name)
                .emoji(serenity::ReactionType::Unicode(lang.flag.clone()))
                .style(serenity::ButtonStyle::Secondary),
        );
    }
    if !links.is_empty() {
        let rows = buttons.chunks(5).map(|c| serenity::CreateActionRow::Buttons(c.to_vec())).collect();
        // an edit doesn't ping again
        msg.edit(
            &ctx.http,
            serenity::EditMessage::new()
                .content(format!("{}\n-# 🌍 **Translate:** {}", main_text, links.join(" · ")))
                .components(rows)
                .allowed_mentions(serenity::CreateAllowedMentions::new()),
        )
        .await?;
    }
    // announcement channel: crosspost to following servers
    let is_news = channel.to_channel(&ctx.http).await.ok().and_then(|c| c.guild()).is_some_and(|c| c.kind == serenity::ChannelType::News);
    if is_news && ch_cfg.map_or(true, |c| c.crosspost) {
        if let Err(e) = msg.crosspost(&ctx.http).await {
            eprintln!("[Announce] crosspost: {}", e);
        }
    }
    if !d.expire.is_empty() {
        if let Err(e) = crate::modules::expire::schedule(data, channel, msg.id, &d.expire, &user.to_string()).await {
            eprintln!("[Announce] delete after {}: {}", d.expire, e);
        }
    }
    println!("[Announce] {} → {} (ping: {})", user, d.channel, if d.ping.is_empty() { "—" } else { &d.ping });
    Ok(msg.link())
}

/// Last line of a copy: a link to the original; the copy is found by it when the original is deleted
pub fn original_line(link: &str) -> String {
    format!("-# 🇬🇧 [Original]({})", link)
}

/// The copy points to this original (`…/<channel>/<message>)` in its original_line)
pub fn is_copy_of(content: &str, channel: serenity::ChannelId, message: serenity::MessageId) -> bool {
    content.lines().last().is_some_and(|l| l.starts_with("-# 🇬🇧 [Original](") && l.ends_with(&format!("/{}/{})", channel, message)))
}

/// A post was deleted in a channel with copies → delete the copies too (searching the latest messages of the copy channel)
pub async fn on_delete(ctx: &serenity::Context, data: &Data, channel: serenity::ChannelId, message: serenity::MessageId) {
    let Some(cfg) = data.cfg.announce.as_ref() else { return };
    let Some(ch_cfg) = cfg.channel.iter().find(|c| !c.mirror.is_empty() && data.names.channel(&c.name) == Some(channel)) else { return };
    let me = ctx.cache.current_user().id;
    for target in ch_cfg.mirror.values() {
        let Some(mch) = data.names.channel(target) else { continue };
        let Ok(recent) = mch.messages(&ctx.http, serenity::GetMessages::new().limit(50)).await else { continue };
        for m in recent.iter().filter(|m| (m.author.id == me || m.webhook_id.is_some()) && is_copy_of(&m.content, channel, message)) {
            match m.delete(&ctx.http).await {
                Ok(()) => println!("[Announce] original {} deleted → deleted copy in {}", message, target),
                Err(e) => eprintln!("[Announce] copy in {}: {}", target, e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::with_ping;

    #[test]
    fn copy_points_to_its_original() {
        use poise::serenity_prelude::{ChannelId, MessageId};
        let link = "https://discord.com/channels/1/22/333";
        let copy = format!("# Ивент\nтекст\n{}", super::original_line(link));
        assert!(super::is_copy_of(&copy, ChannelId::new(22), MessageId::new(333)));
        assert!(!super::is_copy_of(&copy, ChannelId::new(22), MessageId::new(33)));
        assert!(!super::is_copy_of("# Ивент\nтекст", ChannelId::new(22), MessageId::new(333)));
    }

    #[test]
    fn rules_after_the_mark_become_a_card() {
        use super::split_rules;
        let (post, card) = split_rules("# Event\ntext\n---rules---\n### 📜 Rules\n1. One\n2. Two\n");
        assert_eq!(post, "# Event\ntext");
        assert_eq!(card, Some((Some("📜 Rules".into()), "1. One\n2. Two".into())));
        assert_eq!(split_rules("just text\n"), ("just text".into(), None));
        assert_eq!(split_rules("t\n---rules---\nno title").1, Some((None, "no title".into())));
    }

    #[test]
    fn ping_goes_to_the_last_small_line() {
        assert_eq!(with_ping("# T\ntext\n-# Discuss → #chat", "<@&1>"), "# T\ntext\n-# Discuss → #chat · <@&1>");
        assert_eq!(with_ping("# T\ntext", "<@&1>"), "# T\ntext\n-# <@&1>");
        assert_eq!(with_ping("# T\ntext\n", ""), "# T\ntext");
    }
}
