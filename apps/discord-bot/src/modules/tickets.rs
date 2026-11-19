//! Tickets: `/ticketpanel` posts a panel with a button per language → "subject and details" form → private thread
//! in the tickets channel. The author is added to the thread; mentioning that language's moderator role adds them.
//! The author or a moderator (Manage Threads) can close it: the conversation is saved in the database and sent as a file to the
//! transcripts channel, the thread is archived and locked.
//! Database: the ticket number is an atomic counter (`UPSERT counter:ticket SET n += 1`), one open ticket per person,
//! opening and closing write a mod-log event on the `ticket` table.

use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;
use serde::Deserialize;

pub const SCHEMA: &str = r##"
DEFINE TABLE OVERWRITE counter SCHEMALESS;
DEFINE TABLE OVERWRITE ticket SCHEMAFULL;
DEFINE FIELD OVERWRITE num ON ticket TYPE int;
DEFINE FIELD OVERWRITE user ON ticket TYPE record<member>;
DEFINE FIELD OVERWRITE lang ON ticket TYPE string;
DEFINE FIELD OVERWRITE topic ON ticket TYPE string;
DEFINE FIELD OVERWRITE status ON ticket TYPE string DEFAULT "open" ASSERT $value INSIDE ["open", "closed"];
DEFINE FIELD OVERWRITE opened_at ON ticket TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD OVERWRITE closed_at ON ticket TYPE option<datetime>;
DEFINE FIELD OVERWRITE closed_by ON ticket TYPE option<string>;
DEFINE FIELD OVERWRITE transcript ON ticket TYPE option<string>;
DEFINE INDEX OVERWRITE idx_ticket_user_status ON ticket COLUMNS user, status;
DEFINE EVENT OVERWRITE ticket_log ON ticket
    WHEN $event = "CREATE" OR ($event = "UPDATE" AND $before.status != $after.status)
    THEN {
        CREATE mod_log CONTENT {
            kind: IF $event = "CREATE" THEN "ticket_open" ELSE "ticket_close" END,
            user_id: record::id($after.user),
            channel_id: record::id($after.id),
            text: "#" + <string>$after.num + " · " + $after.topic
                + (IF $after.closed_by != NONE THEN " · closed by <@" + $after.closed_by + ">" ELSE "" END),
        };
    };
"##;

#[derive(Deserialize, Debug)]
pub struct Ticket {
    pub user: String,
    pub num: i64,
    pub topic: String,
}

/// The person's open ticket (thread ID), if any
pub async fn open_ticket_of(data: &Data, user: serenity::UserId) -> Option<String> {
    let mut r = data
        .db
        .db
        .query("SELECT VALUE record::id(id) FROM ticket WHERE user = type::thing('member', $u) AND status = 'open' LIMIT 1")
        .bind(("u", user.to_string()))
        .await
        .ok()?;
    r.take::<Vec<String>>(0).ok()?.into_iter().next()
}

pub async fn next_number(data: &Data) -> Result<i64, Error> {
    let n: Vec<i64> = data.db.db.query("UPSERT counter:ticket SET n += 1 RETURN VALUE n").await?.take(0)?;
    n.into_iter().next().ok_or_else(|| "счётчик тикетов не вернул номер".into())
}

pub async fn save(data: &Data, thread: serenity::ChannelId, num: i64, user: serenity::UserId, lang: &str, topic: &str) -> Result<(), Error> {
    data.db
        .db
        .query("CREATE type::thing('ticket', $t) SET num = $n, user = type::thing('member', $u), lang = $l, topic = $topic")
        .bind(("t", thread.to_string()))
        .bind(("n", num))
        .bind(("u", user.to_string()))
        .bind(("l", lang.to_string()))
        .bind(("topic", topic.to_string()))
        .await?
        .check()?;
    Ok(())
}

/// Close if still open: UPDATE … WHERE status = 'open' fires exactly once
pub async fn close(data: &Data, thread: serenity::ChannelId, by: serenity::UserId) -> Result<Option<Ticket>, Error> {
    let closed: Vec<Ticket> = data
        .db
        .db
        .query(
            "UPDATE type::thing('ticket', $t) SET status = 'closed', closed_at = time::now(), closed_by = $by
             WHERE status = 'open' RETURN record::id(user) AS user, num, topic",
        )
        .bind(("t", thread.to_string()))
        .bind(("by", by.to_string()))
        .await?
        .take(0)?;
    Ok(closed.into_iter().next())
}

