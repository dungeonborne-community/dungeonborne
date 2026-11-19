//! Content creators announce themselves. In 🎬・creator-hub (visible only to creators) there is a "🔴 Stream / 🎥 Video" panel
//! → form (link, title, a few words) → the bot posts the announcement in 🔴・live-now, pinging 🔔 Streams subscribers;
//! Discord fetches the video or stream preview from the link by itself. No YouTube or Twitch polling.
//! Spam protection lives in the database: a cooldown per announcement kind (`$last + <duration>$cd > time::now()`), links only to
//! allowed platforms. Each announcement is an `announcement` record; an event on it writes the announcement to the moderation log.
//! `/creator add | remove | list | panel`: moderators grant and remove the creator role and post the panel.

use crate::{Context, Data, Error};
use poise::serenity_prelude as serenity;
use serde::Deserialize;

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE creator SCHEMAFULL;
DEFINE FIELD OVERWRITE name ON creator TYPE string;
DEFINE FIELD OVERWRITE link ON creator TYPE option<string>;
DEFINE FIELD OVERWRITE added_by ON creator TYPE string;
DEFINE FIELD OVERWRITE at ON creator TYPE datetime DEFAULT time::now() READONLY;
REMOVE FIELD IF EXISTS youtube ON creator;
REMOVE FIELD IF EXISTS twitch ON creator;
DEFINE EVENT OVERWRITE creator_log ON creator WHEN $event = "CREATE" OR $event = "DELETE" THEN {
    LET $c = $after OR $before;
    CREATE mod_log CONTENT {
        kind: IF $event = "CREATE" THEN "creator_add" ELSE "creator_remove" END,
        user_id: record::id($c.id),
        user_name: $c.name,
        text: $c.link,
    };
};

DEFINE TABLE OVERWRITE announcement SCHEMAFULL;
DEFINE FIELD OVERWRITE creator ON announcement TYPE record<creator>;
DEFINE FIELD OVERWRITE name ON announcement TYPE string;
DEFINE FIELD OVERWRITE kind ON announcement TYPE string ASSERT $value INSIDE ["stream", "video"];
DEFINE FIELD OVERWRITE link ON announcement TYPE string;
DEFINE FIELD OVERWRITE title ON announcement TYPE string;
DEFINE FIELD OVERWRITE message ON announcement TYPE option<string>;
DEFINE FIELD OVERWRITE at ON announcement TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE idx_announcement_creator ON announcement COLUMNS creator, kind, at;
DEFINE EVENT OVERWRITE announcement_log ON announcement WHEN $event = "CREATE" THEN {
    CREATE mod_log CONTENT {
        kind: "creator_post",
        user_id: record::id($after.creator),
        user_name: $after.name,
        text: (IF $after.kind = "stream" THEN "🔴 " ELSE "🎥 " END) + $after.title + " · " + $after.link,
    };
};
"#;

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Creator {
    pub user: String,
    pub name: String,
    pub link: Option<String>,
}

const FIELDS: &str = "record::id(id) AS user, name, link";

pub async fn upsert(data: &Data, user: &str, name: &str, link: Option<String>, by: &str) -> Result<(), Error> {
    data.db
        .db
        .query("UPSERT type::thing('creator', $u) SET name = $n, added_by = $by, link = IF $l != NONE THEN $l ELSE link END")
        .bind(("u", user.to_string()))
        .bind(("n", name.to_string()))
        .bind(("l", link))
        .bind(("by", by.to_string()))
        .await?
        .check()?;
    Ok(())
}

pub async fn delete(data: &Data, user: &str) -> Result<bool, Error> {
    #[derive(Deserialize)]
    struct Gone {
        #[allow(dead_code)]
        name: String,
    }
    let gone: Vec<Gone> =
        data.db.db.query("DELETE type::thing('creator', $u) RETURN BEFORE").bind(("u", user.to_string())).await?.check()?.take(0)?;
    Ok(!gone.is_empty())
}

pub async fn list(data: &Data) -> Vec<Creator> {
    match data.db.db.query(format!("SELECT {} FROM creator ORDER BY name", FIELDS)).await {
        Ok(mut r) => r.take(0).unwrap_or_default(),
        Err(e) => {
            eprintln!("[Creators] list: {}", e);
            Vec::new()
        }
    }
}

