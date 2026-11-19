//! Events: `/event create template:<template>` → form (name, start, end, description, rules) → the bot creates
//! a Discord scheduled event, a temporary category from the bot.toml template (rules, chat, results, voice), a participant
//! role with a "Participate" button in the rules channel. At the start time the event becomes active; at the end the bot
//! moves the text channels to the archive (read-only, visible to the team), deletes the voice channels, category and role,
//! and ends the event. `/event close` closes it early, `/event list` shows open ones.
//! Events are `event:<Discord event id>` records; start and end are claimed atomically with `UPDATE … WHERE … RETURN AFTER`.

use crate::config::{EventTemplate, EventsCfg};
use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use std::time::Duration;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE event SCHEMAFULL;
DEFINE FIELD OVERWRITE name ON event TYPE string;
DEFINE FIELD OVERWRITE template ON event TYPE string;
DEFINE FIELD OVERWRITE category ON event TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE channels ON event TYPE array<string> DEFAULT [];
DEFINE FIELD OVERWRITE voices ON event TYPE array<string> DEFAULT [];
DEFINE FIELD OVERWRITE role ON event TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE start ON event TYPE datetime;
DEFINE FIELD OVERWRITE end ON event TYPE datetime;
DEFINE FIELD OVERWRITE started ON event TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE external ON event TYPE bool DEFAULT true;
DEFINE FIELD OVERWRITE status ON event TYPE string DEFAULT "open" ASSERT $value INSIDE ["open", "closing", "closed"];
DEFINE INDEX OVERWRITE idx_event_status ON event COLUMNS status, end;
"#;

const SEND: u64 = 1 << 11;
const VIEW: u64 = 1 << 10;
const HISTORY: u64 = 1 << 16;
const CONNECT: u64 = 1 << 20;
const REACT: u64 = 1 << 6;
const THREADS: u64 = (1 << 35) | (1 << 36) | (1 << 38);

/// "2026-11-21 18:00" + offset "+03:00" → unix (UTC)
pub fn parse_local(text: &str, offset: &str) -> Option<i64> {
    let (date, time) = text.trim().split_once(' ')?;
    let d: Vec<i64> = date.split('-').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let t: Vec<i64> = time.trim().split(':').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let (y, m, dd) = (*d.first()?, *d.get(1)?, *d.get(2)?);
    let (hh, mm) = (*t.first()?, *t.get(1).unwrap_or(&0));
    if !(1..=12).contains(&m) || !(1..=31).contains(&dd) || !(0..24).contains(&hh) || !(0..60).contains(&mm) {
        return None;
    }
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + dd - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    let sign = if offset.starts_with('-') { -1 } else { 1 };
    let (oh, om) = offset.trim_start_matches(['+', '-']).split_once(':').unwrap_or((offset.trim_start_matches(['+', '-']), "0"));
    let off = sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60);
    Some(days * 86400 + hh * 3600 + mm * 60 - off)
}

fn fill(template: &str, name: &str) -> String {
    template.replace("{name}", name).replace("{NAME}", &name.to_uppercase())
}

fn staff(data: &Data, cfg: &EventsCfg) -> Vec<serenity::RoleId> {
    cfg.staff_roles.iter().filter_map(|r| data.names.role(r)).collect()
}

fn ow(role: serenity::RoleId, allow: u64, deny: u64) -> serenity::PermissionOverwrite {
    serenity::PermissionOverwrite {
        allow: serenity::Permissions::from_bits_truncate(allow),
        deny: serenity::Permissions::from_bits_truncate(deny),
        kind: serenity::PermissionOverwriteType::Role(role),
    }
}