async fn ticket_of_thread(data: &Data, thread: serenity::ChannelId) -> Option<Ticket> {
    let mut r = data
        .db
        .db
        .query("SELECT record::id(user) AS user, num, topic FROM ONLY type::thing('ticket', $t)")
        .bind(("t", thread.to_string()))
        .await
        .ok()?;
    r.take::<Option<Ticket>>(0).ok()?
}

/// Опубликовать панель тикетов
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_GUILD")]
pub async fn ticketpanel(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let cfg = data.cfg.tickets.as_ref().ok_or("тикеты выключены в bot.toml")?;
    let channel = data.names.channel(&cfg.channel).ok_or_else(|| format!("канал '{}' не найден", cfg.channel))?;
    let buttons = cfg
        .button
        .iter()
        .map(|b| {
            let mut button = serenity::CreateButton::new(format!("tk:open:{}", b.lang)).label(&b.label).style(serenity::ButtonStyle::Primary);
            if let Some(e) = &b.emoji {
                button = button.emoji(serenity::ReactionType::Unicode(e.clone()));
            }
            button
        })
        .collect();
    let embed = serenity::CreateEmbed::new()
        .title("🎫 Support / Поддержка")
        .description(
            "Private help from moderators: account problems, bans, reports, creator and helper applications.\n\
             Приватная помощь модераторов: аккаунт, разбан, жалобы, заявки автора и хелпера.\n\n\
             ⚠️ Never send your password / Никогда не отправляй пароль.",
        )
        .color(0x5865F2);
    channel
        .send_message(ctx.http(), serenity::CreateMessage::new().embed(embed).components(vec![serenity::CreateActionRow::Buttons(buttons)]))
        .await?;
    ctx.send(poise::CreateReply::default().content(format!("✅ <#{}>", channel)).ephemeral(true)).await?;
    Ok(())
}

fn ephemeral(text: impl Into<String>) -> serenity::CreateInteractionResponse {
    serenity::CreateInteractionResponse::Message(serenity::CreateInteractionResponseMessage::new().content(text).ephemeral(true))
}

/// Buttons tk:open:<lang> and tk:close
pub async fn on_component(ctx: &serenity::Context, data: &Data, mci: &serenity::ComponentInteraction) {
    let Some(rest) = mci.data.custom_id.strip_prefix("tk:") else { return };
    let ru = mci.locale.starts_with("ru");
    let response = if let Some(lang) = rest.strip_prefix("open:") {
        if let Some(thread) = open_ticket_of(data, mci.user.id).await {
            ephemeral(if ru { format!("У тебя уже есть открытый тикет: <#{}>", thread) } else { format!("You already have an open ticket: <#{}>", thread) })
        } else {
            let (title, topic, details) = if lang == "ru" {
                ("Тикет", "Тема", "Подробности")
            } else {
                ("Ticket", "Topic", "Details")
            };
            let button = data.cfg.tickets.as_ref().and_then(|c| c.button.iter().find(|b| b.lang == lang));
            let title = button.and_then(|b| b.form_title.as_deref()).unwrap_or(title);
            let topic = button.and_then(|b| b.topic_label.as_deref()).unwrap_or(topic);
            let details = button.and_then(|b| b.details_label.as_deref()).unwrap_or(details);
            let rows = match button.filter(|b| !b.fields.is_empty()) {
                // custom form: fields f0..f4
                Some(b) => b
                    .fields
                    .iter()
                    .take(5)
                    .enumerate()
                    .map(|(i, f)| {
                        let style = if f.long { serenity::InputTextStyle::Paragraph } else { serenity::InputTextStyle::Short };
                        let mut input = serenity::CreateInputText::new(style, &f.label, format!("f{}", i))
                            .required(f.required)
                            .max_length(f.max.unwrap_or(if f.long { 1000 } else { 100 }));
                        if let Some(p) = &f.placeholder {
                            input = input.placeholder(p);
                        }
                        serenity::CreateActionRow::InputText(input)
                    })
                    .collect(),
                None => vec![
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(serenity::InputTextStyle::Short, topic, "topic").max_length(80).required(true),
                    ),
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(serenity::InputTextStyle::Paragraph, details, "details").max_length(1500).required(false),
                    ),
                ],
            };
            let modal = serenity::CreateModal::new(format!("tk:form:{}", lang), title).components(rows);
            serenity::CreateInteractionResponse::Modal(modal)
        }
    } else if rest == "close" {
        let thread = mci.channel_id;
        let is_mod = mci.member.as_ref().and_then(|m| m.permissions).is_some_and(|p| p.manage_threads());
        match ticket_of_thread(data, thread).await {
            Some(t) if t.user == mci.user.id.to_string() || is_mod => {
                let _ = mci.create_response(&ctx.http, ephemeral("🔒 …")).await;
                finish(ctx, data, thread, mci.user.id).await;
                return;
            }
            Some(_) => ephemeral(if ru { "Закрыть может автор или модератор" } else { "Only the author or a moderator can close it" }),
            None => ephemeral("⚠️"),
        }
    } else {
        return;
    };
    if let Err(e) = mci.create_response(&ctx.http, response).await {
        eprintln!("[Tickets] button response: {}", e);
    }
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

