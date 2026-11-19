//! Starboard: a message reaches `threshold` ⭐ reactions (the author's own star excluded) → copy in the best-of channel,
//! after that the counter on the copy is kept updated. Record `star:<message id>` stores the count and the copy ID;
//! the first publication is claimed by `UPDATE … WHERE board_message = NONE` so two quick stars don't create two copies.

use crate::Data;
use poise::serenity_prelude as serenity;
use serde::Deserialize;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE star SCHEMAFULL;
DEFINE FIELD OVERWRITE channel ON star TYPE string;
DEFINE FIELD OVERWRITE author ON star TYPE record<member>;
DEFINE FIELD OVERWRITE stars ON star TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE board_message ON star TYPE option<string>;
"#;

const PENDING: &str = "pending";

#[derive(Deserialize, Debug)]
pub struct Star {
    pub stars: i64,
    pub board_message: Option<String>,
}

pub async fn set_count(data: &Data, message: &str, channel: &str, author: &str, count: i64) -> Option<Star> {
    let r = data
        .db
        .db
        .query("UPSERT type::thing('star', $m) SET channel = $c, author = type::thing('member', $a), stars = $n RETURN stars, board_message")
        .bind(("m", message.to_string()))
        .bind(("c", channel.to_string()))
        .bind(("a", author.to_string()))
        .bind(("n", count))
        .await;
    match r {
        Ok(mut r) => r.take::<Vec<Star>>(0).ok()?.into_iter().next(),
        Err(e) => {
            eprintln!("[Starboard] {}: {}", message, e);
            None
        }
    }
}

/// true — this task publishes the copy (nobody started before it)
pub async fn claim(data: &Data, message: &str) -> bool {
    let r = data
        .db
        .db
        .query("UPDATE type::thing('star', $m) SET board_message = $p WHERE board_message = NONE RETURN VALUE board_message")
        .bind(("m", message.to_string()))
        .bind(("p", PENDING))
        .await;
    r.ok().and_then(|mut r| r.take::<Vec<String>>(0).ok()).is_some_and(|v| !v.is_empty())
}

async fn set_board(data: &Data, message: &str, board: Option<String>) {
    let _ = data
        .db
        .db
        .query("UPDATE type::thing('star', $m) SET board_message = $b")
        .bind(("m", message.to_string()))
        .bind(("b", board))
        .await;
}

fn header(emoji: &str, count: i64, channel: serenity::ChannelId) -> String {
    format!("{} **{}** · <#{}>", emoji, count, channel)
}

pub async fn on_reaction(ctx: &serenity::Context, data: &Data, reaction: &serenity::Reaction) {
    let Some(cfg) = &data.cfg.starboard else { return };
    let serenity::ReactionType::Unicode(emoji) = &reaction.emoji else { return };
    if emoji.trim_end_matches('\u{fe0f}') != cfg.emoji.trim_end_matches('\u{fe0f}') {
        return;
    }
    let Some(board) = data.names.channel(&cfg.channel) else { return };
    let ignored = cfg.ignore.iter().filter_map(|c| data.names.channel(c)).any(|c| c == reaction.channel_id);
    if reaction.channel_id == board || ignored {
        return;
    }
    let Ok(msg) = reaction.channel_id.message(&ctx.http, reaction.message_id).await else { return };
    if msg.author.bot {
        return;
    }
    // the author's star on their own message doesn't count
    let users = reaction
        .channel_id
        .reaction_users(&ctx.http, msg.id, reaction.emoji.clone(), Some(100), None)
        .await
        .unwrap_or_default();
    let count = users.iter().filter(|u| u.id != msg.author.id && !u.bot).count() as i64;

    let id = msg.id.to_string();
    let Some(star) = set_count(data, &id, &msg.channel_id.to_string(), &msg.author.id.to_string(), count).await else { return };
    let text = header(&cfg.emoji, star.stars, msg.channel_id);
    match star.board_message.as_deref() {
        Some(PENDING) => {}
        Some(board_id) => {
            if let Ok(bid) = board_id.parse::<u64>() {
                let _ = board.edit_message(&ctx.http, serenity::MessageId::new(bid), serenity::EditMessage::new().content(text)).await;
            }
        }
        None if count >= cfg.threshold as i64 && claim(data, &id).await => {
            let mut embed = serenity::CreateEmbed::new()
                .author(serenity::CreateEmbedAuthor::new(&msg.author.name).icon_url(msg.author.face()))
                .description(msg.content.chars().take(4000).collect::<String>())
                .field("Original / Оригинал", format!("[→]({})", msg.link()), false)
                .timestamp(msg.timestamp)
                .color(0xF1C40F);
            if let Some(img) = msg.attachments.iter().find(|a| a.content_type.as_deref().is_some_and(|t| t.starts_with("image/"))) {
                embed = embed.image(&img.url);
            }
            let post = serenity::CreateMessage::new().content(text).embed(embed).allowed_mentions(serenity::CreateAllowedMentions::new());
            match board.send_message(&ctx.http, post).await {
                Ok(m) => set_board(data, &id, Some(m.id.to_string())).await,
                Err(e) => {
                    eprintln!("[Starboard] publish: {}", e);
                    set_board(data, &id, None).await;
                }
            }
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::testing;

    #[tokio::test]
    async fn count_and_single_claim() {
        let (data, dir) = testing::data("stars", "").await;
        let s = set_count(&data, "1", "10", "5", 2).await.unwrap();
        assert_eq!((s.stars, s.board_message), (2, None));
        assert!(claim(&data, "1").await);
        assert!(!claim(&data, "1").await, "the second star doesn't publish again");
        set_board(&data, "1", Some("999".into())).await;
        let s = set_count(&data, "1", "10", "5", 4).await.unwrap();
        assert_eq!((s.stars, s.board_message.as_deref()), (4, Some("999")));
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
