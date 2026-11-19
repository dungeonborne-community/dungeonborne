//! `/poll` — a native Discord poll posted by the bot: form (question, one answer per line, caption) → poll in the current channel.
//! Voting uses Discord buttons; Discord shows the results itself at the end (and posts a "poll ended" message).
//! An answer may start with an emoji: "🛡 Fighter". Duration in hours (1…336), multiple answers via `multiselect`.

use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;
use std::time::Duration;

const MAX_ANSWERS: usize = 10;

/// Line "🛡 Fighter" → (Some("🛡"), "Fighter"); "Fighter" → (None, "Fighter")
pub fn split_emoji(line: &str) -> (Option<String>, String) {
    let line = line.trim();
    if let Some((first, rest)) = line.split_once(' ') {
        if !first.is_empty() && !first.chars().any(|c| c.is_alphanumeric()) && !rest.trim().is_empty() {
            return (Some(first.to_string()), rest.trim().to_string());
        }
    }
    (None, line.to_string())
}

/// Answers from the form text: one per line, empty lines skipped; errors as readable text
pub fn parse_answers(text: &str) -> Result<Vec<(Option<String>, String)>, String> {
    let answers: Vec<_> = text.lines().filter(|l| !l.trim().is_empty()).map(split_emoji).collect();
    if answers.len() < 2 {
        return Err("нужно хотя бы 2 варианта — по одному в строке".into());
    }
    if answers.len() > MAX_ANSWERS {
        return Err(format!("не больше {} вариантов", MAX_ANSWERS));
    }
    if let Some((_, t)) = answers.iter().find(|(_, t)| t.chars().count() > 55) {
        return Err(format!("вариант длиннее 55 символов: «{}»", t));
    }
    Ok(answers)
}

/// Create a poll / Создать опрос
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_EVENTS")]
pub async fn poll(
    ctx: Context<'_>,
    #[description = "Duration in hours, 1–336 (default 24) / Сколько часов"]
    #[min = 1]
    #[max = 336]
    hours: Option<u32>,
    #[description = "Several answers allowed / Можно выбрать несколько"] multiselect: Option<bool>,
) -> Result<(), Error> {
    let id = format!("pl:form:{}:{}", hours.unwrap_or(24), u8::from(multiselect.unwrap_or(false)));
    let modal = serenity::CreateModal::new(id, "Poll / Опрос").components(vec![
        serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(serenity::InputTextStyle::Short, "Question / Вопрос", "q").max_length(300).required(true),
        ),
        serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(serenity::InputTextStyle::Paragraph, "Answers, one per line / Варианты по строке", "a")
                .placeholder("🛡 Fighter\n🗡 Rogue\n✨ Priest")
                .max_length(700)
                .required(true),
        ),
        serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(serenity::InputTextStyle::Paragraph, "Text above (optional) / Текст над опросом", "t")
                .max_length(1800)
                .required(false),
        ),
    ]);
    let poise::Context::Application(actx) = ctx else { return Ok(()) };
    actx.interaction.create_response(ctx.http(), serenity::CreateInteractionResponse::Modal(modal)).await?;
    actx.has_sent_initial_response.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

pub async fn on_modal(ctx: &serenity::Context, data: &Data, mi: &serenity::ModalInteraction) {
    let Some(rest) = mi.data.custom_id.strip_prefix("pl:form:") else { return };
    let (hours, multi) = rest.split_once(':').map(|(h, m)| (h.parse::<u64>().unwrap_or(24), m == "1")).unwrap_or((24, false));
    let field = |id: &str| {
        mi.data
            .components
            .iter()
            .flat_map(|r| &r.components)
            .find_map(|c| match c {
                serenity::ActionRowComponent::InputText(t) if t.custom_id == id => t.value.clone(),
                _ => None,
            })
            .unwrap_or_default()
    };
    let ru = mi.locale.starts_with("ru");
    let reply = |text: String| serenity::CreateInteractionResponse::Message(serenity::CreateInteractionResponseMessage::new().content(text).ephemeral(true));
    let answers = match parse_answers(&field("a")) {
        Ok(a) => a,
        Err(e) => {
            let _ = mi.create_response(&ctx.http, reply(format!("⚠️ {}", e))).await;
            return;
        }
    };
    let mut poll = serenity::CreatePoll::new()
        .question(field("q").trim())
        .answers(
            answers
                .into_iter()
                .map(|(emoji, text)| {
                    let a = serenity::CreatePollAnswer::new().text(text);
                    match emoji {
                        Some(e) => a.emoji(e),
                        None => a,
                    }
                })
                .collect(),
        )
        .duration(Duration::from_secs(hours * 3600));
    if multi {
        poll = poll.allow_multiselect();
    }
    let note = data.names.render(field("t").trim(), None, &data.server_name);
    let mut msg = serenity::CreateMessage::new().poll(poll).allowed_mentions(serenity::CreateAllowedMentions::new());
    if !note.is_empty() {
        msg = msg.content(note);
    }
    let text = match mi.channel_id.send_message(&ctx.http, msg).await {
        Ok(m) => {
            println!("[Poll] {} → poll in {} for {} h", mi.user.name, mi.channel_id, hours);
            if ru { format!("✅ Опрос опубликован на {} ч: {}", hours, m.link()) } else { format!("✅ Poll posted for {} h: {}", hours, m.link()) }
        }
        Err(e) => format!("⚠️ {}", e),
    };
    let _ = mi.create_response(&ctx.http, reply(text)).await;
}

#[cfg(test)]
mod tests {
    use super::{parse_answers, split_emoji};

    #[test]
    fn answers_with_and_without_emoji() {
        assert_eq!(split_emoji("🛡 Fighter"), (Some("🛡".into()), "Fighter".into()));
        assert_eq!(split_emoji("Death Knight"), (None, "Death Knight".into()));
        assert_eq!(split_emoji("  ❄️ Cryomancer  "), (Some("❄️".into()), "Cryomancer".into()));
        assert!(parse_answers("only one").is_err());
        assert_eq!(parse_answers("A\n\nB\n").unwrap().len(), 2);
        assert!(parse_answers(&"x\n".repeat(11)).is_err());
    }
}
