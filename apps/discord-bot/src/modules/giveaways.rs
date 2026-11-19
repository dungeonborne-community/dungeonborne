//! Giveaways: `/giveaway start` posts a message with a "Join" button; pressing it again leaves.
//! Entries are a `member->entered->giveaway` graph (unique pair); the database parses the duration itself (`<duration>`).
//! Every 15 s `UPDATE … SET status = 'drawing' WHERE status = 'open' AND ends_at <= time::now()` atomically claims
//! finished ones; winners are picked with `ORDER BY rand()`. `/giveaway end` ends early, `/giveaway reroll`
//! re-picks without the previous winners.

use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use std::time::Duration;
use surrealdb::RecordId;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE giveaway SCHEMAFULL;
DEFINE FIELD OVERWRITE prize ON giveaway TYPE string;
DEFINE FIELD OVERWRITE winners ON giveaway TYPE int ASSERT $value >= 1 AND $value <= 20;
DEFINE FIELD OVERWRITE channel ON giveaway TYPE string;
DEFINE FIELD OVERWRITE host ON giveaway TYPE string;
DEFINE FIELD OVERWRITE ends_at ON giveaway TYPE datetime;
DEFINE FIELD OVERWRITE status ON giveaway TYPE string DEFAULT "open" ASSERT $value INSIDE ["open", "drawing", "ended"];
DEFINE FIELD OVERWRITE result ON giveaway TYPE array<string> DEFAULT [];
DEFINE INDEX OVERWRITE idx_giveaway_status ON giveaway COLUMNS status, ends_at;
DEFINE TABLE OVERWRITE entered TYPE RELATION IN member OUT giveaway SCHEMAFULL;
DEFINE FIELD OVERWRITE at ON entered TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX OVERWRITE idx_entered_pair ON entered COLUMNS in, out UNIQUE;
"#;

fn gid(message: &str) -> RecordId {
    RecordId::from_table_key("giveaway", message)
}

fn mid(user: &str) -> RecordId {
    RecordId::from_table_key("member", user)
}

#[derive(Deserialize, Debug, Clone)]
pub struct Giveaway {
    pub key: String,
    pub prize: String,
    pub winners: i64,
    pub channel: String,
    pub host: String,
    pub ends: i64,
    pub status: String,
    pub result: Vec<String>,
}

const FIELDS: &str = "record::id(id) AS key, prize, winners, channel, host, time::unix(ends_at) AS ends, status, result";

/// A user-provided duration ("1h30m", "2d") is parsed by SurrealDB; from 1 minute to 30 days.
/// The check is a separate query: statements of one query aren't transactional, a THROW in the first wouldn't undo the CREATE.
pub async fn create(data: &Data, message: &str, prize: &str, winners: i64, channel: &str, host: &str, duration: &str) -> Result<i64, Error> {
    let ok: Option<bool> = data
        .db
        .db
        .query("RETURN <duration>$d >= 1m AND <duration>$d <= 30d")
        .bind(("d", duration.to_string()))
        .await?
        .check()?
        .take(0)?;
    if ok != Some(true) {
        return Err("длительность от 1m до 30d (например 30m, 2h, 1d)".into());
    }
    let q = "CREATE type::thing('giveaway', $m) SET prize = $prize, winners = $w, channel = $c, host = $h,
             ends_at = time::now() + <duration>$d RETURN VALUE time::unix(ends_at);";
    let mut r = data
        .db
        .db
        .query(q)
        .bind(("d", duration.to_string()))
        .bind(("m", message.to_string()))
        .bind(("prize", prize.to_string()))
        .bind(("w", winners))
        .bind(("c", channel.to_string()))
        .bind(("h", host.to_string()))
        .await?
        .check()?;
    let ends: Vec<i64> = r.take(0)?;
    ends.into_iter().next().ok_or_else(|| "розыгрыш не создан".into())
}

