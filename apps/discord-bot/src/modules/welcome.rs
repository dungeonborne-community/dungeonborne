//! Greeting after onboarding: a newcomer finished onboarding (greet without a role) or got a role from `[[welcome.greet]]`
//! (e.g. a language role) → the bot greets them once in the right channel. "Once" is stored in the database: `member.greeted` (set<string>);
//! `UPSERT … SET greeted += $role RETURN BEFORE` atomically marks the role and shows whether it was marked before.
//! Old-timers are not greeted: only those who joined the server less than a day ago.

use crate::Data;
use poise::serenity_prelude as serenity;
use serde::Deserialize;

const NEWCOMER_SECS: i64 = 24 * 3600;
/// Key in member.greeted for the greeting without a role (after onboarding)
const ONBOARDING_KEY: &str = "@onboarding";

#[derive(Deserialize)]
struct Before {
    #[serde(default)]
    greeted: Vec<String>,
}

pub async fn on_roles(ctx: &serenity::Context, data: &Data, event: &serenity::GuildMemberUpdateEvent) {
    let Some(cfg) = &data.cfg.welcome else { return };
    let newcomer = serenity::Timestamp::now().unix_timestamp() - event.joined_at.unix_timestamp() < NEWCOMER_SECS;
    // pending — hasn't accepted the rules on join yet: can't write in chat, so we greet later
    if event.user.bot || !newcomer || event.pending {
        return;
    }
    let onboarded = event.flags.is_some_and(|f| f.contains(serenity::GuildMemberFlags::COMPLETED_ONBOARDING));
    for g in &cfg.greet {
        let (ready, key) = match &g.role {
            Some(name) => (data.names.role(name).is_some_and(|r| event.roles.contains(&r)), name.as_str()),
            None => (onboarded, ONBOARDING_KEY),
        };
        if !ready || !claim(data, event.user.id, key).await {
            continue;
        }
        let Some(channel) = data.names.need_channel(&g.channel, "Welcome") else { continue };
        let text = data.names.render(&g.text, Some(event.user.id), &data.server_name);
        let msg = serenity::CreateMessage::new()
            .content(text)
            .allowed_mentions(serenity::CreateAllowedMentions::new().users(vec![event.user.id]));
        match channel.send_message(&ctx.http, msg).await {
            Ok(_) => println!("[Welcome] {} → {} ({})", event.user.name, g.channel, key),
            Err(e) => eprintln!("[Welcome] {}: {}", g.channel, e),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::modules::testing;

    #[tokio::test]
    async fn greet_once_per_role() {
        let (data, dir) = testing::data("greet", "").await;
        let u = poise::serenity_prelude::UserId::new(9);
        assert!(super::claim(&data, u, "RU").await);
        assert!(!super::claim(&data, u, "RU").await);
        assert!(super::claim(&data, u, "EN").await);
        assert!(!super::claim(&data, u, "EN").await);
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// true — the role was just marked (greet), false — already marked or an error
pub async fn claim(data: &Data, user: serenity::UserId, role: &str) -> bool {
    let r = data
        .db
        .db
        .query("UPSERT type::thing('member', $id) SET greeted += $role RETURN BEFORE")
        .bind(("id", user.to_string()))
        .bind(("role", role.to_string()))
        .await;
    match r {
        Ok(mut r) => match r.take::<Vec<Option<Before>>>(0) {
            Ok(before) => !before.into_iter().flatten().any(|b| b.greeted.iter().any(|g| g == role)),
            Err(e) => {
                eprintln!("[Welcome] greeted {}: {}", user, e);
                false
            }
        },
        Err(e) => {
            eprintln!("[Welcome] greeted {}: {}", user, e);
            false
        }
    }
}