async fn template_names(ctx: Context<'_>, partial: &str) -> serenity::CreateAutocompleteResponse {
    let choices = ctx
        .data()
        .cfg
        .events
        .iter()
        .flat_map(|e| &e.template)
        .filter(|t| t.name.contains(&partial.to_lowercase()))
        .map(|t| serenity::AutocompleteChoice::new(t.name.clone(), t.name.clone()))
        .collect();
    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

#[derive(Deserialize, Clone)]
struct EventRow {
    id: surrealdb::RecordId,
    name: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    channels: Vec<String>,
    #[serde(default)]
    voices: Vec<String>,
    #[serde(default)]
    role: String,
    #[serde(default)]
    external: bool,
}

impl EventRow {
    /// Discord event id (record key)
    fn key(&self) -> String {
        self.id.key().to_string().trim_matches(|c| c == '⟨' || c == '⟩' || c == '`' || c == '\'').to_string()
    }
}

async fn open_events(ctx: Context<'_>, partial: &str) -> serenity::CreateAutocompleteResponse {
    let rows: Vec<EventRow> = ctx
        .data()
        .db
        .db
        .query("SELECT * FROM event WHERE status = 'open' ORDER BY start")
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();
    let choices = rows
        .into_iter()
        .filter(|r| r.name.to_lowercase().contains(&partial.to_lowercase()))
        .map(|r| serenity::AutocompleteChoice::new(r.name.clone(), r.key()))
        .collect();
    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

/// Ивенты: мероприятие Discord и временная категория
#[poise::command(slash_command, guild_only, subcommands("create", "close", "list"), subcommand_required, default_member_permissions = "MANAGE_EVENTS")]
pub async fn event(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Create an event and its temporary channels / Создать ивент и временные каналы
#[poise::command(slash_command, guild_only)]
async fn create(
    ctx: Context<'_>,
    #[description = "Template / Шаблон"]
    #[autocomplete = "template_names"]
    template: String,
) -> Result<(), Error> {
    let cfg = ctx.data().cfg.events.as_ref().ok_or("модуль events выключен")?;
    let t = cfg.template.iter().find(|t| t.name == template).ok_or("нет такого шаблона в [events]")?;
    let tz = &cfg.tz_label;
    let mut rows = vec![
        serenity::CreateActionRow::InputText(serenity::CreateInputText::new(serenity::InputTextStyle::Short, "Name / Название", "name").max_length(80).required(true)),
        serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(serenity::InputTextStyle::Short, format!("Start / Начало ({})", tz), "start").placeholder("2026-11-21 18:00").max_length(16).required(true),
        ),
        serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(serenity::InputTextStyle::Short, format!("End / Конец ({})", tz), "end").placeholder("2026-11-28 18:00").max_length(16).required(true),
        ),
        serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(serenity::InputTextStyle::Paragraph, "Event description / Описание", "description").max_length(1000).required(false),
        ),
    ];
    if t.channels.iter().any(|c| c.rules) {
        rows.push(serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(serenity::InputTextStyle::Paragraph, "Rules (### title, then text) / Правила", "rules")
                .max_length(4000)
                .required(false),
        ));
    }
    let modal = serenity::CreateModal::new(format!("evt:form:{}", t.name), "Event / Ивент").components(rows);
    let poise::Context::Application(actx) = ctx else { return Ok(()) };
    actx.interaction.create_response(ctx.http(), serenity::CreateInteractionResponse::Modal(modal)).await?;
    actx.has_sent_initial_response.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// Close an event now / Закрыть ивент сейчас
#[poise::command(slash_command, guild_only)]
async fn close(
    ctx: Context<'_>,
    #[description = "Event / Ивент"]
    #[autocomplete = "open_events"]
    event: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let data = ctx.data();
    let rows: Vec<EventRow> = data
        .db
        .db
        .query("UPDATE type::thing('event', $id) SET status = 'closing' WHERE status = 'open' RETURN AFTER")
        .bind(("id", event))
        .await?
        .take(0)?;
    match rows.into_iter().next() {
        Some(r) => {
            finish(ctx.serenity_context(), data, &r).await;
            ctx.say(format!("✅ {}", r.name)).await?;
        }
        None => {
            ctx.say("—").await?;
        }
    }
    Ok(())
}

/// SurrealDB 2.x sorts only by fields in the projection, hence ORDER BY the alias `s`
const LIST_Q: &str =
    "SELECT name, record::id(id) AS key, time::unix(start) AS s, time::unix(end) AS e, category FROM event WHERE status = 'open' ORDER BY s";