/// The link must point to an allowed platform (the domain or its subdomain)
pub fn check_link(link: &str, allowed: &[String]) -> Result<String, String> {
    let link = link.trim();
    let rest = link.strip_prefix("https://").or_else(|| link.strip_prefix("http://")).ok_or("ссылка должна начинаться с https://")?;
    if link.contains(char::is_whitespace) {
        return Err("в ссылке не должно быть пробелов".into());
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("").split(':').next().unwrap_or("").to_lowercase();
    let ok = allowed.iter().any(|d| host == d.as_str() || host.ends_with(&format!(".{}", d)));
    if ok {
        Ok(link.to_string())
    } else {
        Err(format!("площадка «{}» не в списке разрешённых: {}", host, allowed.join(", ")))
    }
}

/// Cooldown: None = can announce, Some(unix) = when it becomes possible
pub async fn cooldown_until(data: &Data, user: &str, kind: &str, cooldown: &str) -> Result<Option<i64>, Error> {
    let q = "LET $last = (SELECT VALUE at FROM announcement WHERE creator = type::thing('creator', $u) AND kind = $k ORDER BY at DESC LIMIT 1)[0];
             RETURN IF $last != NONE AND $last + <duration>$cd > time::now() THEN time::unix($last + <duration>$cd) ELSE NONE END;";
    let mut r = data
        .db
        .db
        .query(q)
        .bind(("u", user.to_string()))
        .bind(("k", kind.to_string()))
        .bind(("cd", cooldown.to_string()))
        .await?
        .check()?;
    Ok(r.take::<Option<i64>>(1)?)
}

pub async fn record(data: &Data, user: &str, name: &str, kind: &str, link: &str, title: &str, message: &str) -> Result<(), Error> {
    data.db
        .db
        .query("CREATE announcement SET creator = type::thing('creator', $u), name = $n, kind = $k, link = $l, title = $t, message = $m")
        .bind(("u", user.to_string()))
        .bind(("n", name.to_string()))
        .bind(("k", kind.to_string()))
        .bind(("l", link.to_string()))
        .bind(("t", title.to_string()))
        .bind(("m", message.to_string()))
        .await?
        .check()?;
    Ok(())
}

#[poise::command(slash_command, guild_only, subcommands("add", "remove", "list_cmd", "panel"), subcommand_required, default_member_permissions = "MANAGE_ROLES")]
pub async fn creator(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

fn creator_role(data: &Data) -> Option<serenity::RoleId> {
    data.cfg.creators.as_ref().and_then(|c| data.names.role(&c.role))
}

/// Сделать человека автором: роль 🎬 Creator и доступ к панели анонсов
#[poise::command(slash_command, guild_only)]
async fn add(
    ctx: Context<'_>,
    #[description = "Creator / Автор"] user: serenity::User,
    #[description = "Main channel link (optional) / Ссылка на канал"] link: Option<String>,
) -> Result<(), Error> {
    let data = ctx.data();
    let cfg = data.cfg.creators.as_ref().ok_or("модуль авторов выключен")?;
    let link = link.map(|l| check_link(&l, &cfg.allowed)).transpose()?;
    upsert(data, &user.id.to_string(), &user.name, link, &ctx.author().id.to_string()).await?;
    let role = creator_role(data).ok_or_else(|| format!("роль '{}' не найдена", cfg.role))?;
    ctx.http().add_member_role(data.guild, user.id, role, Some("/creator add")).await?;
    println!("[Creators] + {}", user.name);
    ctx.send(poise::CreateReply::default().content(format!("✅ <@{}> — 🎬 Creator", user.id)).ephemeral(true)).await?;
    Ok(())
}

/// Убрать автора: снять роль
#[poise::command(slash_command, guild_only)]
async fn remove(ctx: Context<'_>, #[description = "Creator / Автор"] user: serenity::User) -> Result<(), Error> {
    let data = ctx.data();
    let was = delete(data, &user.id.to_string()).await?;
    if let Some(role) = creator_role(data) {
        let _ = ctx.http().remove_member_role(data.guild, user.id, role, Some("/creator remove")).await;
    }
    println!("[Creators] − {}", user.name);
    let text = if was { format!("➖ <@{}>", user.id) } else { format!("➖ <@{}> (роль снята, в списке не был)", user.id) };
    ctx.send(poise::CreateReply::default().content(text).ephemeral(true)).await?;
    Ok(())
}

/// Все авторы
#[poise::command(slash_command, guild_only, rename = "list")]
async fn list_cmd(ctx: Context<'_>) -> Result<(), Error> {
    let all = list(ctx.data()).await;
    let lines: Vec<String> = all
        .iter()
        .map(|c| match &c.link {
            Some(l) => format!("<@{}> · {}", c.user, l),
            None => format!("<@{}>", c.user),
        })
        .collect();
    let embed = serenity::CreateEmbed::new()
        .title(format!("🎬 Creators ({})", all.len()))
        .description(if lines.is_empty() { "—".into() } else { lines.join("\n") })
        .color(0x9B59B6);
    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true)).await?;
    Ok(())
}

