//! "Translate / Перевести" in the message menu (right-click → Apps): DeepL translation into the Discord client language
//! of whoever clicked; only they see the reply. Translations are cached in the database under a composite ID
//! `translation:[message id, language]` — clicking the same message again doesn't spend the DeepL quota.
//! The message menu doesn't need the Message Content Intent: the text arrives with the command.

use crate::{Context, Error};
use poise::serenity_prelude as serenity;
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE translation SCHEMAFULL;
DEFINE FIELD OVERWRITE text ON translation TYPE string;
DEFINE FIELD OVERWRITE source ON translation TYPE string;
DEFINE FIELD OVERWRITE at ON translation TYPE datetime DEFAULT time::now() READONLY;
DEFINE EVENT OVERWRITE translation_retention ON translation WHEN $event = "CREATE" THEN {
    DELETE translation WHERE at < time::now() - 30d;
};
"#;

/// Discord locale → DeepL language; unsupported ones → English
pub fn target_lang(locale: &str) -> &'static str {
    match locale {
        "en-GB" => "EN-GB",
        "pt-BR" => "PT-BR",
        "zh-TW" => "ZH-HANT",
        l => match l.split('-').next().unwrap_or("en") {
            "ru" => "RU",
            "uk" => "UK",
            "de" => "DE",
            "fr" => "FR",
            "es" => "ES",
            "it" => "IT",
            "pl" => "PL",
            "tr" => "TR",
            "ja" => "JA",
            "ko" => "KO",
            "zh" => "ZH-HANS",
            "nl" => "NL",
            "sv" => "SV",
            "cs" => "CS",
            "da" => "DA",
            "fi" => "FI",
            "el" => "EL",
            "hu" => "HU",
            "ro" => "RO",
            "bg" => "BG",
            "lt" => "LT",
            "id" => "ID",
            "no" => "NB",
            _ => "EN-US",
        },
    }
}

#[derive(Serialize, Deserialize)]
struct Cached {
    text: String,
    source: String,
}

#[derive(Deserialize)]
struct DeeplResponse {
    translations: Vec<DeeplItem>,
}

#[derive(Deserialize)]
struct DeeplItem {
    detected_source_language: String,
    text: String,
}

/// Text of the message and its embeds (in example posts and announcements the main text is often in an embed)
fn message_text(msg: &serenity::Message) -> String {
    let mut parts = vec![msg.content.clone()];
    for e in &msg.embeds {
        parts.extend(e.title.clone());
        parts.extend(e.description.clone());
        for f in &e.fields {
            parts.push(format!("{}\n{}", f.name, f.value));
        }
    }
    let text = parts.into_iter().filter(|p| !p.trim().is_empty()).collect::<Vec<_>>().join("\n\n");
    text.chars().take(4000).collect()
}

async fn deepl(http: &reqwest::Client, key: &str, text: &str, lang: &str) -> Result<Cached, Error> {
    let host = if key.ends_with(":fx") { "api-free.deepl.com" } else { "api.deepl.com" };
    let resp = http
        .post(format!("https://{}/v2/translate", host))
        .header("Authorization", format!("DeepL-Auth-Key {}", key))
        .json(&serde_json::json!({ "text": [text], "target_lang": lang }))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(format!("DeepL {}: {}", resp.status(), resp.text().await.unwrap_or_default()).into());
    }
    let item = resp.json::<DeeplResponse>().await?.translations.into_iter().next().ok_or("DeepL: пустой ответ")?;
    Ok(Cached { text: item.text, source: item.detected_source_language })
}

