//! `/embed`: moderators post a nice-looking message to the current channel via a form (title, text, color).
//! The text supports substitutions: {#channel}, {@role}, {server}.

use crate::{Data, Error};
use poise::serenity_prelude as serenity;
use poise::Modal as _;

#[derive(poise::Modal)]
#[name = "Embed"]
struct EmbedForm {
    #[name = "Title / Заголовок"]
    #[max_length = 256]
    title: Option<String>,
    #[name = "Text / Текст"]
    #[paragraph]
    #[max_length = 4000]
    text: String,
    #[name = "Color, hex (5865F2)"]
    #[max_length = 7]
    color: Option<String>,
}

/// Опубликовать embed-сообщение в этот канал
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_MESSAGES")]
pub async fn embed(ctx: poise::ApplicationContext<'_, Data, Error>) -> Result<(), Error> {
    let Some(form) = EmbedForm::execute(ctx).await? else { return Ok(()) };
    let data = ctx.data();
    let color = form
        .color
        .as_deref()
        .and_then(|c| u32::from_str_radix(c.trim().trim_start_matches('#'), 16).ok())
        .unwrap_or(0x5865F2);
    let mut embed = serenity::CreateEmbed::new()
        .description(data.names.render(&form.text, None, &data.server_name))
        .color(color);
    if let Some(title) = form.title.filter(|t| !t.trim().is_empty()) {
        embed = embed.title(title);
    }
    ctx.channel_id().send_message(ctx.http(), serenity::CreateMessage::new().embed(embed)).await?;
    ctx.send(poise::CreateReply::default().content("✅").ephemeral(true)).await?;
    Ok(())
}
