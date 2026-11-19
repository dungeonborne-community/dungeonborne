//! Invite tracker. Discord doesn't say which invite a person joined with, so the bot keeps a snapshot
//! of counters (`invite:<code>`) and on join compares it with a fresh list: the code whose `uses` grew
//! (or which disappeared after reaching its limit) is the one that was used.
//! Who brought whom is a graph: `RELATE member:<inviter>->invited->member:<newcomer>`. `/invites` counts over the graph:
//! how many someone brought in total and how many of them are still on the server (`->invited->member[WHERE left_at = NONE]`).
//! The code and the inviter are written to `member`, and the join entry in the mod log shows them (event in mod_log.rs).

use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use surrealdb::RecordId;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE invite SCHEMAFULL;
DEFINE FIELD OVERWRITE uses ON invite TYPE int;
DEFINE FIELD OVERWRITE max_uses ON invite TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE inviter ON invite TYPE option<string>;
DEFINE TABLE OVERWRITE invited TYPE RELATION IN member OUT member SCHEMAFULL;
DEFINE FIELD OVERWRITE code ON invited TYPE string;
DEFINE FIELD OVERWRITE at ON invited TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX OVERWRITE idx_invited_pair ON invited COLUMNS in, out UNIQUE;
"#;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Snap {
    pub code: String,
    pub uses: i64,
    pub max_uses: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inviter: Option<String>,
}

impl From<&serenity::RichInvite> for Snap {
    fn from(i: &serenity::RichInvite) -> Self {
        Self { code: i.code.clone(), uses: i.uses as i64, max_uses: i.max_uses as i64, inviter: i.inviter.as_ref().map(|u| u.id.to_string()) }
    }
}

/// Which invite was used: uses grew, or the code disappeared after reaching max_uses. Ambiguous — None.
pub fn detect(old: &[Snap], fresh: &[Snap]) -> Option<Snap> {
    let before: HashMap<&str, &Snap> = old.iter().map(|s| (s.code.as_str(), s)).collect();
    let grew: Vec<&Snap> = fresh
        .iter()
        .filter(|f| before.get(f.code.as_str()).map_or(f.uses > 0, |b| f.uses > b.uses))
        .collect();
    if grew.len() == 1 {
        return Some(grew[0].clone());
    }
    if grew.is_empty() {
        let gone: Vec<&Snap> = old
            .iter()
            .filter(|o| o.max_uses > 0 && o.uses + 1 >= o.max_uses && !fresh.iter().any(|f| f.code == o.code))
            .collect();
        if gone.len() == 1 {
            return Some(Snap { uses: gone[0].uses + 1, ..gone[0].clone() });
        }
    }
    None
}

pub async fn load(data: &Data) -> Vec<Snap> {
    match data.db.db.query("SELECT record::id(id) AS code, uses, max_uses, inviter FROM invite").await {
        Ok(mut r) => r.take(0).unwrap_or_default(),
        Err(e) => {
            eprintln!("[Invites] snapshot: {}", e);
            Vec::new()
        }
    }
}

/// Replace the snapshot with a fresh list
pub async fn store(data: &Data, fresh: &[Snap]) {
    let codes: Vec<String> = fresh.iter().map(|s| s.code.clone()).collect();
    let q = "FOR $s IN $snaps { UPSERT type::thing('invite', $s.code) SET uses = $s.uses, max_uses = $s.max_uses, inviter = $s.inviter; };
             DELETE invite WHERE record::id(id) NOTINSIDE $codes;";
    let r = data.db.db.query(q).bind(("snaps", fresh.to_vec())).bind(("codes", codes)).await.and_then(|r| r.check());
    if let Err(e) = r {
        eprintln!("[Invites] saving snapshot: {}", e);
    }
}

async fn fetch(ctx: &serenity::Context, data: &Data) -> Option<Vec<Snap>> {
    match data.guild.invites(&ctx.http).await {
        Ok(list) => Some(list.iter().map(Snap::from).collect()),
        Err(e) => {
            eprintln!("[Invites] invite list (needs the Manage Server permission): {}", e);
            None
        }
    }
}

pub async fn snapshot(ctx: &serenity::Context, data: &Data) {
    if let Some(fresh) = fetch(ctx, data).await {
        store(data, &fresh).await;
        println!("[Invites] invites in snapshot: {}", fresh.len());
    }
}

pub async fn on_create(data: &Data, ev: &serenity::InviteCreateEvent) {
    let snap = Snap { code: ev.code.clone(), uses: 0, max_uses: ev.max_uses as i64, inviter: ev.inviter.as_ref().map(|u| u.id.to_string()) };
    let r = data
        .db
        .db
        .query("UPSERT type::thing('invite', $s.code) SET uses = $s.uses, max_uses = $s.max_uses, inviter = $s.inviter")
        .bind(("s", snap))
        .await;
    if let Err(e) = r.and_then(|r| r.check()) {
        eprintln!("[Invites] new invite {}: {}", ev.code, e);
    }
}

/// Join: determine the invite, write the link into the graph. Returns (code, inviter) for the log.
pub async fn on_join(ctx: &serenity::Context, data: &Data, user: &serenity::User) -> Option<Snap> {
    let old = load(data).await;
    let fresh = fetch(ctx, data).await?;
    store(data, &fresh).await;
    let used = detect(&old, &fresh)?;
    if let Some(inviter) = &used.inviter {
        relate(data, inviter, &user.id.to_string(), &used.code).await;
    }
    Some(used)
}