/// Translate the message into your Discord language
#[poise::command(context_menu_command = "Translate / Перевести", guild_only)]
pub async fn translate(ctx: Context<'_>, msg: serenity::Message) -> Result<(), Error> {
    let data = ctx.data();
    let cfg = data.cfg.translate.as_ref().ok_or("перевод выключен в bot.toml")?;
    let lang = target_lang(ctx.locale().unwrap_or("en-US"));
    let text = message_text(&msg);
    if text.is_empty() {
        ctx.send(poise::CreateReply::default().content("🤷").ephemeral(true)).await?;
        return Ok(());
    }
    ctx.defer_ephemeral().await?;

    let cached: Option<Cached> = data
        .db
        .db
        .query("SELECT text, source FROM ONLY type::thing('translation', [$m, $l])")
        .bind(("m", msg.id.to_string()))
        .bind(("l", lang))
        .await?
        .take(0)?;
    let result = match cached {
        Some(c) => c,
        None => {
            let key = std::env::var(&cfg.deepl_key_env).map_err(|_| format!("нет ключа DeepL в {}", cfg.deepl_key_env))?;
            let c = deepl(&data.http, key.trim(), &text, lang).await?;
            data.db
                .db
                .query("UPSERT type::thing('translation', [$m, $l]) CONTENT $c")
                .bind(("m", msg.id.to_string()))
                .bind(("l", lang))
                .bind(("c", Cached { text: c.text.clone(), source: c.source.clone() }))
                .await?
                .check()?;
            c
        }
    };

    let same = lang.starts_with(result.source.as_str());
    let body = if same { format!("ℹ️ {}", if lang == "RU" { "Сообщение уже на твоём языке" } else { "Already in your language" }) } else { result.text };
    let embed = serenity::CreateEmbed::new()
        .description(body.chars().take(4096).collect::<String>())
        .color(0x5865F2)
        .footer(serenity::CreateEmbedFooter::new(format!("{} → {} · DeepL", result.source, lang)));
    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Cached;
    use crate::modules::testing;

    /// Composite ID translation:[message, language]: different languages are different records, a missing one is NONE
    #[tokio::test]
    async fn cache_by_array_id() {
        let (data, dir) = testing::data("translate", "").await;
        let get = "SELECT text, source FROM ONLY type::thing('translation', [$m, $l])";
        let put = "UPSERT type::thing('translation', [$m, $l]) CONTENT $c";
        let missing: Option<Cached> = data.db.db.query(get).bind(("m", "1")).bind(("l", "RU")).await.unwrap().take(0).unwrap();
        assert!(missing.is_none());
        let c = Cached { text: "Привет".into(), source: "EN".into() };
        data.db.db.query(put).bind(("m", "1")).bind(("l", "RU")).bind(("c", c)).await.unwrap().check().unwrap();
        let hit: Option<Cached> = data.db.db.query(get).bind(("m", "1")).bind(("l", "RU")).await.unwrap().take(0).unwrap();
        assert_eq!(hit.map(|c| (c.text, c.source)), Some(("Привет".into(), "EN".into())));
        let other: Option<Cached> = data.db.db.query(get).bind(("m", "1")).bind(("l", "DE")).await.unwrap().take(0).unwrap();
        assert!(other.is_none());
        drop(data);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn locales() {
        assert_eq!(super::target_lang("ru"), "RU");
        assert_eq!(super::target_lang("en-US"), "EN-US");
        assert_eq!(super::target_lang("es-419"), "ES");
        assert_eq!(super::target_lang("pt-BR"), "PT-BR");
        assert_eq!(super::target_lang("th"), "EN-US");
    }
}

/// Buttons `tr:<thread>:<message>` under info messages (placed by setup_server.py): the translation is a bot message
/// in the "🌍 …" thread under the original; the bot shows it to whoever clicked — only they see the reply.
/// The text is taken from the thread at click time: fix the translation there and the button returns the new one right away.
pub async fn on_component(ctx: &serenity::Context, data: &crate::Data, mci: &serenity::ComponentInteraction) {
    let mut parts = mci.data.custom_id.splitn(3, ':').skip(1).map(|p| p.parse::<u64>().ok());
    let (Some(Some(thread)), Some(Some(message))) = (parts.next(), parts.next()) else { return };
    let ru = mci.locale.starts_with("ru");
    let mut embeds: Vec<serenity::CreateEmbed> = Vec::new();
    let text = match serenity::ChannelId::new(thread).message(&ctx.http, serenity::MessageId::new(message)).await {
        // only the bot's own messages and only from this server — so the button doesn't turn into "show anything"
        Ok(m) if m.author.id == ctx.cache.current_user().id && m.guild_id.map_or(true, |g| g == data.guild) => {
            embeds = m.embeds.into_iter().map(Into::into).collect();
            m.content
        }
        Ok(_) => return,
        Err(e) => {
            eprintln!("[Translate] translation {}/{}: {}", thread, message, e);
            if ru { "Перевод не найден — открой ветку 🌍 под сообщением.".into() } else { "Translation not found — open the 🌍 thread under the message.".into() }
        }
    };
    let reply = serenity::CreateInteractionResponseMessage::new()
        .content(text)
        .ephemeral(true)
        .allowed_mentions(serenity::CreateAllowedMentions::new())
        .flags(if embeds.is_empty() {
            serenity::InteractionResponseFlags::EPHEMERAL | serenity::InteractionResponseFlags::SUPPRESS_EMBEDS
        } else {
            serenity::InteractionResponseFlags::EPHEMERAL
        })
        .embeds(embeds);
    if let Err(e) = mci.create_response(&ctx.http, serenity::CreateInteractionResponse::Message(reply)).await {
        eprintln!("[Translate] button response: {}", e);
    }
}
