//! "Images only" channels (🖼️・media): the bot deletes messages without an image or GIF and briefly explains why
//! (the hint disappears after a few seconds). A caption on an image is fine. Threads under images aren't checked:
//! people discuss there. Moderators (exempt_roles and the Manage Messages permission) aren't checked.

use crate::Data;
use poise::serenity_prelude as serenity;
use std::time::Duration;

const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp"];
const GIF_HOSTS: &[&str] = &["tenor.com/", "giphy.com/", "media.discordapp.net/", "cdn.discordapp.com/", "imgur.com/"];

fn is_image_name(name: &str) -> bool {
    let name = name.split(['?', '#']).next().unwrap_or(name).to_lowercase();
    name.rsplit_once('.').is_some_and(|(_, ext)| IMAGE_EXT.contains(&ext))
}

/// A link that Discord shows as an image or GIF: a direct link to an image or a GIF service
fn is_image_link(word: &str) -> bool {
    let w = word.trim_matches(|c| c == '<' || c == '>');
    if !(w.starts_with("https://") || w.starts_with("http://")) {
        return false;
    }
    is_image_name(w) || GIF_HOSTS.iter().any(|h| w.contains(h) && (w.contains("tenor.com/") || w.contains("giphy.com/") || is_image_name(w)))
}

/// The message has an image or GIF: an image attachment, a sticker, an image in a link preview, or a link to an image/GIF
pub fn has_image(msg: &serenity::Message) -> bool {
    msg.attachments.iter().any(|a| a.content_type.as_deref().is_some_and(|t| t.starts_with("image/")) || is_image_name(&a.filename))
        || !msg.sticker_items.is_empty()
        || msg.embeds.iter().any(|e| e.image.is_some() || e.thumbnail.is_some() && matches!(e.kind.as_deref(), Some("image" | "gifv")))
        || msg.content.split_whitespace().any(is_image_link)
}

fn exempt(data: &Data, msg: &serenity::Message) -> bool {
    let Some(cfg) = &data.cfg.media_only else { return true };
    let Some(member) = &msg.member else { return false };
    member.permissions.is_some_and(|p| p.manage_messages() || p.administrator())
        || cfg.exempt_roles.iter().filter_map(|r| data.names.role(r)).any(|r| member.roles.contains(&r))
}

pub async fn on_message(ctx: &serenity::Context, data: &Data, msg: &serenity::Message) {
    let Some(cfg) = &data.cfg.media_only else { return };
    if msg.author.bot || !cfg.channels.iter().any(|c| data.names.channel(c) == Some(msg.channel_id)) {
        return;
    }
    if has_image(msg) || exempt(data, msg) {
        return;
    }
    if let Err(e) = msg.delete(&ctx.http).await {
        eprintln!("[MediaOnly] deleting in {}: {}", msg.channel_id, e);
        return;
    }
    println!("[MediaOnly] deleted a message from {} without an image", msg.author.name);
    let text = data.names.render(&cfg.notice, Some(msg.author.id), &data.server_name);
    let notice = msg
        .channel_id
        .send_message(&ctx.http, serenity::CreateMessage::new().content(text).allowed_mentions(serenity::CreateAllowedMentions::new().users(vec![msg.author.id])))
        .await;
    if let Ok(n) = notice {
        let http = ctx.http.clone();
        let secs = cfg.notice_secs;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(secs)).await;
            let _ = n.delete(&http).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{is_image_link, is_image_name};

    #[test]
    fn images_and_gif_links() {
        assert!(is_image_name("Screenshot 2026.PNG"));
        assert!(is_image_name("a.gif?ex=1"));
        assert!(!is_image_name("clip.mp4"));
        assert!(is_image_link("https://tenor.com/view/dungeon-gif-123"));
        assert!(is_image_link("<https://media.discordapp.net/attachments/1/2/x.webp?w=3>"));
        assert!(!is_image_link("https://youtube.com/watch?v=1"));
        assert!(!is_image_link("https://cdn.discordapp.com/attachments/1/2/setup.exe"));
        assert!(!is_image_link("hello"));
    }
}
