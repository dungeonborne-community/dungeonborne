//! Moderation log built on SurrealDB:
//! - anything that needs logging is a record in the `mod_log` table; any module or DB event can create one
//!   (if the bot moves to a SurrealDB server over ws, the backend can too, e.g. for in-game bans);
//! - a single `LIVE SELECT … WHERE posted = false` subscription sends records to the channel; missed ones are sent on startup;
//! - member joins and leaves are logged by the database itself: a `DEFINE EVENT` on the `member` table creates a `mod_log` record,
//!   the bot only updates `member` (UPSERT on join, left_at on leave); a repeat join shows in the joins counter;
//! - the database deletes old sent records itself (retention event).

use crate::Data;
use futures::StreamExt;
use poise::serenity_prelude as serenity;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use surrealdb::{Action, Notification, RecordId};

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE mod_log SCHEMAFULL;
DEFINE FIELD OVERWRITE kind ON mod_log TYPE string;
DEFINE FIELD OVERWRITE user_id ON mod_log TYPE option<string>;
DEFINE FIELD OVERWRITE user_name ON mod_log TYPE option<string>;
DEFINE FIELD OVERWRITE channel_id ON mod_log TYPE option<string>;
DEFINE FIELD OVERWRITE text ON mod_log TYPE option<string>;
DEFINE FIELD OVERWRITE old_text ON mod_log TYPE option<string>;
DEFINE FIELD OVERWRITE posted ON mod_log TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE at ON mod_log TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX OVERWRITE idx_mod_log_posted ON mod_log COLUMNS posted;
DEFINE EVENT OVERWRITE mod_log_retention ON mod_log WHEN $event = "CREATE" THEN {
    DELETE mod_log WHERE posted = true AND at < time::now() - 90d;
};

DEFINE TABLE OVERWRITE member SCHEMAFULL;
DEFINE FIELD OVERWRITE name ON member TYPE option<string>;
DEFINE FIELD OVERWRITE joined_at ON member TYPE option<datetime>;
DEFINE FIELD OVERWRITE left_at ON member TYPE option<datetime>;
DEFINE FIELD OVERWRITE joins ON member TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE greeted ON member TYPE set<string> DEFAULT [];
DEFINE FIELD OVERWRITE invite_code ON member TYPE option<string>;
DEFINE FIELD OVERWRITE invited_by ON member TYPE option<string>;
DEFINE EVENT OVERWRITE member_join_leave ON member
    WHEN $event != "DELETE" AND ($before.joined_at != $after.joined_at
        OR ($before.left_at != $after.left_at AND $after.left_at != NONE))
    THEN {
        LET $join = $before.joined_at != $after.joined_at;
        LET $parts = [
            IF $join AND $after.joins > 1 THEN "rejoin #" + <string>$after.joins END,
            IF $join AND $after.invite_code != NONE THEN "invite " + $after.invite_code END,
            IF $join AND $after.invited_by != NONE THEN "by <@" + $after.invited_by + ">" END,
        ];
        CREATE mod_log CONTENT {
            kind: IF $join THEN "join" ELSE "leave" END,
            user_id: record::id($after.id),
            user_name: $after.name,
            text: array::join($parts[WHERE $this != NONE], " · ") OR NONE,
        };
    };
"#;

