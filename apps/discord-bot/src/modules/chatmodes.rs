//! Channel modes for moderators:
//! - `/slowmode seconds:<0–21600> [for:<30m|2h|1d>]`: Discord slow mode; with `for` it restores the previous value by itself;
//! - `/emojimode on|off [user] [for:…]`: emoji-only: the bot deletes messages containing anything but emoji (Unicode and
//!   the server's custom emoji) and gives a short hint. Without `user`, for the whole channel (team roles are exempt); with `user`,
//!   for that member personally in every channel of the server. With `for` it turns itself off.
//! State: records `chat_mode:[channel, kind]` and `chat_mode:[member, 'user']` (composite id; `channel` of a personal
//! record is where it was enabled, and the end message goes there too); the database tracks expiry, every 30 s `DELETE … RETURN BEFORE` takes
//! the expired ones and everything is restored.

use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE chat_mode SCHEMAFULL;
DEFINE FIELD OVERWRITE channel ON chat_mode TYPE string;
DEFINE FIELD OVERWRITE kind ON chat_mode TYPE string ASSERT $value INSIDE ["slow", "emoji", "user"];
DEFINE FIELD OVERWRITE user ON chat_mode TYPE option<string>;
DEFINE FIELD OVERWRITE prev ON chat_mode TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE until ON chat_mode TYPE option<datetime>;
DEFINE FIELD OVERWRITE by ON chat_mode TYPE string DEFAULT "";
DEFINE INDEX OVERWRITE idx_chat_mode_until ON chat_mode COLUMNS until;
"#;

/// Characters emoji are made of (including flags, keycap, ZWJ sequences and modifiers)
fn emoji_char(c: char) -> bool {
    matches!(c as u32,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2300..=0x23FF | 0x2B00..=0x2BFF | 0x2190..=0x21FF
        | 0x3030 | 0x303D | 0x3297 | 0x3299 | 0x00A9 | 0x00AE | 0x203C | 0x2049 | 0x2122 | 0x2139
        | 0x200D | 0xFE0F | 0xFE0E | 0x20E3 | 0xE0020..=0xE007F)
}

/// The message consists only of emoji (Unicode and custom `<:name:id>` / `<a:name:id>`) and whitespace
pub fn only_emoji(text: &str) -> bool {
    let mut rest = String::new();
    let mut s = text;
    // server custom emoji
    while let Some(start) = s.find('<') {
        rest.push_str(&s[..start]);
        let tail = &s[start..];
        match tail.find('>') {
            Some(end) if (tail.starts_with("<:") || tail.starts_with("<a:")) && tail[..end].matches(':').count() == 2 => s = &tail[end + 1..],
            _ => {
                rest.push('<');
                s = &tail[1..];
            }
        }
    }
    rest.push_str(s);
    let keycap = rest.contains('\u{20E3}');
    let mut any = text.contains("<:") || text.contains("<a:");
    for c in rest.chars().filter(|c| !c.is_whitespace()) {
        if emoji_char(c) || (keycap && (c.is_ascii_digit() || c == '#' || c == '*')) {
            any = true;
        } else {
            return false;
        }
    }
    any
}

async fn set_mode(data: &Data, channel: serenity::ChannelId, kind: &str, prev: u16, after: Option<&str>, by: serenity::UserId) -> Result<Option<i64>, Error> {
    set_mode_for(data, &channel.to_string(), channel, None, kind, prev, after, by).await
}