/// Open events / Открытые ивенты
#[poise::command(slash_command, guild_only)]
async fn list(ctx: Context<'_>) -> Result<(), Error> {
    #[derive(Deserialize)]
    struct L {
        name: String,
        key: String,
        s: i64,
        e: i64,
        category: String,
    }
    let rows: Vec<L> = ctx
        .data()
        .db
        .db
        .query(LIST_Q)
        .await?
        .take(0)?;
    let text = if rows.is_empty() {
        "—".to_string()
    } else {
        rows.iter()
            .map(|r| {
                let cat = if r.category.is_empty() { String::new() } else { format!(" · <#{}>", r.category) };
                format!("• **{}** · <t:{}:f> – <t:{}:f>{} · [event](https://discord.com/events/{}/{})", r.name, r.s, r.e, cat, ctx.data().guild, r.key)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    ctx.send(poise::CreateReply::default().content(text).ephemeral(true)).await?;
    Ok(())
}

fn input(mi: &serenity::ModalInteraction, id: &str) -> String {
    mi.data
        .components
        .iter()
        .flat_map(|r| &r.components)
        .find_map(|c| match c {
            serenity::ActionRowComponent::InputText(t) if t.custom_id == id => t.value.clone(),
            _ => None,
        })
        .unwrap_or_default()
}

pub async fn on_modal(ctx: &serenity::Context, data: &Data, mi: &serenity::ModalInteraction) {
    let Some(template) = mi.data.custom_id.strip_prefix("evt:form:") else { return };
    let defer = serenity::CreateInteractionResponse::Defer(serenity::CreateInteractionResponseMessage::new().ephemeral(true));
    if mi.create_response(&ctx.http, defer).await.is_err() {
        return;
    }
    let text = match build(ctx, data, mi, template).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[Events] create: {}", e);
            format!("⚠️ {}", e)
        }
    };
    let _ = mi.edit_response(&ctx.http, serenity::EditInteractionResponse::new().content(text)).await;
}

async fn build(ctx: &serenity::Context, data: &Data, mi: &serenity::ModalInteraction, template: &str) -> Result<String, Error> {
    let cfg = data.cfg.events.as_ref().ok_or("модуль events выключен")?;
    let t: &EventTemplate = cfg.template.iter().find(|t| t.name == template).ok_or("нет такого шаблона")?;
    let name = input(mi, "name").trim().to_string();
    let start = parse_local(&input(mi, "start"), &cfg.utc_offset).ok_or("начало: формат 2026-11-21 18:00")?;
    let end = parse_local(&input(mi, "end"), &cfg.utc_offset).ok_or("конец: формат 2026-11-28 18:00")?;
    if end <= start {
        return Err("конец должен быть позже начала".into());
    }
    let now = serenity::Timestamp::now().unix_timestamp();
    let description = input(mi, "description");
    let rules = input(mi, "rules");

    // 1. Discord event: stage (broadcast) or "somewhere else" (in game)
    let stage = t.stage.as_deref().and_then(|s| data.names.channel(s));
    let ts = |u: i64| serenity::Timestamp::from_unix_timestamp(u).unwrap_or_else(|_| serenity::Timestamp::now());
    let mut ev = match stage {
        Some(ch) => serenity::CreateScheduledEvent::new(serenity::ScheduledEventType::StageInstance, &name, ts(start.max(now + 60))).channel_id(ch),
        None => serenity::CreateScheduledEvent::new(serenity::ScheduledEventType::External, &name, ts(start.max(now + 60)))
            .location(t.location.clone().unwrap_or_else(|| "In game".into()))
            .end_time(ts(end)),
    };
    if !description.trim().is_empty() {
        ev = ev.description(description.trim());
    }
    let sched = data.guild.create_scheduled_event(&ctx.http, ev).await?;
    let link = format!("https://discord.com/events/{}/{}", data.guild, sched.id);

    // 2. participant role
    let role = if t.role {
        let r = data
            .guild
            .create_role(&ctx.http, serenity::EditRole::new().name(fill(&t.role_name, &name)).mentionable(true))
            .await?;
        Some(r.id)
    } else {
        None
    };

    // 3. temporary category and channels
    let mut category = None;
    let mut texts = Vec::new();
    let mut voices = Vec::new();
    let mut rules_channel = None;
    if !t.channels.is_empty() {
        let staff = staff(data, cfg);
        let cat = data
            .guild
            .create_channel(&ctx.http, serenity::CreateChannel::new(fill(&t.category, &name)).kind(serenity::ChannelType::Category))
            .await?;
        category = Some(cat.id);
        for c in &t.channels {
            let mut perms = Vec::new();
            if c.readonly {
                perms.push(ow(data.guild.everyone_role(), 0, SEND | THREADS));
                perms.extend(staff.iter().map(|r| ow(*r, SEND | THREADS, 0)));
            }
            let voice = c.kind == "voice";
            let mut b = serenity::CreateChannel::new(fill(&c.name, &name))
                .kind(if voice { serenity::ChannelType::Voice } else { serenity::ChannelType::Text })
                .category(cat.id)
                .permissions(perms);
            if let Some(n) = c.user_limit {
                b = b.user_limit(n);
            }
            if let Some(topic) = &c.topic {
                b = b.topic(fill(topic, &name));
            }
            let created = data.guild.create_channel(&ctx.http, b).await?;
            if voice {
                voices.push(created.id.to_string());
            } else {
                texts.push(created.id.to_string());
            }
            if c.rules {
                rules_channel = Some(created.id);
            }
        }
    }

    // 4. rules and the "Participate" button
    if let Some(ch) = rules_channel {
        let head = format!("# {}\n<t:{}:F> – <t:{}:F> (<t:{}:R>)\n-# 📅 [Event / Мероприятие]({})", name, start, end, start, link);
        let (_, card) = crate::modules::announce::split_rules(&format!("x\n{}\n{}", crate::modules::announce::RULES_MARK, rules));
        let mut msg = serenity::CreateMessage::new().content(head);
        if let Some((title, body)) = card {
            let mut e = serenity::CreateEmbed::new().description(body.chars().take(4000).collect::<String>()).color(0xE0A84A);
            if let Some(t) = title {
                e = e.title(t);
            }
            msg = msg.embed(e);
        }
        if role.is_some() {
            msg = msg.components(vec![serenity::CreateActionRow::Buttons(vec![serenity::CreateButton::new(format!("ev:join:{}", sched.id))
                .label("Participate / Участвую")
                .emoji('🎟')
                .style(serenity::ButtonStyle::Success)])]);
        }
        ch.send_message(&ctx.http, msg).await?;
    }

    data.db
        .db
        .query(
            "CREATE type::thing('event', $id) SET name = $n, template = $t, category = $c, channels = $tx, voices = $vc, role = $r,
             start = time::from::unix($s), end = time::from::unix($e), external = $ext",
        )
        .bind(("id", sched.id.to_string()))
        .bind(("n", name.clone()))
        .bind(("t", template.to_string()))
        .bind(("c", category.map(|c| c.to_string()).unwrap_or_default()))
        .bind(("tx", texts))
        .bind(("vc", voices))
        .bind(("r", role.map(|r| r.to_string()).unwrap_or_default()))
        .bind(("s", start))
        .bind(("e", end))
        .bind(("ext", stage.is_none()))
        .await?
        .check()?;
    println!("[Events] {} created «{}» ({})", mi.user.name, name, template);
    let cat = category.map(|c| format!(" · <#{}>", c)).unwrap_or_default();
    let ru = mi.locale.starts_with("ru");
    Ok(if ru {
        format!("✅ «{}»: <t:{}:f> – <t:{}:f>{} · [мероприятие]({}). Закроется само в конце.", name, start, end, cat, link)
    } else {
        format!("✅ \"{}\": <t:{}:f> – <t:{}:f>{} · [event]({}). Closes by itself at the end.", name, start, end, cat, link)
    })
}

/// Button ev:join:<id> — take or drop the participant role
pub async fn on_component(ctx: &serenity::Context, data: &Data, mci: &serenity::ComponentInteraction) {
    let Some(id) = mci.data.custom_id.strip_prefix("ev:join:") else { return };
    let ru = mci.locale.starts_with("ru");
    let role: Option<String> = data
        .db
        .db
        .query("SELECT VALUE role FROM ONLY type::thing('event', $id) WHERE status = 'open'")
        .bind(("id", id.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok().flatten());
    let text = match role.and_then(|r| r.parse::<u64>().ok()).map(serenity::RoleId::new) {
        None => (if ru { "Ивент уже закрыт" } else { "The event is closed" }).to_string(),
        Some(role) => {
            let has = mci.member.as_ref().is_some_and(|m| m.roles.contains(&role));
            let r = if has {
                ctx.http.remove_member_role(data.guild, mci.user.id, role, Some("event")).await
            } else {
                ctx.http.add_member_role(data.guild, mci.user.id, role, Some("event")).await
            };
            match (r, has, ru) {
                (Err(e), _, _) => format!("⚠️ {}", e),
                (Ok(_), false, true) => format!("🎟 Ты участвуешь — роль <@&{}>. Нажми ещё раз, чтобы выйти.", role),
                (Ok(_), false, false) => format!("🎟 You're in — role <@&{}>. Press again to leave.", role),
                (Ok(_), true, true) => "Ты вышел из ивента.".into(),
                (Ok(_), true, false) => "You left the event.".into(),
            }
        }
    };
    let reply = serenity::CreateInteractionResponseMessage::new().content(text).ephemeral(true).allowed_mentions(serenity::CreateAllowedMentions::new());
    let _ = mci.create_response(&ctx.http, serenity::CreateInteractionResponse::Message(reply)).await;
}

/// Close: text channels go to the archive (read-only, visible to the team); voice channels, category and role are deleted;
/// the Discord event is ended
async fn finish(ctx: &serenity::Context, data: &Data, r: &EventRow) {
    let Some(cfg) = &data.cfg.events else { return };
    let archive = data.names.channel(&cfg.archive);
    let staff = staff(data, cfg);
    let mut perms = vec![ow(data.guild.everyone_role(), 0, VIEW)];
    perms.extend(staff.iter().map(|s| ow(*s, VIEW | HISTORY, SEND | THREADS | REACT | CONNECT)));
    // in the archive, channels are prefixed with the event: «clothos-trial・rules»
    let slug: String = r.name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect::<String>().split('-').filter(|x| !x.is_empty()).collect::<Vec<_>>().join("-");
    let slug: String = slug.chars().take(40).collect();
    for id in r.channels.iter().filter_map(|c| c.parse::<u64>().ok()) {
        let mut edit = serenity::EditChannel::new().permissions(perms.clone());
        if let Ok(serenity::Channel::Guild(c)) = serenity::ChannelId::new(id).to_channel(&ctx.http).await {
            let base = c.name.rsplit('・').next().unwrap_or(&c.name).to_string();
            edit = edit.name(format!("{}・{}", slug, base));
        }
        if let Some(a) = archive {
            edit = edit.category(Some(a));
        }
        if let Err(e) = serenity::ChannelId::new(id).edit(&ctx.http, edit).await {
            eprintln!("[Events] archiving channel {}: {}", id, e);
        }
    }
    for id in r.voices.iter().chain(std::iter::once(&r.category)).filter_map(|c| c.parse::<u64>().ok()) {
        let _ = serenity::ChannelId::new(id).delete(&ctx.http).await;
    }
    if let Ok(role) = r.role.parse::<u64>() {
        let _ = data.guild.delete_role(&ctx.http, serenity::RoleId::new(role)).await;
    }
    if let Ok(eid) = r.key().parse::<u64>() {
        let eid = serenity::ScheduledEventId::new(eid);
        let status = match data.guild.scheduled_event(&ctx.http, eid, false).await.map(|e| e.status) {
            Ok(serenity::ScheduledEventStatus::Active) => Some(serenity::ScheduledEventStatus::Completed),
            Ok(serenity::ScheduledEventStatus::Scheduled) => Some(serenity::ScheduledEventStatus::Canceled),
            _ => None,
        };
        if let Some(s) = status {
            let _ = data.guild.edit_scheduled_event(&ctx.http, eid, serenity::EditScheduledEvent::new().status(s)).await;
        }
    }
    let _ = data.db.db.query("UPDATE type::thing('event', $id) SET status = 'closed'").bind(("id", r.key())).await;
    println!("[Events] «{}» closed", r.name);
}

pub fn spawn(ctx: serenity::Context, data: Data) {
    if data.cfg.events.is_none() {
        return;
    }
    tokio::spawn(async move {
        loop {
            // start: "somewhere else" events don't start on their own — make it active
            let started: Vec<EventRow> = data
                .db
                .db
                .query("UPDATE event SET started = true WHERE status = 'open' AND started = false AND start <= time::now() RETURN AFTER")
                .await
                .ok()
                .and_then(|mut r| r.take(0).ok())
                .unwrap_or_default();
            for r in started.iter().filter(|r| r.external) {
                if let Ok(eid) = r.key().parse::<u64>() {
                    let _ = data
                        .guild
                        .edit_scheduled_event(&ctx.http, serenity::ScheduledEventId::new(eid), serenity::EditScheduledEvent::new().status(serenity::ScheduledEventStatus::Active))
                        .await;
                    println!("[Events] «{}» started", r.name);
                }
            }
            // end: claim atomically and close
            let ended: Vec<EventRow> = data
                .db
                .db
                .query("UPDATE event SET status = 'closing' WHERE status = 'open' AND end <= time::now() RETURN AFTER")
                .await
                .ok()
                .and_then(|mut r| r.take(0).ok())
                .unwrap_or_default();
            for r in ended {
                finish(&ctx, &data, &r).await;
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::parse_local;

    #[test]
    fn local_time_to_unix() {
        assert_eq!(parse_local("2026-11-21 18:00", "+03:00"), Some(1_795_273_200));
        assert_eq!(parse_local("2026-11-21 15:00", "+00:00"), Some(1_795_273_200));
        assert_eq!(parse_local("2026-13-01 10:00", "+03:00"), None);
        assert_eq!(parse_local("tomorrow", "+03:00"), None);
    }

    /// The /event list query is parsed by the database and sorts by start
    #[tokio::test]
    async fn list_query_runs() {
        let (data, dir) = crate::modules::testing::data("events_list", "").await;
        let mut r = data.db.db.query(super::LIST_Q).await.unwrap();
        let rows: Vec<serde_json::Value> = r.take(0).unwrap();
        assert!(rows.is_empty());
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
