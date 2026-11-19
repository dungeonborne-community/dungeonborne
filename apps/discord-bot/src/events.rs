//! Discord events → modules. Anything not from our server is ignored.

use crate::modules::{announce, chatmodes, creators, events, expire, poll, media_only, move_forum, giveaways, invites, mod_log, roles, starboard, temp_voice, tickets, translate, welcome};
use crate::{Data, Error};
use poise::serenity_prelude as serenity;
use serenity::FullEvent;

pub async fn handle(fw: poise::FrameworkContext<'_, Data, Error>, event: &FullEvent) -> Result<(), Error> {
    let ctx = fw.serenity_context;
    let data = fw.user_data;
    let ours = |g: Option<serenity::GuildId>| g == Some(data.guild);
    let logging = data.cfg.mod_log.is_some();

    match event {
        FullEvent::GuildMemberAddition { new_member } if ours(Some(new_member.guild_id)) => {
            let invite = if data.cfg.invites.is_some() { invites::on_join(ctx, data, &new_member.user).await } else { None };
            if logging || invite.is_some() {
                mod_log::member_join(data, &new_member.user, invite).await;
            }
        }
        FullEvent::GuildMemberRemoval { guild_id, user, .. } if ours(Some(*guild_id)) && logging => {
            mod_log::member_leave(data, user).await;
        }
        FullEvent::GuildMemberUpdate { old_if_available, event, .. } if ours(Some(event.guild_id)) => {
            welcome::on_roles(ctx, data, event).await;
            if logging {
                mod_log::timeout(data, old_if_available.as_ref(), event).await;
            }
        }
        FullEvent::GuildBanAddition { guild_id, banned_user } if ours(Some(*guild_id)) => {
            mod_log::ban(data, banned_user, true).await;
        }
        FullEvent::GuildBanRemoval { guild_id, unbanned_user } if ours(Some(*guild_id)) => {
            mod_log::ban(data, unbanned_user, false).await;
        }
        FullEvent::Message { new_message } if ours(new_message.guild_id) => {
            if data.cfg.media_only.is_some() {
                media_only::on_message(ctx, data, new_message).await;
            }
            if data.cfg.chatmodes.is_some() {
                chatmodes::on_message(ctx, data, new_message).await;
            }
        }
        FullEvent::MessageDelete { channel_id, deleted_message_id, guild_id } if ours(*guild_id) => {
            if logging {
                mod_log::message_delete(ctx, data, *channel_id, *deleted_message_id).await;
            }
            announce::on_delete(ctx, data, *channel_id, *deleted_message_id).await;
        }
        FullEvent::MessageUpdate { old_if_available, event, .. } if ours(event.guild_id) && logging => {
            mod_log::message_edit(data, old_if_available.as_ref(), event).await;
        }
        FullEvent::VoiceStateUpdate { old, new } if ours(new.guild_id) => {
            temp_voice::on_voice(ctx, data, old.as_ref(), new).await;
        }
        FullEvent::InviteCreate { data: ev } if ours(ev.guild_id) && data.cfg.invites.is_some() => {
            invites::on_create(data, ev).await;
        }
        FullEvent::ReactionAdd { add_reaction: r } | FullEvent::ReactionRemove { removed_reaction: r } if ours(r.guild_id) => {
            starboard::on_reaction(ctx, data, r).await;
        }
        FullEvent::InteractionCreate { interaction: serenity::Interaction::Component(mci) } if ours(mci.guild_id) => {
            match mci.data.custom_id.split(':').next() {
                Some("rp") => roles::on_component(ctx, data, mci).await,
                Some("tk") => tickets::on_component(ctx, data, mci).await,
                Some("gw") => giveaways::on_component(ctx, data, mci).await,
                Some("cr") => creators::on_component(ctx, data, mci).await,
                Some("tr") => translate::on_component(ctx, data, mci).await,
                Some("an") => announce::on_component(ctx, data, mci).await,
                Some("ev") => events::on_component(ctx, data, mci).await,
                _ => {}
            }
        }
        FullEvent::InteractionCreate { interaction: serenity::Interaction::Modal(mi) } if ours(mi.guild_id) => {
            match mi.data.custom_id.split(':').next() {
                Some("tk") => tickets::on_modal(ctx, data, mi).await,
                Some("cr") => creators::on_modal(ctx, data, mi).await,
                Some("an") => announce::on_modal(ctx, data, mi).await,
                Some("mv") => move_forum::on_modal(ctx, data, mi).await,
                Some("ex") => expire::on_modal(ctx, data, mi).await,
                Some("evt") => events::on_modal(ctx, data, mi).await,
                Some("pl") => poll::on_modal(ctx, data, mi).await,
                _ => {}
            }
        }
        _ => {}
    }
    Ok(())
}