/// `key` is the first part of the record id: the channel, or the member for the personal mode
#[allow(clippy::too_many_arguments)]
async fn set_mode_for(
    data: &Data,
    key: &str,
    channel: serenity::ChannelId,
    user: Option<serenity::UserId>,
    kind: &str,
    prev: u16,
    after: Option<&str>,
    by: serenity::UserId,
) -> Result<Option<i64>, Error> {
    let mut r = data
        .db
        .db
        .query(
            "LET $until = IF $after = NONE THEN NONE ELSE time::now() + <duration> $after END;
             IF $until != NONE AND ($until < time::now() + 1m OR $until > time::now() + 30d) { THROW 'срок — от 1m до 30d' };
             LET $had = record::exists(type::thing('chat_mode', [$key, $kind]));
             UPSERT type::thing('chat_mode', [$key, $kind]) SET channel = $ch, kind = $kind, user = $user,
                 prev = IF $had THEN prev ELSE $prev END, until = $until, by = $by
                 RETURN VALUE IF until = NONE THEN NONE ELSE time::unix(until) END;",
        )
        .bind(("after", after.map(|a| a.trim().to_string())))
        .bind(("key", key.to_string()))
        .bind(("ch", channel.to_string()))
        .bind(("user", user.map(|u| u.to_string())))
        .bind(("kind", kind.to_string()))
        .bind(("prev", prev as i64))
        .bind(("by", by.to_string()))
        .await?;
    if let Some(e) = r.take_errors().values().next() {
        let t = e.to_string();
        return Err(if t.contains("срок") { "срок — от 1m до 30d, например 30m, 2h, 1d".into() } else { format!("срок «{}»: {}", after.unwrap_or(""), t).into() });
    }
    let v: Vec<Option<i64>> = r.take(3)?;
    Ok(v.into_iter().next().flatten())
}

async fn clear_mode(data: &Data, channel: serenity::ChannelId, kind: &str) -> Option<i64> {
    clear_key(data, &channel.to_string(), kind).await
}

