//! Deleting a message after a set time: "Delete later" in the message menu (moderators), the `delete_after` field
//! of `/announce`, request files from scripts (`post_announcement.py --delete-after 7d`) in the queue folder.
//! The schedule is `expire_msg:<message id>` records; the database computes the deadline (`time::now() + <duration>`).
//! Every 30 s `DELETE … WHERE at <= time::now() RETURN BEFORE` atomically claims the due ones — no double deletion.
//! The thread opened from the message is deleted with it (the "🌍 …" translations thread).

use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use std::time::Duration;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE expire_msg SCHEMAFULL;
DEFINE FIELD OVERWRITE channel ON expire_msg TYPE string;
DEFINE FIELD OVERWRITE at ON expire_msg TYPE datetime;
DEFINE FIELD OVERWRITE by ON expire_msg TYPE string DEFAULT "";
DEFINE INDEX OVERWRITE idx_expire_at ON expire_msg COLUMNS at;
"#;

const SCHEDULE: &str = r#"
LET $d = <duration> $after;
IF $d < 1m OR $d > 365d { THROW "срок — от 1m до 365d, например 30m, 12h, 7d" };
UPSERT type::thing('expire_msg', $msg) SET channel = $ch, at = time::now() + $d, by = $by RETURN VALUE time::unix(at);
"#;

/// Schedule a message for deletion after `after` (SurrealDB duration: 30m, 12h, 7d). Returns the unix deletion time.
pub async fn schedule(data: &Data, channel: serenity::ChannelId, message: serenity::MessageId, after: &str, by: &str) -> Result<i64, Error> {
    let mut r = data
        .db
        .db
        .query(SCHEDULE)
        .bind(("after", after.trim().to_string()))
        .bind(("msg", message.to_string()))
        .bind(("ch", channel.to_string()))
        .bind(("by", by.to_string()))
        .await?;
    let errors = r.take_errors();
    if let Some(e) = errors.values().next() {
        let text = e.to_string();
        return Err(if text.contains("срок") { "срок — от 1m до 365d, например 30m, 12h, 7d".into() } else { format!("срок «{}»: {}", after.trim(), text).into() });
    }
    let at: Vec<i64> = r.take(2)?;
    at.first().copied().ok_or_else(|| "не удалось запланировать".into())
}

async fn cancel(data: &Data, message: serenity::MessageId) -> bool {
    data.db
        .db
        .query("DELETE type::thing('expire_msg', $msg) RETURN BEFORE")
        .bind(("msg", message.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take::<Vec<Due>>(0).ok())
        .is_some_and(|v| !v.is_empty())
}

/// Удалить позже
#[poise::command(context_menu_command = "Delete later", guild_only, default_member_permissions = "MANAGE_MESSAGES")]
pub async fn delete_later(ctx: Context<'_>, msg: serenity::Message) -> Result<(), Error> {
    let poise::Context::Application(actx) = ctx else { return Ok(()) };
    let modal = serenity::CreateModal::new(format!("ex:form:{}:{}", msg.channel_id, msg.id), "Delete later / Удалить позже").components(vec![
        serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(serenity::InputTextStyle::Short, "After: 30m, 12h, 7d · 0 = cancel / отмена", "after")
                .placeholder("7d")
                .max_length(10)
                .required(true),
        ),
    ]);
    actx.interaction.create_response(ctx.http(), serenity::CreateInteractionResponse::Modal(modal)).await?;
    actx.has_sent_initial_response.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

pub async fn on_modal(ctx: &serenity::Context, data: &Data, mi: &serenity::ModalInteraction) {
    let Some(rest) = mi.data.custom_id.strip_prefix("ex:form:") else { return };
    let ru = mi.locale.starts_with("ru");
    let after: String = mi
        .data
        .components
        .iter()
        .flat_map(|r| &r.components)
        .find_map(|c| match c {
            serenity::ActionRowComponent::InputText(t) if t.custom_id == "after" => t.value.clone(),
            _ => None,
        })
        .unwrap_or_default();
    let text = match rest.split_once(':').and_then(|(c, m)| Some((c.parse::<u64>().ok()?, m.parse::<u64>().ok()?))) {
        None => "⚠️".to_string(),
        Some((c, m)) => {
            let (channel, message) = (serenity::ChannelId::new(c), serenity::MessageId::new(m));
            if after.trim() == "0" {
                let had = cancel(data, message).await;
                match (had, ru) {
                    (true, true) => "✅ Удаление отменено".into(),
                    (true, false) => "✅ Deletion cancelled".into(),
                    (false, true) => "Это сообщение не стояло на удалении".into(),
                    (false, false) => "This message wasn't scheduled".into(),
                }
            } else {
                match schedule(data, channel, message, &after, &mi.user.id.to_string()).await {
                    Ok(at) => {
                        println!("[Expire] {} → {} will be deleted in {}", mi.user.name, message, after.trim());
                        if ru { format!("🗑 Сообщение удалится <t:{}:R> (<t:{}:f>)", at, at) } else { format!("🗑 The message will be deleted <t:{}:R> (<t:{}:f>)", at, at) }
                    }
                    Err(e) => format!("⚠️ {}", e),
                }
            }
        }
    };
    let reply = serenity::CreateInteractionResponseMessage::new().content(text).ephemeral(true);
    if let Err(e) = mi.create_response(&ctx.http, serenity::CreateInteractionResponse::Message(reply)).await {
        eprintln!("[Expire] reply: {}", e);
    }
}

#[derive(Deserialize)]
struct Due {
    id: surrealdb::RecordId,
    channel: String,
}

#[derive(Deserialize)]
struct QueueItem {
    channel: String,
    message: String,
    after: String,
}

/// Script requests: *.json files {channel, message, after} in the queue folder → schedule; the file is deleted
async fn read_queue(data: &Data, dir: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let item: Result<QueueItem, String> = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()));
        match item {
            Ok(q) => match (q.channel.parse::<u64>(), q.message.parse::<u64>()) {
                (Ok(c), Ok(m)) => match schedule(data, serenity::ChannelId::new(c), serenity::MessageId::new(m), &q.after, "script").await {
                    Ok(at) => println!("[Expire] from queue: {} will be deleted <t:{}>", q.message, at),
                    Err(e) => eprintln!("[Expire] {}: {}", path.display(), e),
                },
                _ => eprintln!("[Expire] {}: channel and message must be numbers", path.display()),
            },
            Err(e) => eprintln!("[Expire] {}: {}", path.display(), e),
        }
        let _ = std::fs::remove_file(&path);
    }
}

