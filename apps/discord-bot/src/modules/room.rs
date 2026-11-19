//! `/room` — manage your temporary room (Duo, Trio, Tavern generators): lock and unlock, seat limit,
//! name, kick, hand the room over. Works in the room you are currently in if you own it
//! (record `temp_room:<channel>`) — or if you are a moderator with Move Members.

use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;

/// The room the command author is in, and whether they may manage it
async fn my_room(ctx: Context<'_>) -> Result<serenity::ChannelId, Error> {
    let data = ctx.data();
    let ru = ctx.locale().is_some_and(|l| l.starts_with("ru"));
    let user = ctx.author().id;
    let channel = ctx
        .serenity_context()
        .cache
        .guild(data.guild)
        .and_then(|g| g.voice_states.get(&user).and_then(|v| v.channel_id))
        .ok_or(if ru { "зайди в свою голосовую комнату" } else { "join your voice room first" })?;
    let owner: Option<String> = data
        .db
        .db
        .query("SELECT VALUE record::id(owner) FROM ONLY type::thing('temp_room', $ch)")
        .bind(("ch", channel.to_string()))
        .await?
        .take(0)?;
    let owner = owner.ok_or(if ru { "это не временная комната" } else { "this is not a temporary room" })?;
    let is_mod = ctx.author_member().await.and_then(|m| m.permissions).is_some_and(|p| p.move_members() || p.administrator());
    if owner != user.to_string() && !is_mod {
        return Err((if ru { "управлять может только владелец комнаты" } else { "only the room owner can do this" }).into());
    }
    Ok(channel)
}

async fn done(ctx: Context<'_>, en: String, ru_text: String) -> Result<(), Error> {
    let ru = ctx.locale().is_some_and(|l| l.starts_with("ru"));
    ctx.send(poise::CreateReply::default().content(if ru { ru_text } else { en }).ephemeral(true)).await?;
    Ok(())
}

/// Your voice room / Твоя голосовая комната
#[poise::command(slash_command, guild_only, subcommands("lock", "unlock", "limit", "name", "kick", "owner"), subcommand_required)]
pub async fn room(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Close the room to newcomers / Закрыть вход
#[poise::command(slash_command, guild_only)]
async fn lock(ctx: Context<'_>) -> Result<(), Error> {
    let ch = my_room(ctx).await?;
    let everyone = ctx.data().guild.everyone_role();
    ch.create_permission(
        ctx.http(),
        serenity::PermissionOverwrite { allow: serenity::Permissions::empty(), deny: serenity::Permissions::CONNECT, kind: serenity::PermissionOverwriteType::Role(everyone) },
    )
    .await?;
    done(ctx, "🔒 Room locked — those inside stay".into(), "🔒 Вход закрыт — кто внутри, остаются".into()).await
}

/// Open the room again / Открыть вход
#[poise::command(slash_command, guild_only)]
async fn unlock(ctx: Context<'_>) -> Result<(), Error> {
    let ch = my_room(ctx).await?;
    let everyone = ctx.data().guild.everyone_role();
    // restore category permissions for @everyone: remove our connect deny
    ch.delete_permission(ctx.http(), serenity::PermissionOverwriteType::Role(everyone)).await?;
    if let Some(parent) = ctx.serenity_context().cache.guild(ctx.data().guild).and_then(|g| g.channels.get(&ch).and_then(|c| c.parent_id)) {
        let base = ctx
            .serenity_context()
            .cache
            .guild(ctx.data().guild)
            .and_then(|g| g.channels.get(&parent).map(|c| c.permission_overwrites.clone()))
            .unwrap_or_default();
        if let Some(o) = base.into_iter().find(|o| o.kind == serenity::PermissionOverwriteType::Role(everyone)) {
            ch.create_permission(ctx.http(), o).await?;
        }
    }
    done(ctx, "🔓 Room open".into(), "🔓 Вход открыт".into()).await
}

/// Seats in the room, 0 = no limit / Мест в комнате, 0 — без лимита
#[poise::command(slash_command, guild_only)]
async fn limit(
    ctx: Context<'_>,
    #[description = "0–99"]
    #[min = 0]
    #[max = 99]
    seats: u32,
) -> Result<(), Error> {
    let ch = my_room(ctx).await?;
    ch.edit(ctx.http(), serenity::EditChannel::new().user_limit(seats)).await?;
    done(ctx, format!("👥 Seats: {}", if seats == 0 { "no limit".into() } else { seats.to_string() }), format!("👥 Мест: {}", if seats == 0 { "без лимита".into() } else { seats.to_string() })).await
}

/// Rename the room / Переименовать комнату
#[poise::command(slash_command, guild_only)]
async fn name(ctx: Context<'_>, #[description = "New name / Новое название"] #[max_length = 90] text: String) -> Result<(), Error> {
    let ch = my_room(ctx).await?;
    ch.edit(ctx.http(), serenity::EditChannel::new().name(text.trim())).await?;
    done(ctx, "✏️ Renamed".into(), "✏️ Переименовано".into()).await
}

/// Disconnect someone from your room / Выгнать из комнаты
#[poise::command(slash_command, guild_only)]
async fn kick(ctx: Context<'_>, #[description = "Who / Кого"] user: serenity::User) -> Result<(), Error> {
    let ch = my_room(ctx).await?;
    let data = ctx.data();
    let inside = ctx.serenity_context().cache.guild(data.guild).is_some_and(|g| g.voice_states.get(&user.id).and_then(|v| v.channel_id) == Some(ch));
    if !inside {
        return done(ctx, "This person isn't in your room".into(), "Его нет в твоей комнате".into()).await;
    }
    data.guild.disconnect_member(ctx.http(), user.id).await?;
    done(ctx, format!("👢 {} disconnected", user.name), format!("👢 {} отключён", user.name)).await
}

/// Give the room to someone inside / Передать комнату
#[poise::command(slash_command, guild_only)]
async fn owner(ctx: Context<'_>, #[description = "New owner / Новый владелец"] user: serenity::User) -> Result<(), Error> {
    let ch = my_room(ctx).await?;
    let data: &Data = ctx.data();
    let inside = ctx.serenity_context().cache.guild(data.guild).is_some_and(|g| g.voice_states.get(&user.id).and_then(|v| v.channel_id) == Some(ch));
    if !inside || user.bot {
        return done(ctx, "The new owner must be in the room".into(), "Новый владелец должен быть в комнате".into()).await;
    }
    data.db
        .db
        .query("UPDATE type::thing('temp_room', $ch) SET owner = type::thing('member', $u)")
        .bind(("ch", ch.to_string()))
        .bind(("u", user.id.to_string()))
        .await?
        .check()?;
    let old = ctx.author().id;
    let _ = ch.delete_permission(ctx.http(), serenity::PermissionOverwriteType::Member(old)).await;
    ch.create_permission(
        ctx.http(),
        serenity::PermissionOverwrite { allow: crate::modules::temp_voice::owner_perms(), deny: serenity::Permissions::empty(), kind: serenity::PermissionOverwriteType::Member(user.id) },
    )
    .await?;
    done(ctx, format!("👑 The room now belongs to {}", user.name), format!("👑 Теперь комната у {}", user.name)).await
}