/// Form submitted: create the thread, add the author, call the language's moderators
pub async fn on_modal(ctx: &serenity::Context, data: &Data, mi: &serenity::ModalInteraction) {
    let Some(lang) = mi.data.custom_id.strip_prefix("tk:form:") else { return };
    let defer = serenity::CreateInteractionResponse::Defer(serenity::CreateInteractionResponseMessage::new().ephemeral(true));
    if mi.create_response(&ctx.http, defer).await.is_err() {
        return;
    }
    let text = match open(ctx, data, mi, lang).await {
        Ok(thread) => format!("✅ <#{}>", thread),
        Err(e) => {
            eprintln!("[Tickets] opening for {}: {}", mi.user.name, e);
            format!("⚠️ {}", e)
        }
    };
    let _ = mi.edit_response(&ctx.http, serenity::EditInteractionResponse::new().content(text)).await;
}

async fn open(ctx: &serenity::Context, data: &Data, mi: &serenity::ModalInteraction, lang: &str) -> Result<serenity::ChannelId, Error> {
    let cfg = data.cfg.tickets.as_ref().ok_or("тикеты выключены")?;
    let button = cfg.button.iter().find(|b| b.lang == lang).ok_or("неизвестный язык тикета")?;
    if let Some(thread) = open_ticket_of(data, mi.user.id).await {
        return Ok(serenity::ChannelId::new(thread.parse()?));
    }
    let channel = data.names.channel(&cfg.channel).ok_or("канал тикетов не найден")?;
    let staff: Vec<serenity::RoleId> = button.staff.list().into_iter().filter_map(|r| data.names.role(r)).collect();
    // custom form: subject = form title, answers = card fields
    let answers: Vec<(String, String)> =
        button.fields.iter().take(5).enumerate().map(|(i, f)| (f.label.clone(), input(mi, &format!("f{}", i)))).collect();
    let topic = if button.fields.is_empty() {
        input(mi, "topic")
    } else {
        button.form_title.clone().unwrap_or_else(|| button.label.clone())
    };
    let details = input(mi, "details");
    let num = next_number(data).await?;

    let name: String = format!("🎫 {:04} · {}", num, mi.user.name).chars().take(100).collect();
    let thread = channel
        .create_thread(
            &ctx.http,
            serenity::CreateThread::new(name)
                .kind(serenity::ChannelType::PrivateThread)
                .invitable(false)
                .auto_archive_duration(serenity::AutoArchiveDuration::OneWeek),
        )
        .await?;
    save(data, thread.id, num, mi.user.id, lang, &topic).await?;
    thread.id.add_thread_member(&ctx.http, mi.user.id).await?;

    let mut embed = serenity::CreateEmbed::new()
        .title(format!("🎫 #{:04} · {}", num, topic))
        .color(0x5865F2)
        .footer(serenity::CreateEmbedFooter::new(if lang == "ru" {
            "Модератор скоро ответит. Закрыть тикет — кнопкой ниже."
        } else {
            "A moderator will reply soon. Close the ticket with the button below."
        }));
    if !details.trim().is_empty() {
        embed = embed.description(details);
    }
    for (label, value) in &answers {
        let value = if value.trim().is_empty() { "—".to_string() } else { value.chars().take(1024).collect() };
        embed = embed.field(label, value, false);
    }
    let mention: String = staff.iter().map(|r| format!(" <@&{}>", r)).collect();
    let mentions = serenity::CreateAllowedMentions::new().users(vec![mi.user.id]).roles(staff.clone());
    let close = serenity::CreateButton::new("tk:close").label("Close / Закрыть").emoji('🔒').style(serenity::ButtonStyle::Danger);
    thread
        .id
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new()
                .content(format!("<@{}>{}", mi.user.id, mention))
                .embed(embed)
                .components(vec![serenity::CreateActionRow::Buttons(vec![close])])
                .allowed_mentions(mentions),
        )
        .await?;
    println!("[Tickets] #{:04} {} ({}): {}", num, mi.user.name, lang, topic);
    Ok(thread.id)
}

