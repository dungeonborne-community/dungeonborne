//! Role panels with buttons: `/rolepanel` posts a panel from `[[role_panel]]`, a button grants or removes the role.
//! Button custom_id is `rp:<role id>`; only roles listed in the config are granted.

use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;

async fn panel_names(ctx: Context<'_>, partial: &str) -> serenity::CreateAutocompleteResponse {
    let choices = ctx
        .data()
        .cfg
        .role_panel
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&partial.to_lowercase()))
        .map(|p| serenity::AutocompleteChoice::new(p.name.clone(), p.name.clone()))
        .collect();
    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

/// Опубликовать панель ролей из bot.toml
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_ROLES")]
pub async fn rolepanel(
    ctx: Context<'_>,
    #[description = "Panel name from bot.toml"]
    #[autocomplete = "panel_names"]
    name: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let panel = data.cfg.role_panel.iter().find(|p| p.name == name).ok_or("нет такой панели в bot.toml")?;
    let channel = data.names.channel(&panel.channel).ok_or_else(|| format!("канал '{}' не найден", panel.channel))?;
    let mut buttons = Vec::new();
    for b in &panel.buttons {
        let role = data.names.role(&b.role).ok_or_else(|| format!("роль '{}' не найдена", b.role))?;
        let mut button = serenity::CreateButton::new(format!("rp:{}", role))
            .label(&b.label)
            .style(serenity::ButtonStyle::Secondary);
        if let Some(e) = &b.emoji {
            button = button.emoji(serenity::ReactionType::Unicode(e.clone()));
        }
        buttons.push(button);
    }
    let rows = buttons.chunks(5).map(|c| serenity::CreateActionRow::Buttons(c.to_vec())).collect();
    let mut embed = serenity::CreateEmbed::new().title(&panel.title).color(0x5865F2);
    if !panel.description.is_empty() {
        embed = embed.description(data.names.render(&panel.description, None, &data.server_name));
    }
    channel.send_message(ctx.http(), serenity::CreateMessage::new().embed(embed).components(rows)).await?;
    ctx.send(poise::CreateReply::default().content(format!("✅ <#{}>", channel)).ephemeral(true)).await?;
    Ok(())
}

pub async fn on_component(ctx: &serenity::Context, data: &Data, mci: &serenity::ComponentInteraction) {
    let Some(role) = mci.data.custom_id.strip_prefix("rp:").and_then(|s| s.parse::<u64>().ok()).map(serenity::RoleId::new) else {
        return;
    };
    let ru = mci.locale.starts_with("ru");
    let allowed = data.cfg.role_panel.iter().flat_map(|p| &p.buttons).filter_map(|b| data.names.role(&b.role)).any(|r| r == role);
    let text = match (&mci.member, allowed) {
        (Some(member), true) => {
            let has = member.roles.contains(&role);
            let r = if has {
                ctx.http.remove_member_role(data.guild, member.user.id, role, Some("role panel")).await
            } else {
                ctx.http.add_member_role(data.guild, member.user.id, role, Some("role panel")).await
            };
            match (r, has, ru) {
                (Ok(_), false, true) => format!("✅ Роль <@&{}> выдана", role),
                (Ok(_), false, false) => format!("✅ Role <@&{}> added", role),
                (Ok(_), true, true) => format!("➖ Роль <@&{}> снята", role),
                (Ok(_), true, false) => format!("➖ Role <@&{}> removed", role),
                (Err(e), _, _) => {
                    eprintln!("[Roles] {} → {}: {}", member.user.name, role, e);
                    if ru { "⚠️ Не получилось — роль бота должна быть выше этой роли".into() } else { "⚠️ Failed — ask a moderator".into() }
                }
            }
        }
        _ => if ru { "⚠️ Эта кнопка устарела".into() } else { "⚠️ This button is outdated".into() },
    };
    let reply = serenity::CreateInteractionResponseMessage::new()
        .content(text)
        .ephemeral(true)
        .allowed_mentions(serenity::CreateAllowedMentions::new());
    if let Err(e) = mci.create_response(&ctx.http, serenity::CreateInteractionResponse::Message(reply)).await {
        eprintln!("[Roles] button response: {}", e);
    }
}
