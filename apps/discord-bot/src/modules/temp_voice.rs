//! Temporary voice rooms: join a generator channel (there can be several — Duo, Trio — each with its own seat limit) → the bot creates a room next to it (category permissions + the owner
//! can rename, limit and kick) and moves you there. The last one leaves → the room is deleted.
//! Rooms are `temp_room:<channel id>` records linking to the owner (`record<member>`); deletion is claimed
//! via `DELETE … RETURN BEFORE`, so two leave events won't delete the channel twice. On startup the bot cleans up
//! rooms that emptied while it was offline.

use crate::Data;
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use std::time::Duration;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE temp_room SCHEMAFULL;
DEFINE FIELD OVERWRITE owner ON temp_room TYPE record<member>;
DEFINE FIELD OVERWRITE created_at ON temp_room TYPE datetime DEFAULT time::now() READONLY;
"#;

/// Owner permissions in their room: channel settings (name, limit, bitrate), channel permissions (lock),
/// moving and disconnecting members, muting and deafening others, priority speaker
pub fn owner_perms() -> serenity::Permissions {
    serenity::Permissions::VIEW_CHANNEL
        | serenity::Permissions::CONNECT
        | serenity::Permissions::SPEAK
        | serenity::Permissions::STREAM
        | serenity::Permissions::MANAGE_CHANNELS
        | serenity::Permissions::MANAGE_ROLES
        | serenity::Permissions::MOVE_MEMBERS
        | serenity::Permissions::MUTE_MEMBERS
        | serenity::Permissions::DEAFEN_MEMBERS
        | serenity::Permissions::PRIORITY_SPEAKER
}

fn occupants(ctx: &serenity::Context, guild: serenity::GuildId, channel: serenity::ChannelId) -> Option<usize> {
    let g = ctx.cache.guild(guild)?;
    Some(g.voice_states.values().filter(|v| v.channel_id == Some(channel)).count())
}

pub async fn on_voice(ctx: &serenity::Context, data: &Data, old: Option<&serenity::VoiceState>, new: &serenity::VoiceState) {
    let Some(cfg) = &data.cfg.temp_voice else { return };
    let left = old.and_then(|o| o.channel_id).filter(|c| Some(*c) != new.channel_id);
    if let Some(ch) = left {
        close_if_empty(ctx, data, ch).await;
    }
    let Some(joined) = new.channel_id.filter(|c| Some(*c) != old.and_then(|o| o.channel_id)) else { return };
    let joined_name = ctx.cache.guild(data.guild).and_then(|g| g.channels.get(&joined).map(|c| c.name.clone())).unwrap_or_default();
    let Some(room) = cfg.rooms.iter().find(|r| data.names.channel(&r.generator) == Some(joined) || r.generator == joined_name) else { return };
    if let Some(member) = &new.member {
        if !member.user.bot {
            let name = room.name.as_deref().unwrap_or(&cfg.name);
            open(ctx, data, member, joined, name, room.user_limit).await;
        }
    }
}

async fn open(
    ctx: &serenity::Context,
    data: &Data,
    member: &serenity::Member,
    generator: serenity::ChannelId,
    name: &str,
    user_limit: u32,
) {
    let (parent, mut overwrites) = {
        let Some(g) = ctx.cache.guild(data.guild) else { return };
        let parent = g.channels.get(&generator).and_then(|c| c.parent_id);
        let base = parent.and_then(|p| g.channels.get(&p)).map(|c| c.permission_overwrites.clone()).unwrap_or_default();
        (parent, base)
    };
    overwrites.retain(|o| o.kind != serenity::PermissionOverwriteType::Member(member.user.id));
    overwrites.push(serenity::PermissionOverwrite {
        allow: owner_perms(),
        deny: serenity::Permissions::empty(),
        kind: serenity::PermissionOverwriteType::Member(member.user.id),
    });
    let name = name.replace("{user}", member.display_name());
    let mut builder = serenity::CreateChannel::new(name)
        .kind(serenity::ChannelType::Voice)
        .permissions(overwrites)
        .user_limit(user_limit);
    if let Some(p) = parent {
        builder = builder.category(p);
    }
    let room = match data.guild.create_channel(&ctx.http, builder).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[TempVoice] creating room for {}: {}", member.user.name, e);
            return;
        }
    };
    let saved = data
        .db
        .db
        .query("CREATE type::thing('temp_room', $ch) SET owner = type::thing('member', $owner)")
        .bind(("ch", room.id.to_string()))
        .bind(("owner", member.user.id.to_string()))
        .await
        .and_then(|r| r.check());
    if let Err(e) = saved {
        eprintln!("[TempVoice] room record {}: {}", room.id, e);
    }
    let moved = data.guild.edit_member(&ctx.http, member.user.id, serenity::EditMember::new().voice_channel(room.id)).await;
    match moved {
        Ok(_) => println!("[TempVoice] {} opened '{}'", member.user.name, room.name),
        Err(e) => {
            // the user already left — nobody needs the room
            eprintln!("[TempVoice] move {}: {}", member.user.name, e);
            close_if_empty(ctx, data, room.id).await;
        }
    }
}