/// The whole thread conversation, from oldest to newest
async fn transcript(ctx: &serenity::Context, thread: serenity::ChannelId) -> String {
    let mut all = Vec::new();
    let mut before: Option<serenity::MessageId> = None;
    for _ in 0..20 {
        let mut req = serenity::GetMessages::new().limit(100);
        if let Some(b) = before {
            req = req.before(b);
        }
        let Ok(page) = thread.messages(&ctx.http, req).await else { break };
        if page.is_empty() {
            break;
        }
        before = page.last().map(|m| m.id);
        all.extend(page);
    }
    all.reverse();
    all.iter()
        .map(|m| {
            let when = m.timestamp.to_string().chars().take(16).collect::<String>().replace('T', " ");
            let mut line = format!("[{}] {}: {}", when, m.author.name, m.content);
            for e in &m.embeds {
                line.push_str(&format!(" [embed: {}]", e.title.clone().unwrap_or_default()));
            }
            for a in &m.attachments {
                line.push_str(&format!(" [file: {}]", a.url));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn finish(ctx: &serenity::Context, data: &Data, thread: serenity::ChannelId, by: serenity::UserId) {
    let ticket = match close(data, thread, by).await {
        Ok(Some(t)) => t,
        Ok(None) => return, // already closed
        Err(e) => {
            eprintln!("[Tickets] closing {}: {}", thread, e);
            return;
        }
    };
    let text = transcript(ctx, thread).await;
    let _ = data
        .db
        .db
        .query("UPDATE type::thing('ticket', $t) SET transcript = $x")
        .bind(("t", thread.to_string()))
        .bind(("x", text.clone()))
        .await;
    if let Some(ch) = data.cfg.tickets.as_ref().and_then(|c| data.names.channel(&c.transcripts)) {
        let file = serenity::CreateAttachment::bytes(text.into_bytes(), format!("ticket-{:04}.txt", ticket.num));
        let msg = serenity::CreateMessage::new()
            .content(format!("🎫 #{:04} · {} · <@{}>", ticket.num, ticket.topic, ticket.user))
            .add_file(file)
            .allowed_mentions(serenity::CreateAllowedMentions::new());
        if let Err(e) = ch.send_message(&ctx.http, msg).await {
            eprintln!("[Tickets] transcript #{}: {}", ticket.num, e);
        }
    }
    let _ = thread
        .send_message(&ctx.http, serenity::CreateMessage::new().content(format!("🔒 Closed / Закрыт · <@{}>", by)))
        .await;
    if let Err(e) = thread.edit_thread(&ctx.http, serenity::EditThread::new().archived(true).locked(true)).await {
        eprintln!("[Tickets] archiving thread #{}: {}", ticket.num, e);
    }
    println!("[Tickets] #{:04} closed", ticket.num);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::testing;

    /// Counter, one open ticket per person, close-once, event → mod_log
    #[tokio::test]
    async fn ticket_lifecycle() {
        let (data, dir) = testing::data("tickets", "").await;
        let user = serenity::UserId::new(5);
        let thread = serenity::ChannelId::new(900);
        assert_eq!((next_number(&data).await.unwrap(), next_number(&data).await.unwrap()), (1, 2));
        assert!(open_ticket_of(&data, user).await.is_none());

        save(&data, thread, 2, user, "ru", "Не могу войти").await.unwrap();
        assert_eq!(open_ticket_of(&data, user).await.as_deref(), Some("900"));
        let t = ticket_of_thread(&data, thread).await.unwrap();
        assert_eq!((t.user.as_str(), t.num, t.topic.as_str()), ("5", 2, "Не могу войти"));

        let closed = close(&data, thread, serenity::UserId::new(77)).await.unwrap().unwrap();
        assert_eq!(closed.num, 2);
        assert!(close(&data, thread, serenity::UserId::new(77)).await.unwrap().is_none(), "a second close does nothing");
        assert!(open_ticket_of(&data, user).await.is_none());

        #[derive(Deserialize)]
        struct Log {
            line: String,
        }
        let logs: Vec<Log> =
            data.db.db.query("SELECT kind + ': ' + text AS line, at FROM mod_log ORDER BY at").await.unwrap().take(0).unwrap();
        let logs: Vec<String> = logs.into_iter().map(|l| l.line).collect();
        assert_eq!(logs, vec!["ticket_open: #2 · Не могу войти", "ticket_close: #2 · Не могу войти · closed by <@77>"]);
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