/// Опубликовать панель анонсов в канале авторов
#[poise::command(slash_command, guild_only)]
async fn panel(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let cfg = data.cfg.creators.as_ref().ok_or("модуль авторов выключен")?;
    let hub = data.names.channel(&cfg.hub).ok_or_else(|| format!("канал '{}' не найден", cfg.hub))?;
    let live = data.names.channel(&cfg.channel).map(|c| format!("<#{}>", c)).unwrap_or_else(|| cfg.channel.clone());
    let embed = serenity::CreateEmbed::new()
        .title("📣 Announce your content / Анонс своего контента")
        .description(format!(
            "Going live or posted a video? Press a button, paste the link — the bot posts it in {live} and pings subscribers.\n\
             Stream: once per {s} · Video: once per {v}\n\n\
             Начал стрим или выложил видео? Нажми кнопку и вставь ссылку — бот опубликует анонс в {live} и позовёт подписчиков.\n\
             Стрим: раз в {s} · Видео: раз в {v}",
            live = live,
            s = cfg.stream_cooldown,
            v = cfg.video_cooldown
        ))
        .color(0x9B59B6);
    let buttons = vec![
        serenity::CreateButton::new("cr:stream").label("Stream / Стрим").emoji('🔴').style(serenity::ButtonStyle::Danger),
        serenity::CreateButton::new("cr:video").label("Video / Видео").emoji('🎥').style(serenity::ButtonStyle::Primary),
    ];
    hub.send_message(ctx.http(), serenity::CreateMessage::new().embed(embed).components(vec![serenity::CreateActionRow::Buttons(buttons)]))
        .await?;
    ctx.send(poise::CreateReply::default().content(format!("✅ <#{}>", hub)).ephemeral(true)).await?;
    Ok(())
}

/// A creator has the 🎬 Creator role; administrators can too (for testing)
fn is_creator(data: &Data, member: Option<&serenity::Member>) -> bool {
    let Some(m) = member else { return false };
    m.permissions.is_some_and(|p| p.administrator()) || creator_role(data).is_some_and(|r| m.roles.contains(&r))
}

fn reply(text: impl Into<String>) -> serenity::CreateInteractionResponse {
    serenity::CreateInteractionResponse::Message(serenity::CreateInteractionResponseMessage::new().content(text).ephemeral(true))
}

fn cooldown_of<'a>(cfg: &'a crate::config::CreatorsCfg, kind: &str) -> &'a str {
    if kind == "stream" {
        &cfg.stream_cooldown
    } else {
        &cfg.video_cooldown
    }
}