/// Button press: join or leave. Returns (participating now, number of participants). None — the giveaway is closed.
pub async fn toggle(data: &Data, message: &str, user: &str) -> Result<Option<(bool, i64)>, Error> {
    let q = "LET $open = (SELECT VALUE status FROM ONLY $g) = 'open';
             LET $was = count(SELECT id FROM entered WHERE in = $m AND out = $g) > 0;
             IF $open AND $was { DELETE entered WHERE in = $m AND out = $g }
             ELSE IF $open { RELATE $m->entered->$g };
             RETURN { open: $open, joined: $open AND !$was, total: count($g<-entered) };";
    #[derive(Deserialize)]
    struct Res {
        open: bool,
        joined: bool,
        total: i64,
    }
    let mut r = data.db.db.query(q).bind(("g", gid(message))).bind(("m", mid(user))).await?.check()?;
    let res: Option<Res> = r.take(3)?;
    Ok(res.filter(|r| r.open).map(|r| (r.joined, r.total)))
}

/// Claim finished ones (open → drawing exactly once)
pub async fn take_ended(data: &Data) -> Vec<Giveaway> {
    let q = format!("UPDATE giveaway SET status = 'drawing' WHERE status = 'open' AND ends_at <= time::now() RETURN {}", FIELDS);
    match data.db.db.query(q).await {
        Ok(mut r) => r.take(0).unwrap_or_default(),
        Err(e) => {
            eprintln!("[Giveaway] {}", e);
            Vec::new()
        }
    }
}

/// Random winners excluding `exclude`; the giveaway is marked ended, winners are appended to result
pub async fn draw(data: &Data, message: &str, count: i64, exclude: &[String]) -> Result<Vec<String>, Error> {
    let q = "LET $w = (SELECT VALUE record::id(in) FROM entered WHERE out = $g AND record::id(in) NOTINSIDE $ex ORDER BY rand() LIMIT $n);
             UPDATE $g SET status = 'ended', result = array::union(result, $w);
             RETURN $w;";
    let mut r = data
        .db
        .db
        .query(q)
        .bind(("g", gid(message)))
        .bind(("ex", exclude.to_vec()))
        .bind(("n", count))
        .await?
        .check()?;
    Ok(r.take::<Vec<String>>(2)?)
}

pub async fn get(data: &Data, message: &str) -> Option<Giveaway> {
    let mut r = data.db.db.query(format!("SELECT {} FROM ONLY $g", FIELDS)).bind(("g", gid(message))).await.ok()?;
    r.take::<Option<Giveaway>>(0).ok()?
}

fn button(total: i64, open: bool) -> serenity::CreateActionRow {
    serenity::CreateActionRow::Buttons(vec![serenity::CreateButton::new("gw:join")
        .label(format!("Join / Участвовать ({})", total))
        .emoji('🎉')
        .style(serenity::ButtonStyle::Success)
        .disabled(!open)])
}

fn embed(g: &Giveaway, winners: &[String]) -> serenity::CreateEmbed {
    let ended = g.status == "ended";
    let mut lines = vec![
        format!("🏆 **{}**", g.prize),
        format!("Winners / Победителей: **{}**", g.winners),
        format!("Host / Ведущий: <@{}>", g.host),
    ];
    if ended {
        let list = if winners.is_empty() { "—".into() } else { winners.iter().map(|w| format!("<@{}>", w)).collect::<Vec<_>>().join(", ") };
        lines.push(format!("🎉 Winners / Победители: {}", list));
    } else {
        lines.push(format!("⏰ Ends / Конец: <t:{}:R>", g.ends));
    }
    serenity::CreateEmbed::new()
        .title(if ended { "🎁 Giveaway ended / Розыгрыш завершён" } else { "🎁 Giveaway / Розыгрыш" })
        .description(lines.join("\n"))
        .color(if ended { 0x95A5A6 } else { 0xE91E63 })
}