pub async fn relate(data: &Data, inviter: &str, newcomer: &str, code: &str) {
    let r = data
        .db
        .db
        .query("RELATE $from->invited->$to SET code = $code")
        .bind(("from", RecordId::from_table_key("member", inviter)))
        .bind(("to", RecordId::from_table_key("member", newcomer)))
        .bind(("code", code.to_string()))
        .await
        .and_then(|r| r.check());
    // a repeat join via the same person's invite hits the unique index — the link already exists
    if let Err(e) = r {
        if !e.to_string().contains("already contains") {
            eprintln!("[Invites] link {}→{}: {}", inviter, newcomer, e);
        }
    }
}

#[derive(Deserialize, Default, Debug)]
pub struct Stats {
    pub total: i64,
    pub stayed: i64,
}

/// Graph traversal from a record ID: the inviter may have no member record of their own (joined before the bot)
pub async fn stats(data: &Data, user: &str) -> Stats {
    let q = "RETURN { total: count($m->invited), stayed: count($m->invited->(member WHERE left_at = NONE)) }";
    match data.db.db.query(q).bind(("m", RecordId::from_table_key("member", user))).await {
        Ok(mut r) => r.take::<Option<Stats>>(0).ok().flatten().unwrap_or_default(),
        Err(e) => {
            eprintln!("[Invites] stats {}: {}", user, e);
            Stats::default()
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct Top {
    pub inviter: String,
    pub total: i64,
}

pub async fn top(data: &Data) -> Vec<Top> {
    let q = "SELECT record::id(in) AS inviter, count() AS total FROM invited GROUP BY inviter ORDER BY total DESC LIMIT 10";
    match data.db.db.query(q).await {
        Ok(mut r) => r.take(0).unwrap_or_default(),
        Err(e) => {
            eprintln!("[Invites] top: {}", e);
            Vec::new()
        }
    }
}

/// Кто сколько людей привёл на сервер
#[poise::command(slash_command, guild_only)]
pub async fn invites(ctx: Context<'_>, #[description = "Whose invites (empty — top 10)"] user: Option<serenity::User>) -> Result<(), Error> {
    let data = ctx.data();
    let embed = match user {
        Some(u) => {
            let s = stats(data, &u.id.to_string()).await;
            serenity::CreateEmbed::new()
                .title(format!("📨 {}", u.name))
                .description(format!("Invited / Привёл: **{}**\nStill here / Остались: **{}**", s.total, s.stayed))
        }
        None => {
            let lines: Vec<String> =
                top(data).await.iter().enumerate().map(|(i, t)| format!("**{}.** <@{}> — {}", i + 1, t.inviter, t.total)).collect();
            serenity::CreateEmbed::new()
                .title("📨 Top inviters / Лучшие пригласившие")
                .description(if lines.is_empty() { "—".into() } else { lines.join("\n") })
        }
    };
    ctx.send(poise::CreateReply::default().embed(embed.color(0x5865F2)).allowed_mentions(serenity::CreateAllowedMentions::new()))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::testing;

    fn s(code: &str, uses: i64, max: i64, by: Option<&str>) -> Snap {
        Snap { code: code.into(), uses, max_uses: max, inviter: by.map(Into::into) }
    }

    #[test]
    fn detect_used_invite() {
        let old = vec![s("a", 1, 0, Some("1")), s("b", 4, 5, Some("2"))];
        assert_eq!(detect(&old, &[s("a", 2, 0, Some("1")), s("b", 4, 5, Some("2"))]).unwrap().code, "a");
        // b reached its limit and disappeared from the list
        assert_eq!(detect(&old, &[s("a", 1, 0, Some("1"))]).unwrap().code, "b");
        // a new code that wasn't in the snapshot, already used
        assert_eq!(detect(&old, &[s("a", 1, 0, None), s("b", 4, 5, None), s("c", 1, 0, Some("3"))]).unwrap().code, "c");
        // nothing changed (vanity URL) or two changed — don't guess
        assert!(detect(&old, &old).is_none());
        assert!(detect(&old, &[s("a", 2, 0, None), s("b", 5, 5, None)]).is_none());
    }

    /// Snapshot in the database, invited graph, stats accounting for those who left, top, a repeat link isn't duplicated
    #[tokio::test]
    async fn graph_stats() {
        let (data, dir) = testing::data("invites", "").await;
        store(&data, &[s("a", 1, 0, Some("1")), s("b", 0, 3, None)]).await;
        let mut snap = load(&data).await;
        snap.sort_by(|x, y| x.code.cmp(&y.code));
        assert_eq!(snap, vec![s("a", 1, 0, Some("1")), s("b", 0, 3, None)]);
        store(&data, &[s("a", 2, 0, Some("1"))]).await;
        assert_eq!(load(&data).await, vec![s("a", 2, 0, Some("1"))], "disappeared codes are removed");

        for (who, name) in [("10", "Аня"), ("11", "Боря"), ("12", "Вова")] {
            crate::modules::mod_log::member_join(&data, &testing::user(who.parse().unwrap(), name), None).await;
        }
        relate(&data, "1", "10", "a").await;
        relate(&data, "1", "11", "a").await;
        relate(&data, "1", "11", "a").await; // repeat join
        relate(&data, "2", "12", "b").await;
        crate::modules::mod_log::member_leave(&data, &testing::user(11, "Боря")).await;

        let st = stats(&data, "1").await;
        assert_eq!((st.total, st.stayed), (2, 1));
        assert_eq!(stats(&data, "999").await.total, 0);
        let t = top(&data).await;
        assert_eq!(t.iter().map(|t| (t.inviter.as_str(), t.total)).collect::<Vec<_>>(), vec![("1", 2), ("2", 1)]);
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