/// cr:stream / cr:video buttons → role and cooldown check → form
pub async fn on_component(ctx: &serenity::Context, data: &Data, mci: &serenity::ComponentInteraction) {
    let Some(kind) = mci.data.custom_id.strip_prefix("cr:").filter(|k| *k == "stream" || *k == "video") else { return };
    let Some(cfg) = &data.cfg.creators else { return };
    let ru = mci.locale.starts_with("ru");
    let response = if !is_creator(data, mci.member.as_ref()) {
        reply(if ru { "Анонсы доступны авторам с ролью 🎬 Creator" } else { "Announcements are for 🎬 Creators" })
    } else {
        match cooldown_until(data, &mci.user.id.to_string(), kind, cooldown_of(cfg, kind)).await {
            Ok(Some(until)) => reply(if ru { format!("⏳ Следующий анонс — <t:{}:R>", until) } else { format!("⏳ Next announcement <t:{}:R>", until) }),
            Ok(None) => {
                let title = if kind == "stream" { "🔴 Stream / Стрим" } else { "🎥 Video / Видео" };
                let modal = serenity::CreateModal::new(format!("cr:form:{}", kind), title).components(vec![
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(serenity::InputTextStyle::Short, "Link / Ссылка", "link")
                            .placeholder("https://")
                            .max_length(200)
                            .required(true),
                    ),
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(serenity::InputTextStyle::Short, "Title / Название", "title").max_length(100).required(true),
                    ),
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(serenity::InputTextStyle::Paragraph, "A few words / Пара слов", "about")
                            .max_length(300)
                            .required(false),
                    ),
                ]);
                serenity::CreateInteractionResponse::Modal(modal)
            }
            Err(e) => {
                eprintln!("[Creators] cooldown {}: {}", mci.user.name, e);
                reply("⚠️")
            }
        }
    };
    if let Err(e) = mci.create_response(&ctx.http, response).await {
        eprintln!("[Creators] button response: {}", e);
    }
}

fn input(mi: &serenity::ModalInteraction, id: &str) -> String {
    mi.data
        .components
        .iter()
        .flat_map(|row| &row.components)
        .find_map(|c| match c {
            serenity::ActionRowComponent::InputText(t) if t.custom_id == id => t.value.clone(),
            _ => None,
        })
        .unwrap_or_default()
}

/// Announcement text: Discord expands the link into a video or stream preview by itself
pub fn announcement_text(kind: &str, user: serenity::UserId, title: &str, about: &str, link: &str, ping: &str) -> String {
    let head = if kind == "stream" { format!("🔴 <@{}> is live / в эфире!", user) } else { format!("🎥 <@{}> · new video / новое видео", user) };
    let about: String = about.lines().filter(|l| !l.trim().is_empty()).map(|l| format!("> {}\n", l.trim())).collect();
    format!("{}{}\n**{}**\n{}{}", ping, head, title.trim(), about, link)
}

pub async fn on_modal(ctx: &serenity::Context, data: &Data, mi: &serenity::ModalInteraction) {
    let Some(kind) = mi.data.custom_id.strip_prefix("cr:form:") else { return };
    let Some(cfg) = &data.cfg.creators else { return };
    let defer = serenity::CreateInteractionResponse::Defer(serenity::CreateInteractionResponseMessage::new().ephemeral(true));
    if mi.create_response(&ctx.http, defer).await.is_err() {
        return;
    }
    let ru = mi.locale.starts_with("ru");
    let text = match publish(ctx, data, cfg, mi, kind).await {
        Ok(url) => if ru { format!("✅ Анонс опубликован: {}", url) } else { format!("✅ Announced: {}", url) },
        Err(e) => format!("⚠️ {}", e),
    };
    let _ = mi.edit_response(&ctx.http, serenity::EditInteractionResponse::new().content(text)).await;
}