#[poise::command(slash_command, guild_only, subcommands("start", "end", "reroll"), subcommand_required, default_member_permissions = "MANAGE_EVENTS")]
pub async fn giveaway(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Начать розыгрыш в этом канале
#[poise::command(slash_command, guild_only)]
async fn start(
    ctx: Context<'_>,
    #[description = "Prize / Приз"] prize: String,
    #[description = "Duration: 30m, 2h, 1d / Длительность"] duration: String,
    #[description = "Number of winners / Победителей"]
    #[min = 1]
    #[max = 20]
    winners: Option<i64>,
) -> Result<(), Error> {
    let data = ctx.data();
    let winners = winners.unwrap_or(1);
    let host = ctx.author().id.to_string();
    let preview = Giveaway {
        key: String::new(),
        prize: prize.clone(),
        winners,
        channel: ctx.channel_id().to_string(),
        host: host.clone(),
        ends: 0,
        status: "open".into(),
        result: vec![],
    };
    // the message is needed before the record: its ID is the giveaway key; on a duration error the message is deleted
    let msg = ctx
        .channel_id()
        .send_message(ctx.http(), serenity::CreateMessage::new().embed(embed(&preview, &[])).components(vec![button(0, true)]))
        .await?;
    match create(data, &msg.id.to_string(), &prize, winners, &ctx.channel_id().to_string(), &host, &duration).await {
        Ok(ends) => {
            let g = Giveaway { ends, ..preview };
            let _ = ctx.channel_id().edit_message(ctx.http(), msg.id, serenity::EditMessage::new().embed(embed(&g, &[]))).await;
            ctx.send(poise::CreateReply::default().content("✅").ephemeral(true)).await?;
        }
        Err(e) => {
            let _ = msg.delete(ctx.http()).await;
            return Err(format!("длительность '{}': {}", duration, e).into());
        }
    }
    Ok(())
}

/// Завершить розыгрыш досрочно
#[poise::command(slash_command, guild_only)]
async fn end(ctx: Context<'_>, #[description = "Giveaway message ID"] message_id: String) -> Result<(), Error> {
    let r = ctx
        .data()
        .db
        .db
        .query("UPDATE $g SET ends_at = time::now() WHERE status = 'open' RETURN VALUE status")
        .bind(("g", gid(message_id.trim())))
        .await?
        .take::<Vec<String>>(0)?;
    let text = if r.is_empty() { "⚠️ not found or already ended / не найден или уже завершён" } else { "✅ ≤15 s" };
    ctx.send(poise::CreateReply::default().content(text).ephemeral(true)).await?;
    Ok(())
}

/// Перевыбрать победителей (без прежних)
#[poise::command(slash_command, guild_only)]
async fn reroll(
    ctx: Context<'_>,
    #[description = "Giveaway message ID"] message_id: String,
    #[description = "How many / Сколько"]
    #[min = 1]
    #[max = 20]
    count: Option<i64>,
) -> Result<(), Error> {
    let data = ctx.data();
    let g = get(data, message_id.trim()).await.filter(|g| g.status == "ended").ok_or("розыгрыш не найден или ещё идёт")?;
    let new = draw(data, &g.key, count.unwrap_or(1), &g.result).await?;
    let text = if new.is_empty() {
        "No more participants / Больше некого выбрать".into()
    } else {
        format!("🔁 {} — **{}**", new.iter().map(|w| format!("<@{}>", w)).collect::<Vec<_>>().join(", "), g.prize)
    };
    ctx.send(poise::CreateReply::default().content(text)).await?;
    Ok(())
}

pub async fn on_component(ctx: &serenity::Context, data: &Data, mci: &serenity::ComponentInteraction) {
    if mci.data.custom_id != "gw:join" {
        return;
    }
    let ru = mci.locale.starts_with("ru");
    let text = match toggle(data, &mci.message.id.to_string(), &mci.user.id.to_string()).await {
        Ok(Some((joined, total))) => {
            let _ = mci
                .message
                .channel_id
                .edit_message(&ctx.http, mci.message.id, serenity::EditMessage::new().components(vec![button(total, true)]))
                .await;
            match (joined, ru) {
                (true, true) => "🎉 Ты участвуешь! Нажми ещё раз, чтобы выйти.",
                (true, false) => "🎉 You're in! Click again to leave.",
                (false, true) => "➖ Ты вышел из розыгрыша.",
                (false, false) => "➖ You left the giveaway.",
            }
        }
        Ok(None) => if ru { "⏰ Розыгрыш уже завершён" } else { "⏰ This giveaway has ended" },
        Err(e) => {
            eprintln!("[Giveaway] entry {}: {}", mci.user.name, e);
            "⚠️"
        }
    };
    let reply = serenity::CreateInteractionResponseMessage::new().content(text).ephemeral(true);
    let _ = mci.create_response(&ctx.http, serenity::CreateInteractionResponse::Message(reply)).await;
}

pub fn spawn(ctx: serenity::Context, data: Data) {
    tokio::spawn(async move {
        loop {
            for g in take_ended(&data).await {
                finish(&ctx, &data, g).await;
            }
            tokio::time::sleep(Duration::from_secs(15)).await;
        }
    });
}

async fn finish(ctx: &serenity::Context, data: &Data, g: Giveaway) {
    let winners = match draw(data, &g.key, g.winners, &[]).await {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[Giveaway] giveaway {}: {}", g.key, e);
            return;
        }
    };
    let (Ok(channel), Ok(message)) = (g.channel.parse::<u64>(), g.key.parse::<u64>()) else { return };
    let channel = serenity::ChannelId::new(channel);
    let message = serenity::MessageId::new(message);
    let ended = Giveaway { status: "ended".into(), ..g.clone() };
    let total = data
        .db
        .db
        .query("RETURN count($g<-entered)")
        .bind(("g", gid(&g.key)))
        .await
        .ok()
        .and_then(|mut r| r.take::<Option<i64>>(0).ok().flatten())
        .unwrap_or(0);
    let _ = channel
        .edit_message(&ctx.http, message, serenity::EditMessage::new().embed(embed(&ended, &winners)).components(vec![button(total, false)]))
        .await;
    let text = if winners.is_empty() {
        format!("🎁 **{}** — no participants / нет участников", g.prize)
    } else {
        format!("🎉 {} — **{}**!", winners.iter().map(|w| format!("<@{}>", w)).collect::<Vec<_>>().join(", "), g.prize)
    };
    let reply = serenity::CreateMessage::new().content(text).reference_message((channel, message));
    if let Err(e) = channel.send_message(&ctx.http, reply).await {
        eprintln!("[Giveaway] announcement {}: {}", g.key, e);
    }
    println!("[Giveaway] '{}' ended, winners: {}", g.prize, winners.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::testing;

    #[tokio::test]
    async fn lifecycle() {
        let (data, dir) = testing::data("giveaway", "").await;
        assert!(create(&data, "1", "x", 1, "10", "5", "10s").await.is_err(), "under a minute is not allowed");
        assert!(create(&data, "1", "x", 1, "10", "5", "мусор").await.is_err());
        let ends = create(&data, "1", "Ключ", 2, "10", "5", "1h").await.unwrap();
        assert!((ends - serenity::Timestamp::now().unix_timestamp() - 3600).abs() < 60);

        assert_eq!(toggle(&data, "1", "a").await.unwrap(), Some((true, 1)));
        assert_eq!(toggle(&data, "1", "b").await.unwrap(), Some((true, 2)));
        assert_eq!(toggle(&data, "1", "c").await.unwrap(), Some((true, 3)));
        assert_eq!(toggle(&data, "1", "c").await.unwrap(), Some((false, 2)), "pressing again leaves");
        assert_eq!(toggle(&data, "404", "a").await.unwrap(), None);

        assert!(take_ended(&data).await.is_empty(), "still running");
        data.db.db.query("UPDATE giveaway:⟨1⟩ SET ends_at = time::now() - 1s").await.unwrap();
        let ended = take_ended(&data).await;
        assert_eq!(ended.len(), 1);
        assert!(take_ended(&data).await.is_empty(), "claimed only once");
        assert_eq!(toggle(&data, "1", "d").await.unwrap(), None, "can't join after it ends");

        let mut w = draw(&data, "1", 2, &[]).await.unwrap();
        w.sort();
        assert_eq!(w, vec!["a", "b"]);
        assert!(draw(&data, "1", 1, &w).await.unwrap().is_empty(), "reroll without previous winners — nobody left");
        let g = get(&data, "1").await.unwrap();
        assert_eq!((g.status.as_str(), g.result.len()), ("ended", 2));
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