pub fn spawn(ctx: serenity::Context, data: Data) {
    let Some(cfg) = &data.cfg.expire else { return };
    let queue = cfg.queue.clone();
    let _ = std::fs::create_dir_all(&queue);
    println!("[Expire] timed deletion, request queue: {}", queue);
    tokio::spawn(async move {
        loop {
            read_queue(&data, &queue).await;
            let due: Vec<Due> = data
                .db
                .db
                .query("DELETE expire_msg WHERE at <= time::now() RETURN BEFORE")
                .await
                .ok()
                .and_then(|mut r| r.take(0).ok())
                .unwrap_or_default();
            for d in due {
                let message = d.id.key().to_string().trim_matches(|c| c == '⟨' || c == '⟩' || c == '`').to_string();
                let (Ok(c), Ok(m)) = (d.channel.parse::<u64>(), message.parse::<u64>()) else { continue };
                // a thread opened from a message has the same id
                let _ = serenity::ChannelId::new(m).delete(&ctx.http).await;
                match serenity::ChannelId::new(c).delete_message(&ctx.http, serenity::MessageId::new(m)).await {
                    Ok(_) => println!("[Expire] deleted message {}", m),
                    Err(e) => eprintln!("[Expire] deleting {}: {}", m, e),
                }
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::modules::testing;
    use poise::serenity_prelude as serenity;

    #[tokio::test]
    async fn schedule_validates_and_due_rows_are_taken_once() {
        let (data, dir) = testing::data("expire", "").await;
        let (c, m) = (serenity::ChannelId::new(1), serenity::MessageId::new(2));
        assert!(super::schedule(&data, c, m, "30s", "t").await.is_err());
        assert!(super::schedule(&data, c, m, "400d", "t").await.is_err());
        assert!(super::schedule(&data, c, m, "nonsense", "t").await.is_err());
        let at = super::schedule(&data, c, m, "7d", "t").await.unwrap();
        assert!(at > 0);
        // an expired record is claimed only once
        data.db.db.query("UPDATE expire_msg:⟨2⟩ SET at = time::now() - 1s").await.unwrap();
        let q = "DELETE expire_msg WHERE at <= time::now() RETURN BEFORE";
        let first: Vec<super::Due> = data.db.db.query(q).await.unwrap().take(0).unwrap();
        let second: Vec<super::Due> = data.db.db.query(q).await.unwrap().take(0).unwrap();
        assert_eq!(first.first().map(|d| d.channel.as_str()), Some("1"));
        assert_eq!((first.len(), second.len()), (1, 0));
        // cancel: no record — false
        assert!(!super::cancel(&data, m).await);
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
