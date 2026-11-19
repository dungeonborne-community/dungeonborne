//! Community Discord bot: modules are enabled by bot.toml sections, state is kept in the built-in SurrealDB.
//!
//! Run: `cargo run --release [-- config.toml]` (defaults to `bot.toml` or env `BOT_CONFIG`; the token goes in `.env`).

mod config;
mod db;
mod events;
mod modules;
mod names;

use poise::serenity_prelude as serenity;
use std::sync::Arc;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

/// Shared data: cheap Clone; module background tasks get their own copy
#[derive(Clone)]
pub struct Data {
    pub cfg: Arc<config::Config>,
    pub db: db::BotDb,
    pub guild: serenity::GuildId,
    pub names: Arc<names::Names>,
    pub server_name: String,
    pub http: reqwest::Client,
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    // config: first argument (`cargo run --release -- bot.prod.toml`), env BOT_CONFIG or bot.toml
    let path = std::env::args().nth(1).or_else(|| std::env::var("BOT_CONFIG").ok()).unwrap_or_else(|| "bot.toml".into());
    let cfg = match config::Config::load(&path) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("[Config] {}", e);
            std::process::exit(2);
        }
    };
    let token = match std::env::var(&cfg.discord.token_env) {
        Ok(t) if !t.trim().is_empty() => t,
        _ => {
            eprintln!("[Config] no bot token in variable {} (.env)", cfg.discord.token_env);
            std::process::exit(2);
        }
    };
    let db = match db::BotDb::init(&cfg.db.path).await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("[SurrealDB] {}", e);
            std::process::exit(1);
        }
    };

    // GUILD_MEMBERS and MESSAGE_CONTENT are privileged (Developer Portal → Bot → Privileged Gateway Intents),
    // so request them only for modules that need them
    let mut intents = serenity::GatewayIntents::GUILDS
        | serenity::GatewayIntents::GUILD_MODERATION
        | serenity::GatewayIntents::GUILD_VOICE_STATES
        | serenity::GatewayIntents::GUILD_MESSAGES;
    if cfg.mod_log.is_some() || cfg.welcome.is_some() {
        intents |= serenity::GatewayIntents::GUILD_MEMBERS;
    }
    if cfg.mod_log.is_some() || cfg.media_only.is_some() || cfg.chatmodes.is_some() {
        intents |= serenity::GatewayIntents::MESSAGE_CONTENT;
    }
    if cfg.invites.is_some() {
        intents |= serenity::GatewayIntents::GUILD_INVITES;
    }
    if cfg.starboard.is_some() {
        intents |= serenity::GatewayIntents::GUILD_MESSAGE_REACTIONS;
    }

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: modules::commands(&cfg),
            event_handler: |fw, event| Box::pin(events::handle(fw, event)),
            on_error: |err| Box::pin(on_error(err)),
            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            Box::pin(async move {
                let guild = serenity::GuildId::new(cfg.discord.guild_id);
                poise::builtins::register_in_guild(ctx, &framework.options().commands, guild).await?;
                let names = Arc::new(names::Names::load(&ctx.http, guild).await?);
                let server_name = guild.to_partial_guild(&ctx.http).await?.name;
                println!("[Discord] {} connected to '{}', commands: {}", ready.user.name, server_name, framework.options().commands.len());
                let data = Data {
                    cfg,
                    db,
                    guild,
                    names,
                    server_name,
                    http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build()?,
                };
                modules::start(ctx, &data).await;
                Ok(data)
            })
        })
        .build();

    // the message cache is needed by the moderation log: text of deleted messages and the old version of edited ones
    let mut cache = serenity::cache::Settings::default();
    cache.max_messages = 2000;
    let client = serenity::ClientBuilder::new(token, intents).framework(framework).cache_settings(cache).await;
    match client {
        Ok(mut client) => {
            if let Err(e) = client.start().await {
                eprintln!("[Discord] {}", e);
                if e.to_string().contains("4014") || e.to_string().to_lowercase().contains("disallowed") {
                    eprintln!("[Discord] enable Server Members Intent and Message Content Intent in Developer Portal → Bot");
                }
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("[Discord] {}", e);
            std::process::exit(1);
        }
    }
}

async fn on_error(err: poise::FrameworkError<'_, Data, Error>) {
    match err {
        poise::FrameworkError::Command { error, ctx, .. } => {
            eprintln!("[Command] /{}: {}", ctx.command().name, error);
            let _ = ctx
                .send(poise::CreateReply::default().content(format!("⚠️ {}", error)).ephemeral(true))
                .await;
        }
        other => {
            if let Err(e) = poise::builtins::on_error(other).await {
                eprintln!("[Discord] error handler failed: {}", e);
            }
        }
    }
}