async fn publish(ctx: &serenity::Context, data: &Data, cfg: &crate::config::CreatorsCfg, mi: &serenity::ModalInteraction, kind: &str) -> Result<String, Error> {
    if !is_creator(data, mi.member.as_ref()) {
        return Err("нужна роль 🎬 Creator".into());
    }
    let user = mi.user.id.to_string();
    if let Some(until) = cooldown_until(data, &user, kind, cooldown_of(cfg, kind)).await? {
        return Err(format!("следующий анонс — <t:{}:R>", until).into());
    }
    let link = check_link(&input(mi, "link"), &cfg.allowed)?;
    let title = input(mi, "title");
    let channel = data.names.channel(&cfg.channel).ok_or("канал анонсов не найден")?;
    let ping_role = cfg.ping.as_deref().and_then(|r| data.names.role(r));
    let ping = ping_role.map(|r| format!("<@&{}> ", r)).unwrap_or_default();
    let mentions = serenity::CreateAllowedMentions::new().roles(ping_role.into_iter().collect::<Vec<_>>());
    let msg = channel
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new()
                .content(announcement_text(kind, mi.user.id, &title, &input(mi, "about"), &link, &ping))
                .allowed_mentions(mentions),
        )
        .await?;
    record(data, &user, &mi.user.name, kind, &link, &title, &msg.id.to_string()).await?;
    println!("[Creators] {} {}: {}", kind, mi.user.name, title);
    Ok(msg.link())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::testing;

    #[test]
    fn links_and_text() {
        let allowed: Vec<String> = ["youtube.com", "youtu.be", "twitch.tv"].map(String::from).to_vec();
        assert!(check_link("https://www.youtube.com/watch?v=abc", &allowed).is_ok());
        assert!(check_link("https://m.youtube.com/watch?v=abc", &allowed).is_ok());
        assert!(check_link("https://youtu.be/abc", &allowed).is_ok());
        assert!(check_link("https://twitch.tv/vasya", &allowed).is_ok());
        assert!(check_link("https://youtube.com.evil.ru/x", &allowed).is_err(), "foreign domain that looks like youtube");
        assert!(check_link("https://notyoutube.com/x", &allowed).is_err());
        assert!(check_link("youtube.com/x", &allowed).is_err(), "no https");
        assert!(check_link("https://youtube.com/x y", &allowed).is_err());

        let t = announcement_text("stream", serenity::UserId::new(5), " Данж ", "строка 1\n\nстрока 2", "https://twitch.tv/v", "<@&9> ");
        assert_eq!(t, "<@&9> 🔴 <@5> is live / в эфире!\n**Данж**\n> строка 1\n> строка 2\nhttps://twitch.tv/v");
    }

    /// Cooldown per announcement kind, creator and announcement events → mod_log, removing a creator
    #[tokio::test]
    async fn cooldowns_and_log() {
        let (data, dir) = testing::data("creators", "").await;
        upsert(&data, "5", "Вася", Some("https://twitch.tv/vasya".into()), "1").await.unwrap();
        upsert(&data, "5", "Вася", None, "1").await.unwrap();
        assert_eq!(list(&data).await, vec![Creator { user: "5".into(), name: "Вася".into(), link: Some("https://twitch.tv/vasya".into()) }]);

        assert_eq!(cooldown_until(&data, "5", "stream", "4h").await.unwrap(), None);
        record(&data, "5", "Вася", "stream", "https://twitch.tv/vasya", "Данж", "100").await.unwrap();
        let until = cooldown_until(&data, "5", "stream", "4h").await.unwrap().expect("cooldown active");
        assert!((until - serenity::Timestamp::now().unix_timestamp() - 4 * 3600).abs() < 60);
        assert_eq!(cooldown_until(&data, "5", "video", "8h").await.unwrap(), None, "video has its own cooldown");
        data.db.db.query("UPDATE announcement SET at = time::now() - 5h").await.unwrap();
        assert_eq!(cooldown_until(&data, "5", "stream", "4h").await.unwrap(), None, "cooldown passed");
        assert!(cooldown_until(&data, "5", "stream", "garbage").await.is_err());

        assert!(delete(&data, "5").await.unwrap());
        assert!(!delete(&data, "5").await.unwrap());

        #[derive(Deserialize)]
        struct L {
            kind: String,
            text: Option<String>,
        }
        let logs: Vec<L> = data.db.db.query("SELECT kind, text, at FROM mod_log ORDER BY at").await.unwrap().take(0).unwrap();
        let logs: Vec<(String, Option<String>)> = logs.into_iter().map(|l| (l.kind, l.text)).collect();
        assert_eq!(
            logs,
            vec![
                ("creator_add".into(), Some("https://twitch.tv/vasya".into())),
                ("creator_post".into(), Some("🔴 Данж · https://twitch.tv/vasya".into())),
                ("creator_remove".into(), Some("https://twitch.tv/vasya".into())),
            ]
        );
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