/// Log record. Empty fields aren't serialized: in SCHEMAFULL option<string> accepts NONE but not NULL
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Entry {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Stored {
    id: RecordId,
    kind: String,
    user_id: Option<String>,
    user_name: Option<String>,
    channel_id: Option<String>,
    text: Option<String>,
    old_text: Option<String>,
}

pub async fn write(data: &Data, entry: Entry) {
    if data.cfg.mod_log.is_none() {
        return;
    }
    if let Err(e) = data.db.db.query("CREATE mod_log CONTENT $e").bind(("e", entry)).await.and_then(|r| r.check()) {
        eprintln!("[ModLog] record: {}", e);
    }
}

/// Join. `invite` is the invite used, from the tracker (code, inviter), if it could be determined
pub async fn member_join(data: &Data, user: &serenity::User, invite: Option<crate::modules::invites::Snap>) {
    let q = "UPSERT type::thing('member', $id) SET name = $name, joined_at = time::now(), left_at = NONE,
             joins = (joins OR 0) + 1, invite_code = $code, invited_by = $by";
    let (code, by) = invite.map(|s| (Some(s.code), s.inviter)).unwrap_or((None, None));
    let r = data
        .db
        .db
        .query(q)
        .bind(("id", user.id.to_string()))
        .bind(("name", user.name.clone()))
        .bind(("code", code))
        .bind(("by", by))
        .await;
    if let Err(e) = r.and_then(|r| r.check()) {
        eprintln!("[ModLog] join {}: {}", user.name, e);
    }
}

pub async fn member_leave(data: &Data, user: &serenity::User) {
    let q = "UPSERT type::thing('member', $id) SET name = $name, left_at = time::now()";
    let r = data.db.db.query(q).bind(("id", user.id.to_string())).bind(("name", user.name.clone())).await;
    if let Err(e) = r.and_then(|r| r.check()) {
        eprintln!("[ModLog] leave {}: {}", user.name, e);
    }
}

/// LIVE subscription → log channel. Reconnects when the stream drops; on every connect sends what was missed.
pub fn spawn(ctx: serenity::Context, data: Data) {
    let Some(cfg) = &data.cfg.mod_log else { return };
    let Some(channel) = data.names.need_channel(&cfg.channel, "ModLog") else { return };
    tokio::spawn(async move {
        loop {
            match data.db.db.query("LIVE SELECT * FROM mod_log WHERE posted = false").await {
                Ok(mut r) => match r.stream::<Notification<Stored>>(0) {
                    Ok(mut stream) => {
                        println!("[ModLog] LIVE SELECT mod_log → #{}", channel);
                        post_pending(&ctx, &data, channel).await;
                        while let Some(n) = stream.next().await {
                            match n {
                                Ok(n) if n.action == Action::Create => post(&ctx, &data, channel, n.data).await,
                                Ok(_) => {}
                                Err(e) => eprintln!("[ModLog] LIVE: {}", e),
                            }
                        }
                        eprintln!("[ModLog] LIVE: stream closed, reconnecting");
                    }
                    Err(e) => eprintln!("[ModLog] LIVE: {}", e),
                },
                Err(e) => eprintln!("[ModLog] LIVE: {}", e),
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

async fn post_pending(ctx: &serenity::Context, data: &Data, channel: serenity::ChannelId) {
    let pending: Vec<Stored> = match data.db.db.query("SELECT * FROM mod_log WHERE posted = false ORDER BY at").await {
        Ok(mut r) => r.take(0).unwrap_or_default(),
        Err(e) => {
            eprintln!("[ModLog] catch-up: {}", e);
            return;
        }
    };
    if !pending.is_empty() {
        println!("[ModLog] sending {} missed records", pending.len());
    }
    for entry in pending {
        post(ctx, data, channel, entry).await;
    }
}

/// First "claim" the record (posted = true only if still false), so LIVE and catch-up won't send it twice
async fn post(ctx: &serenity::Context, data: &Data, channel: serenity::ChannelId, entry: Stored) {
    let claimed = match data
        .db
        .db
        .query("UPDATE $id SET posted = true WHERE posted = false RETURN VALUE posted")
        .bind(("id", entry.id.clone()))
        .await
    {
        Ok(mut r) => r.take::<Vec<bool>>(0).map(|v| !v.is_empty()).unwrap_or(false),
        Err(e) => {
            eprintln!("[ModLog] {}: {}", entry.id, e);
            return;
        }
    };
    if !claimed {
        return;
    }
    let msg = serenity::CreateMessage::new()
        .embed(render(&entry))
        .allowed_mentions(serenity::CreateAllowedMentions::new());
    if let Err(e) = channel.send_message(&ctx.http, msg).await {
        eprintln!("[ModLog] send {}: {}", entry.kind, e);
        let _ = data.db.db.query("UPDATE $id SET posted = false").bind(("id", entry.id)).await;
    }
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max - 1).collect::<String>() + "…"
    }
}

fn render(e: &Stored) -> serenity::CreateEmbed {
    let (icon, title, color) = match e.kind.as_str() {
        "join" => ("📥", "Joined / Зашёл", 0x2ECC71),
        "leave" => ("📤", "Left / Вышел", 0x95A5A6),
        "message_delete" => ("🗑", "Message deleted / Сообщение удалено", 0xED4245),
        "message_edit" => ("✏️", "Message edited / Сообщение изменено", 0xF1C40F),
        "ban" => ("🔨", "Banned / Бан", 0x992D22),
        "unban" => ("🕊", "Unbanned / Разбан", 0x3498DB),
        "timeout" => ("🔇", "Timeout / Тайм-аут", 0xE67E22),
        "ticket_open" => ("🎫", "Ticket opened / Тикет открыт", 0x5865F2),
        "ticket_close" => ("🔒", "Ticket closed / Тикет закрыт", 0x95A5A6),
        "creator_add" => ("🎬", "Creator added / Автор добавлен", 0x9B59B6),
        "creator_remove" => ("🎬", "Creator removed / Автор убран", 0x95A5A6),
        "creator_post" => ("📣", "Creator announcement / Анонс автора", 0xE91E63),
        other => ("📝", other, 0x5865F2),
    };
    let mut lines = Vec::new();
    if let Some(id) = &e.user_id {
        let name = e.user_name.as_deref().map(|n| format!(" ({})", n)).unwrap_or_default();
        lines.push(format!("**User:** <@{}>{}", id, name));
    }
    if let Some(ch) = &e.channel_id {
        lines.push(format!("**Channel:** <#{}>", ch));
    }
    let mut embed = serenity::CreateEmbed::new()
        .title(format!("{} {}", icon, title))
        .color(color)
        .description(lines.join("\n"))
        .timestamp(serenity::Timestamp::now());
    if let Some(old) = e.old_text.as_deref().filter(|s| !s.is_empty()) {
        embed = embed.field("Before / Было", clip(old, 1024), false);
    }
    if let Some(text) = e.text.as_deref().filter(|s| !s.is_empty()) {
        let name = if e.old_text.is_some() { "After / Стало" } else { "Text / Текст" };
        embed = embed.field(name, clip(text, 1024), false);
    }
    if let Some(id) = &e.user_id {
        embed = embed.footer(serenity::CreateEmbedFooter::new(format!("ID {}", id)));
    }
    embed
}

/// Deleted message: the text comes from the serenity cache (if the message was in it)
pub async fn message_delete(ctx: &serenity::Context, data: &Data, channel: serenity::ChannelId, id: serenity::MessageId) {
    let Some(msg) = ctx.cache.message(channel, id).map(|m| m.clone()) else { return };
    if msg.author.bot {
        return;
    }
    let mut text = msg.content.clone();
    for a in &msg.attachments {
        text.push_str(&format!("\n📎 {}", a.filename));
    }
    write(
        data,
        Entry {
            kind: "message_delete".into(),
            user_id: Some(msg.author.id.to_string()),
            user_name: Some(msg.author.name.clone()),
            channel_id: Some(channel.to_string()),
            text: Some(text),
            ..Default::default()
        },
    )
    .await;
}

pub async fn message_edit(data: &Data, old: Option<&serenity::Message>, event: &serenity::MessageUpdateEvent) {
    let (Some(old), Some(new_text)) = (old, event.content.as_ref()) else { return };
    if old.author.bot || &old.content == new_text {
        return; // edits without a text change are link previews unfurling
    }
    write(
        data,
        Entry {
            kind: "message_edit".into(),
            user_id: Some(old.author.id.to_string()),
            user_name: Some(old.author.name.clone()),
            channel_id: Some(event.channel_id.to_string()),
            text: Some(new_text.clone()),
            old_text: Some(old.content.clone()),
        },
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::testing;

    async fn kinds(data: &Data) -> Vec<(String, Option<String>)> {
        #[derive(Deserialize)]
        struct K {
            kind: String,
            text: Option<String>,
        }
        let rows: Vec<K> = data.db.db.query("SELECT kind, text, at FROM mod_log ORDER BY at").await.unwrap().take(0).unwrap();
        rows.into_iter().map(|k| (k.kind, k.text)).collect()
    }

    /// DEFINE EVENT on member writes join/leave/repeat join; a record from the greeting (without joined_at) isn't a join
    #[tokio::test]
    async fn member_events_write_log() {
        let (data, dir) = testing::data("member", "").await;
        let u = testing::user(42, "Вася");
        assert!(crate::modules::welcome::claim(&data, u.id, "RU").await);
        assert!(kinds(&data).await.is_empty(), "a greeted record doesn't count as a join");

        member_join(&data, &u, None).await;
        member_leave(&data, &u).await;
        member_join(&data, &u, Some(crate::modules::invites::Snap { code: "abc".into(), uses: 3, max_uses: 0, inviter: Some("1".into()) })).await;
        member_leave(&data, &testing::user(7, "Старожил")).await; // joined before the bot — the leave is still visible
        assert_eq!(
            kinds(&data).await,
            vec![
                ("join".into(), None),
                ("leave".into(), None),
                ("join".into(), Some("rejoin #2 · invite abc · by <@1>".into())),
                ("leave".into(), None)
            ]
        );
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LIVE SELECT delivers the new record in full; the posted "claim" fires exactly once
    #[tokio::test]
    async fn live_and_claim() {
        let (data, dir) = testing::data("live", "").await;
        let mut r = data.db.db.query("LIVE SELECT * FROM mod_log WHERE posted = false").await.unwrap();
        let mut stream = r.stream::<Notification<Stored>>(0).unwrap();
        write(&data, Entry { kind: "ban".into(), user_id: Some("5".into()), ..Default::default() }).await;

        let n = tokio::time::timeout(Duration::from_secs(5), stream.next()).await.unwrap().unwrap().unwrap();
        assert_eq!(n.action, Action::Create);
        assert_eq!((n.data.kind.as_str(), n.data.user_id.as_deref(), n.data.text.as_deref()), ("ban", Some("5"), None));

        let claim = "UPDATE $id SET posted = true WHERE posted = false RETURN VALUE posted";
        let first: Vec<bool> = data.db.db.query(claim).bind(("id", n.data.id.clone())).await.unwrap().take(0).unwrap();
        let second: Vec<bool> = data.db.db.query(claim).bind(("id", n.data.id)).await.unwrap().take(0).unwrap();
        assert_eq!((first.len(), second.len()), (1, 0));
        drop(stream);
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

pub async fn ban(data: &Data, user: &serenity::User, banned: bool) {
    let kind = if banned { "ban" } else { "unban" };
    write(data, Entry { kind: kind.into(), user_id: Some(user.id.to_string()), user_name: Some(user.name.clone()), ..Default::default() }).await;
}

pub async fn timeout(data: &Data, old: Option<&serenity::Member>, event: &serenity::GuildMemberUpdateEvent) {
    let before = old.and_then(|m| m.communication_disabled_until);
    let after = event.communication_disabled_until;
    if before == after {
        return;
    }
    let text = match after {
        Some(until) if until.unix_timestamp() > serenity::Timestamp::now().unix_timestamp() => {
            format!("until / до <t:{}:f>", until.unix_timestamp())
        }
        _ => "removed / снят".into(),
    };
    write(
        data,
        Entry {
            kind: "timeout".into(),
            user_id: Some(event.user.id.to_string()),
            user_name: Some(event.user.name.clone()),
            text: Some(text),
            ..Default::default()
        },
    )
    .await;
}