/// Delete the room if it is temporary and empty. DELETE … RETURN BEFORE — only one call gets the record.
async fn close_if_empty(ctx: &serenity::Context, data: &Data, channel: serenity::ChannelId) {
    if occupants(ctx, data.guild, channel).unwrap_or(0) > 0 {
        return;
    }
    let r = data
        .db
        .db
        .query("DELETE type::thing('temp_room', $ch) RETURN BEFORE")
        .bind(("ch", channel.to_string()))
        .await;
    let was_room = match r {
        Ok(mut r) => r.take::<Vec<Room>>(0).map(|v| !v.is_empty()).unwrap_or(false),
        Err(e) => {
            eprintln!("[TempVoice] deleting record {}: {}", channel, e);
            false
        }
    };
    if was_room {
        match channel.delete(&ctx.http).await {
            Ok(_) => println!("[TempVoice] room {} closed", channel),
            Err(e) => eprintln!("[TempVoice] deleting channel {}: {}", channel, e),
        }
    }
}

#[derive(Deserialize)]
struct Room {
    #[allow(dead_code)]
    owner: surrealdb::RecordId,
}

#[derive(Deserialize)]
struct RoomKey {
    ch: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::testing;

    /// The owner is a link to member (name via owner.name); DELETE … RETURN BEFORE returns the record once
    #[tokio::test]
    async fn room_link_and_single_delete() {
        let (data, dir) = testing::data("rooms", "").await;
        crate::modules::mod_log::member_join(&data, &testing::user(5, "Петя"), None).await;
        data.db
            .db
            .query("CREATE type::thing('temp_room', '777') SET owner = type::thing('member', '5')")
            .await
            .unwrap()
            .check()
            .unwrap();
        let owner: Option<String> =
            data.db.db.query("SELECT VALUE owner.name FROM ONLY temp_room:⟨777⟩").await.unwrap().take(0).unwrap();
        assert_eq!(owner.as_deref(), Some("Петя"));

        let keys: Vec<RoomKey> = data.db.db.query("SELECT record::id(id) AS ch FROM temp_room").await.unwrap().take(0).unwrap();
        assert_eq!(keys.iter().map(|k| k.ch.as_str()).collect::<Vec<_>>(), vec!["777"]);

        let q = "DELETE type::thing('temp_room', $ch) RETURN BEFORE";
        let first: Vec<Room> = data.db.db.query(q).bind(("ch", "777")).await.unwrap().take(0).unwrap();
        let second: Vec<Room> = data.db.db.query(q).bind(("ch", "777")).await.unwrap().take(0).unwrap();
        assert_eq!((first.len(), second.len()), (1, 0));
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A few seconds after startup (cache filled): close empty rooms and records without a channel
pub fn spawn_cleanup(ctx: serenity::Context, data: Data) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let rooms: Vec<RoomKey> = match data.db.db.query("SELECT record::id(id) AS ch FROM temp_room").await {
            Ok(mut r) => r.take(0).unwrap_or_default(),
            Err(e) => {
                eprintln!("[TempVoice] cleanup: {}", e);
                return;
            }
        };
        for room in rooms {
            let Ok(id) = room.ch.parse::<u64>() else { continue };
            let channel = serenity::ChannelId::new(id);
            let exists = ctx.cache.guild(data.guild).is_some_and(|g| g.channels.contains_key(&channel));
            if exists {
                close_if_empty(&ctx, &data, channel).await;
            } else {
                let _ = data.db.db.query("DELETE type::thing('temp_room', $ch)").bind(("ch", room.ch)).await;
            }
        }
    });
}
