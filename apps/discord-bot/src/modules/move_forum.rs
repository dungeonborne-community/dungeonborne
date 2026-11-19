//! "Move to support" in the message menu (right-click → Apps): a question from chat becomes a post in the help forum.
//! A helper or moderator names the post → the bot creates a post with a quote and attachments, sets the language tag
//! (by the channel it was moved from), invites the author into the post and replies in chat with a link to it.
//! The original message stays, so the conversation around it isn't broken.

use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;

/// Helpers and moderators: roles from [move_to_forum] roles or the Manage Messages permission
fn allowed(data: &Data, member: Option<&serenity::Member>) -> bool {
    let Some(cfg) = &data.cfg.move_to_forum else { return false };
    let Some(m) = member else { return false };
    if m.permissions.is_some_and(|p| p.administrator() || p.manage_messages()) {
        return true;
    }
    cfg.roles.iter().filter_map(|r| data.names.role(r)).any(|r| m.roles.contains(&r))
}

/// Перенести сообщение в форум помощи
#[poise::command(context_menu_command = "Move to support", guild_only)]
pub async fn move_to_help(ctx: Context<'_>, msg: serenity::Message) -> Result<(), Error> {
    let data = ctx.data();
    let ru = ctx.locale().is_some_and(|l| l.starts_with("ru"));
    let poise::Context::Application(actx) = ctx else { return Ok(()) };
    if !allowed(data, actx.interaction.member.as_deref()) {
        ctx.send(poise::CreateReply::default().content(if ru { "Переносить могут хелперы и модераторы" } else { "Only helpers and moderators can move messages" }).ephemeral(true))
            .await?;
        return Ok(());
    }
    if msg.author.bot {
        ctx.send(poise::CreateReply::default().content("🤖 ✖").ephemeral(true)).await?;
        return Ok(());
    }
    // the default post title is the beginning of the question
    let suggested: String = msg.content.lines().next().unwrap_or("").chars().take(90).collect();
    let modal = serenity::CreateModal::new(format!("mv:form:{}:{}", msg.channel_id, msg.id), "Move to support / В форум помощи").components(vec![
        serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(serenity::InputTextStyle::Short, "Post title / Название поста", "title")
                .max_length(100)
                .required(true)
                .value(if suggested.trim().is_empty() { "Question".to_string() } else { suggested }),
        ),
    ]);
    actx.interaction.create_response(ctx.http(), serenity::CreateInteractionResponse::Modal(modal)).await?;
    actx.has_sent_initial_response.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
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

/// Post text: where and whose question, the quote, attachments. At most 2000 characters.
pub fn post_text(author: serenity::UserId, channel: serenity::ChannelId, content: &str, files: &[String]) -> String {
    let mut quote: String = content.lines().map(|l| format!("> {}\n", l)).collect();
    if quote.is_empty() {
        quote = "> …\n".into();
    }
    let head = format!("📥 <@{}> · <#{}>\n", author, channel);
    let tail: String = files.iter().map(|f| format!("{}\n", f)).collect();
    let room = 2000usize.saturating_sub(head.chars().count() + tail.chars().count() + 2);
    if quote.chars().count() > room {
        quote = quote.chars().take(room.saturating_sub(2)).collect::<String>() + "…\n";
    }
    format!("{}{}{}", head, quote, tail)
}

pub async fn on_modal(ctx: &serenity::Context, data: &Data, mi: &serenity::ModalInteraction) {
    let Some(rest) = mi.data.custom_id.strip_prefix("mv:form:") else { return };
    let defer = serenity::CreateInteractionResponse::Defer(serenity::CreateInteractionResponseMessage::new().ephemeral(true));
    if mi.create_response(&ctx.http, defer).await.is_err() {
        return;
    }
    let text = match move_it(ctx, data, mi, rest).await {
        Ok(thread) => format!("✅ <#{}>", thread),
        Err(e) => {
            eprintln!("[MoveToHelp] {}: {}", mi.user.name, e);
            format!("⚠️ {}", e)
        }
    };
    let _ = mi.edit_response(&ctx.http, serenity::EditInteractionResponse::new().content(text)).await;
}

async fn move_it(ctx: &serenity::Context, data: &Data, mi: &serenity::ModalInteraction, rest: &str) -> Result<serenity::ChannelId, Error> {
    let cfg = data.cfg.move_to_forum.as_ref().ok_or("модуль выключен")?;
    if !allowed(data, mi.member.as_ref()) {
        return Err("нет прав".into());
    }
    let (ch, id) = rest.split_once(':').ok_or("битая форма")?;
    let channel = serenity::ChannelId::new(ch.parse()?);
    let msg = channel.message(&ctx.http, serenity::MessageId::new(id.parse()?)).await?;
    let forum = data.names.channel(&cfg.forum).ok_or_else(|| format!("форум '{}' не найден", cfg.forum))?;

    // language tag: by the channel it was moved from
    let tag_name = cfg
        .lang_tags
        .iter()
        .find(|t| t.channels.iter().any(|c| data.names.channel(c) == Some(channel)))
        .map(|t| t.tag.as_str())
        .unwrap_or(cfg.default_tag.as_str());
    let tags: Vec<serenity::ForumTagId> = forum
        .to_channel(&ctx.http)
        .await?
        .guild()
        .map(|g| g.available_tags.into_iter().filter(|t| t.name == tag_name).map(|t| t.id).collect())
        .unwrap_or_default();

    let files: Vec<String> = msg.attachments.iter().map(|a| a.url.clone()).collect();
    let title: String = input(mi, "title").trim().chars().take(100).collect();
    let body = post_text(msg.author.id, channel, &msg.content, &files);
    let mut post = serenity::CreateForumPost::new(
        if title.is_empty() { "Question".to_string() } else { title },
        serenity::CreateMessage::new().content(body).allowed_mentions(serenity::CreateAllowedMentions::new().users(vec![msg.author.id])),
    );
    for t in tags {
        post = post.add_applied_tag(t);
    }
    let thread = forum.create_forum_post(&ctx.http, post).await?;

    let note = format!("📥 <@{}> → <#{}> — answers there / ответы там", msg.author.id, thread.id);
    let _ = channel
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new()
                .content(note)
                .reference_message(&msg)
                .allowed_mentions(serenity::CreateAllowedMentions::new().users(vec![msg.author.id])),
        )
        .await;
    println!("[MoveToHelp] {} moved a message from {} to {}", mi.user.name, msg.author.name, thread.id);
    Ok(thread.id)
}

#[cfg(test)]
mod tests {
    use super::post_text;
    use poise::serenity_prelude as serenity;

    #[test]
    fn quotes_and_fits_discord_limit() {
        let t = post_text(serenity::UserId::new(1), serenity::ChannelId::new(2), "line one\nline two", &["https://cdn/x.png".into()]);
        assert_eq!(t, "📥 <@1> · <#2>\n> line one\n> line two\nhttps://cdn/x.png\n");
        let long = "a".repeat(5000);
        assert!(post_text(serenity::UserId::new(1), serenity::ChannelId::new(2), &long, &[]).chars().count() <= 2000);
    }
}