async fn clear_key(data: &Data, key: &str, kind: &str) -> Option<i64> {
    #[derive(Deserialize)]
    struct Prev {
        prev: i64,
    }
    let rows: Vec<Prev> = data
        .db
        .db
        .query("DELETE type::thing('chat_mode', [$ch, $kind]) RETURN BEFORE")
        .bind(("ch", key.to_string()))
        .bind(("kind", kind.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();
    rows.into_iter().next().map(|p| p.prev)
}

fn current_slowmode(ctx: &serenity::Context, data: &Data, channel: serenity::ChannelId) -> u16 {
    ctx.cache.guild(data.guild).and_then(|g| g.channels.get(&channel).and_then(|c| c.rate_limit_per_user)).unwrap_or(0)
}

/// Slow mode for this channel / Медленный режим в этом канале
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_MESSAGES")]
pub async fn slowmode(
    ctx: Context<'_>,
    #[description = "Seconds between messages, 0 = off (max 21600) / Секунд между сообщениями"]
    #[min = 0]
    #[max = 21600]
    seconds: u16,
    #[description = "Turn back after: 30m, 2h, 1d / Вернуть как было через"]
    #[rename = "for"]
    duration: Option<String>,
) -> Result<(), Error> {
    let data = ctx.data();
    let channel = ctx.channel_id();
    let prev = current_slowmode(ctx.serenity_context(), data, channel);
    let ru = ctx.locale().is_some_and(|l| l.starts_with("ru"));
    let until = if seconds == 0 {
        // turned off manually: don't restore the previous value anymore
        clear_mode(data, channel, "slow").await;
        None
    } else if let Some(d) = duration.as_deref() {
        set_mode(data, channel, "slow", prev, Some(d), ctx.author().id).await?
    } else {
        clear_mode(data, channel, "slow").await;
        None
    };
    channel.edit(ctx.http(), serenity::EditChannel::new().rate_limit_per_user(seconds)).await?;
    let back = until.map(|u| if ru { format!(", вернётся <t:{}:R>", u) } else { format!(", back <t:{}:R>", u) }).unwrap_or_default();
    let text = match (seconds, ru) {
        (0, true) => "🐇 Медленный режим выключен".to_string(),
        (0, false) => "🐇 Slow mode off".to_string(),
        (s, true) => format!("🐢 Медленный режим: одно сообщение в {} с{}", s, back),
        (s, false) => format!("🐢 Slow mode: one message every {} s{}", s, back),
    };
    ctx.send(poise::CreateReply::default().content(text).ephemeral(true)).await?;
    Ok(())
}

#[derive(poise::ChoiceParameter, PartialEq)]
pub enum OnOff {
    #[name = "on"]
    On,
    #[name = "off"]
    Off,
}

/// Emoji-only mode for this channel or one member / «Только эмодзи» в канале или для участника
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_MESSAGES")]
pub async fn emojimode(
    ctx: Context<'_>,
    #[description = "on / off"] mode: OnOff,
    #[description = "Only this member, in every channel / Только этому участнику, во всех каналах"] user: Option<serenity::User>,
    #[description = "Turn off after: 30m, 2h, 1d / Выключить через"]
    #[rename = "for"]
    duration: Option<String>,
) -> Result<(), Error> {
    let data = ctx.data();
    let channel = ctx.channel_id();
    let ru = ctx.locale().is_some_and(|l| l.starts_with("ru"));
    if let Some(u) = user {
        return emojimode_user(ctx, mode, u, duration).await;
    }
    let text = if mode == OnOff::On {
        let until = set_mode(data, channel, "emoji", 0, duration.as_deref(), ctx.author().id).await?;
        let back = until.map(|u| if ru { format!(", выключится <t:{}:R>", u) } else { format!(", off <t:{}:R>", u) }).unwrap_or_default();
        let public = if ru { "😀 Режим «только эмодзи»: пишем одними эмодзи" } else { "😀 Emoji-only mode: emoji only, please" };
        let _ = channel.send_message(ctx.http(), serenity::CreateMessage::new().content(format!("{}{}", public, back))).await;
        if ru { format!("✅ Включено{}", back) } else { format!("✅ On{}", back) }
    } else {
        clear_mode(data, channel, "emoji").await;
        let _ = channel
            .send_message(ctx.http(), serenity::CreateMessage::new().content(if ru { "💬 Режим «только эмодзи» выключен" } else { "💬 Emoji-only mode is off" }))
            .await;
        (if ru { "✅ Выключено" } else { "✅ Off" }).to_string()
    };
    ctx.send(poise::CreateReply::default().content(text).ephemeral(true)).await?;
    Ok(())
}

/// Personal mode: the member may use only emoji in every channel of the server
async fn emojimode_user(ctx: Context<'_>, mode: OnOff, user: serenity::User, duration: Option<String>) -> Result<(), Error> {
    let data = ctx.data();
    let channel = ctx.channel_id();
    let ru = ctx.locale().is_some_and(|l| l.starts_with("ru"));
    if user.bot {
        return Err((if ru { "у ботов режима нет" } else { "bots can't be put in emoji mode" }).into());
    }
    let ping = serenity::CreateAllowedMentions::new().users(vec![user.id]);
    let text = if mode == OnOff::On {
        let until = set_mode_for(data, &user.id.to_string(), channel, Some(user.id), "user", 0, duration.as_deref(), ctx.author().id).await?;
        let back = until.map(|u| format!(" · <t:{}:R>", u)).unwrap_or_default();
        let public = format!("😀 <@{}> speaks only in emoji now / теперь говорит только эмодзи{}", user.id, back);
        let _ = channel.send_message(ctx.http(), serenity::CreateMessage::new().content(public).allowed_mentions(ping)).await;
        println!("[ChatModes] {} → emoji-only for {} ({})", ctx.author().name, user.name, duration.as_deref().unwrap_or("no limit"));
        if ru { format!("✅ {}: только эмодзи во всех каналах{}", user.name, back) } else { format!("✅ {}: emoji only in every channel{}", user.name, back) }
    } else {
        if clear_key(data, &user.id.to_string(), "user").await.is_none() {
            return Err((if ru { "у этого участника режим не включён" } else { "this member isn't in emoji mode" }).into());
        }
        let public = format!("💬 <@{}> can write text again / снова может писать текстом", user.id);
        let _ = channel.send_message(ctx.http(), serenity::CreateMessage::new().content(public).allowed_mentions(serenity::CreateAllowedMentions::new())).await;
        (if ru { "✅ Выключено" } else { "✅ Off" }).to_string()
    };
    ctx.send(poise::CreateReply::default().content(text).ephemeral(true)).await?;
    Ok(())
}

/// Modes for a message: (emoji-only channel, emoji-only member)
async fn emoji_modes(data: &Data, channel: serenity::ChannelId, user: serenity::UserId) -> (bool, bool) {
    data.db
        .db
        .query("RETURN [record::exists(type::thing('chat_mode', [$ch, 'emoji'])), record::exists(type::thing('chat_mode', [$u, 'user']))]")
        .bind(("ch", channel.to_string()))
        .bind(("u", user.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take::<Vec<bool>>(0).ok())
        .map(|v| (v.first().copied().unwrap_or(false), v.get(1).copied().unwrap_or(false)))
        .unwrap_or((false, false))
}

#[cfg(test)]
async fn emoji_mode_on(data: &Data, channel: serenity::ChannelId) -> bool {
    data.db
        .db
        .query("RETURN record::exists(type::thing('chat_mode', [$ch, 'emoji']))")
        .bind(("ch", channel.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take::<Option<bool>>(0).ok().flatten())
        .unwrap_or(false)
}

/// New message: in an emoji-only channel delete everything else (moderators are exempt)
pub async fn on_message(ctx: &serenity::Context, data: &Data, msg: &serenity::Message) {
    if msg.author.bot || msg.webhook_id.is_some() {
        return;
    }
    let (in_channel, personal) = emoji_modes(data, msg.channel_id, msg.author.id).await;
    if !in_channel && !personal {
        return;
    }
    // team roles are exempt from the channel mode; the personal mode applies to whoever it was enabled for
    let staff = data.cfg.chatmodes.as_ref().map(|c| c.exempt_roles.as_slice()).unwrap_or_default();
    if !personal && msg.member.as_ref().is_some_and(|m| staff.iter().filter_map(|r| data.names.role(r)).any(|r| m.roles.contains(&r))) {
        return;
    }
    if only_emoji(&msg.content) && msg.attachments.is_empty() {
        return;
    }
    // delete in batches (see flush); the hint at most once per HINT_EVERY per person per channel
    queue_delete(msg.channel_id, msg.id);
    let now = Instant::now();
    let fresh = {
        let mut hints = HINTS.lock().unwrap();
        hints.retain(|_, t| now.duration_since(*t) < HINT_EVERY);
        hints.insert((msg.channel_id, msg.author.id), now).is_none()
    };
    if !fresh {
        return;
    }
    let note = if personal {
        format!("<@{}> 😀 emoji only for you right now / тебе пока можно только эмодзи", msg.author.id)
    } else {
        format!("<@{}> 😀 emoji only here right now / сейчас здесь только эмодзи", msg.author.id)
    };
    if let Ok(n) = msg
        .channel_id
        .send_message(&ctx.http, serenity::CreateMessage::new().content(note).allowed_mentions(serenity::CreateAllowedMentions::new().users(vec![msg.author.id])))
        .await
    {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(6)).await;
            queue_delete(n.channel_id, n.id);
        });
    }
}

/// Hint to one person in a channel: no more often than this
const HINT_EVERY: Duration = Duration::from_secs(30);
/// Messages to delete, per channel: once a second they go out as one bulk-delete; under flood that's 1 request instead of dozens,
/// otherwise Discord's per-channel delete limit queues them up and messages hang around for minutes
static TO_DELETE: LazyLock<Mutex<HashMap<serenity::ChannelId, Vec<serenity::MessageId>>>> = LazyLock::new(Default::default);
static HINTS: LazyLock<Mutex<HashMap<(serenity::ChannelId, serenity::UserId), Instant>>> = LazyLock::new(Default::default);

fn queue_delete(channel: serenity::ChannelId, id: serenity::MessageId) {
    TO_DELETE.lock().unwrap().entry(channel).or_default().push(id);
}

async fn flush(http: &serenity::Http) {
    let batch: Vec<_> = std::mem::take(&mut *TO_DELETE.lock().unwrap()).into_iter().collect();
    for (ch, mut ids) in batch {
        ids.sort();
        ids.dedup();
        for chunk in ids.chunks(100) {
            let r = if chunk.len() == 1 { ch.delete_message(http, chunk[0]).await } else { ch.delete_messages(http, chunk).await };
            if let Err(e) = r {
                eprintln!("[ChatModes] {}: failed to delete {} messages: {}", ch, chunk.len(), e);
            }
        }
    }
}

/// Expired modes: slow mode gets its previous value back, emoji-only is turned off
pub fn spawn(ctx: serenity::Context, data: Data) {
    if data.cfg.chatmodes.is_none() {
        return;
    }
    let http = ctx.http.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            flush(&http).await;
        }
    });
    tokio::spawn(async move {
        #[derive(Deserialize)]
        struct Row {
            channel: String,
            kind: String,
            prev: i64,
            user: Option<String>,
        }
        loop {
            let due: Vec<Row> = data
                .db
                .db
                .query("DELETE chat_mode WHERE until != NONE AND until <= time::now() RETURN BEFORE")
                .await
                .ok()
                .and_then(|mut r| r.take(0).ok())
                .unwrap_or_default();
            for r in due {
                let Ok(id) = r.channel.parse::<u64>() else { continue };
                let ch = serenity::ChannelId::new(id);
                if r.kind == "slow" {
                    let _ = ch.edit(&ctx.http, serenity::EditChannel::new().rate_limit_per_user(r.prev.clamp(0, 21600) as u16)).await;
                    println!("[ChatModes] {}: slow mode restored to {} s", id, r.prev);
                } else if r.kind == "user" {
                    let who = r.user.unwrap_or_default();
                    let text = format!("💬 <@{}> can write text again / снова может писать текстом", who);
                    let _ = ch.send_message(&ctx.http, serenity::CreateMessage::new().content(text).allowed_mentions(serenity::CreateAllowedMentions::new())).await;
                    println!("[ChatModes] {}: personal emoji-only expired", who);
                } else {
                    let _ = ch.send_message(&ctx.http, serenity::CreateMessage::new().content("💬 Emoji-only mode is off / Режим «только эмодзи» выключен")).await;
                    println!("[ChatModes] {}: emoji-only expired", id);
                }
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::only_emoji;
    use crate::modules::testing;
    use poise::serenity_prelude as serenity;

    /// A repeated /slowmode with for keeps the original value; without for the record is removed
    #[tokio::test]
    async fn slow_mode_keeps_original_value() {
        let (data, dir) = testing::data("chatmodes", "").await;
        let (ch, u) = (serenity::ChannelId::new(7), serenity::UserId::new(1));
        assert!(super::set_mode(&data, ch, "slow", 30, Some("10s"), u).await.is_err());
        assert!(super::set_mode(&data, ch, "slow", 30, Some("1h"), u).await.unwrap().is_some());
        super::set_mode(&data, ch, "slow", 120, Some("2h"), u).await.unwrap();
        assert_eq!(super::clear_mode(&data, ch, "slow").await, Some(30));
        assert_eq!(super::clear_mode(&data, ch, "slow").await, None);
        assert!(super::set_mode(&data, ch, "emoji", 0, None, u).await.unwrap().is_none());
        assert!(super::emoji_mode_on(&data, ch).await);
        // personal mode: per member, in every channel; the channel keeps its own mode
        let (other, who) = (serenity::ChannelId::new(8), serenity::UserId::new(55));
        super::set_mode_for(&data, &who.to_string(), other, Some(who), "user", 0, Some("30m"), u).await.unwrap();
        assert_eq!(super::emoji_modes(&data, other, who).await, (false, true));
        assert_eq!(super::emoji_modes(&data, ch, who).await, (true, true));
        assert_eq!(super::emoji_modes(&data, other, u).await, (false, false));
        assert!(super::clear_key(&data, &who.to_string(), "user").await.is_some());
        assert_eq!(super::emoji_modes(&data, other, who).await, (false, false));
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn emoji_only_detection() {
        assert!(only_emoji("😀"));
        assert!(only_emoji("🔥 🔥  👍🏽"));
        assert!(only_emoji("👨‍👩‍👧 🇷🇺 ❤️"));
        assert!(only_emoji("<:pepe:123456> <a:dance:42>"));
        assert!(only_emoji("1️⃣"));
        assert!(!only_emoji("lol 😀"));
        assert!(!only_emoji("123"));
        assert!(!only_emoji(""));
        assert!(!only_emoji("<@123> 😀"));
    }
}
