//! Scheduled messages from `[[timed]]`. The schedule lives in the database: `every` is a SurrealDB duration (30m, 12h, 1d),
//! `next_at` survives restarts. Every 30 s a single `UPDATE … WHERE next_at <= time::now() RETURN BEFORE` query
//! atomically claims the due entries and immediately moves them to the next time — a message won't be sent twice.
//! `replace_previous` — the previous reminder message is deleted so they don't pile up in the channel.

use crate::Data;
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use std::time::Duration;
use surrealdb::RecordId;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE timed SCHEMAFULL;
DEFINE FIELD OVERWRITE channel ON timed TYPE string;
DEFINE FIELD OVERWRITE text ON timed TYPE string;
DEFINE FIELD OVERWRITE every ON timed TYPE duration ASSERT $value >= 1m;
DEFINE FIELD OVERWRITE next_at ON timed TYPE datetime;
DEFINE FIELD OVERWRITE replace ON timed TYPE bool DEFAULT true;
DEFINE FIELD OVERWRITE last_message ON timed TYPE option<string>;
"#;

/// Config → database. next_at is kept until the interval changes; records removed from the config are deleted.
const SYNC: &str = r#"
UPSERT type::thing('timed', $name) SET
    next_at = IF next_at = NONE OR every != <duration>$every THEN time::now() + <duration>$every ELSE next_at END,
    channel = $channel, text = $text, every = <duration>$every, replace = $replace;
"#;

#[derive(Deserialize, Debug)]
pub struct Due {
    pub id: RecordId,
    pub channel: String,
    pub text: String,
    pub replace: bool,
    pub last_message: Option<String>,
}

pub async fn sync(data: &Data) -> usize {
    let mut names = Vec::new();
    for t in &data.cfg.timed {
        let Some(channel) = data.names.need_channel(&t.channel, "Timed") else { continue };
        let r = data
            .db
            .db
            .query(SYNC)
            .bind(("name", t.name.clone()))
            .bind(("channel", channel.to_string()))
            .bind(("text", t.text.clone()))
            .bind(("every", t.every.clone()))
            .bind(("replace", t.replace_previous))
            .await
            .and_then(|r| r.check());
        match r {
            Ok(_) => names.push(t.name.clone()),
            Err(e) => eprintln!("[Timed] '{}' (every = \"{}\"): {}", t.name, t.every, e),
        }
    }
    let _ = data.db.db.query("DELETE timed WHERE record::id(id) NOTINSIDE $names").bind(("names", names.clone())).await;
    names.len()
}

pub async fn take_due(data: &Data) -> Vec<Due> {
    match data.db.db.query("UPDATE timed SET next_at = time::now() + every WHERE next_at <= time::now() RETURN BEFORE").await {
        Ok(mut r) => r.take(0).unwrap_or_else(|e| {
            eprintln!("[Timed] {}", e);
            Vec::new()
        }),
        Err(e) => {
            eprintln!("[Timed] {}", e);
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::testing;

    const CFG: &str = r#"
[[timed]]
name = "lfg"
channel = "100"
every = "1h"
text = "LFG {server}"
[[timed]]
name = "bad"
channel = "100"
every = "10s"
text = "under a minute — ASSERT will reject it"
"#;

    #[tokio::test]
    async fn schedule_claims_once_and_survives_resync() {
        let (data, dir) = testing::data("timed", CFG).await;
        let _ = data.db.db.query("CREATE timed:old SET channel = '1', text = 'x', every = 1h, next_at = time::now()").await;
        assert_eq!(sync(&data).await, 1, "bad rejected by ASSERT, old deleted");
        assert!(take_due(&data).await.is_empty(), "the first run is after every");

        data.db.db.query("UPDATE timed:lfg SET next_at = time::now() - 1s").await.unwrap();
        let due = take_due(&data).await;
        assert_eq!(due.len(), 1);
        assert_eq!((due[0].channel.as_str(), due[0].text.as_str(), due[0].replace), ("100", "LFG {server}", true));
        assert!(take_due(&data).await.is_empty(), "moved to the next time");

        // a bot restart doesn't reset next_at
        let before: Option<String> = data.db.db.query("SELECT VALUE <string>next_at FROM ONLY timed:lfg").await.unwrap().take(0).unwrap();
        sync(&data).await;
        let after: Option<String> = data.db.db.query("SELECT VALUE <string>next_at FROM ONLY timed:lfg").await.unwrap().take(0).unwrap();
        assert_eq!(before, after);
        let left: Vec<RecordId> = data.db.db.query("SELECT VALUE id FROM timed").await.unwrap().take(0).unwrap();
        assert_eq!(left.len(), 1);
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

pub fn spawn(ctx: serenity::Context, data: Data) {
    tokio::spawn(async move {
        let count = sync(&data).await;
        println!("[Timed] schedules: {}", count);
        loop {
            for due in take_due(&data).await {
                send(&ctx, &data, due).await;
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

async fn send(ctx: &serenity::Context, data: &Data, due: Due) {
    let Ok(channel) = due.channel.parse::<u64>().map(serenity::ChannelId::new) else { return };
    if due.replace {
        if let Some(old) = due.last_message.as_deref().and_then(|m| m.parse::<u64>().ok()) {
            let _ = channel.delete_message(&ctx.http, serenity::MessageId::new(old)).await;
        }
    }
    let text = data.names.render(&due.text, None, &data.server_name);
    match channel.send_message(&ctx.http, serenity::CreateMessage::new().content(text)).await {
        Ok(msg) => {
            let _ = data
                .db
                .db
                .query("UPDATE $id SET last_message = $m")
                .bind(("id", due.id))
                .bind(("m", msg.id.to_string()))
                .await;
        }
        Err(e) => eprintln!("[Timed] {}: {}", due.id, e),
    }
}
