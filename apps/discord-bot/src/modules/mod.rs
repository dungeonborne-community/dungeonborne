//! Bot modules. Every module's schema is always applied (tables are cheap), but only modules with a section in bot.toml run.

pub mod announce;
pub mod chatmodes;
pub mod bug_reports;
pub mod creators;
pub mod embed;
pub mod events;
pub mod expire;
pub mod faq;
pub mod feeds;
pub mod giveaways;
pub mod invites;
pub mod media_only;
pub mod mod_log;
pub mod patch_notes;
pub mod patreon;
pub mod fund;
pub mod poll;
pub mod room;
pub mod move_forum;
pub mod roles;
pub mod starboard;
pub mod status;
pub mod temp_voice;
pub mod tickets;
pub mod timed;
pub mod translate;
pub mod welcome;

use crate::config::Config;
use crate::{Data, Error};
use poise::serenity_prelude as serenity;

pub const SCHEMAS: &[(&str, &str)] = &[
    ("mod_log", mod_log::SCHEMA),
    ("temp_voice", temp_voice::SCHEMA),
    ("timed", timed::SCHEMA),
    ("status", status::SCHEMA),
    ("fund", fund::SCHEMA),
    ("translate", translate::SCHEMA),
    ("tickets", tickets::SCHEMA),
    ("invites", invites::SCHEMA),
    ("starboard", starboard::SCHEMA),
    ("giveaways", giveaways::SCHEMA),
    ("feeds", feeds::SCHEMA),
    ("creators", creators::SCHEMA),
    ("announce", announce::SCHEMA),
    ("bug_reports", bug_reports::SCHEMA),
    ("patch_notes", patch_notes::SCHEMA),
    ("expire", expire::SCHEMA),
    ("events", events::SCHEMA),
    ("chatmodes", chatmodes::SCHEMA),
];

pub fn commands(cfg: &Config) -> Vec<poise::Command<Data, Error>> {
    let mut list = vec![embed::embed(), poll::poll()];
    if !cfg.role_panel.is_empty() {
        list.push(roles::rolepanel());
    }
    if cfg.translate.is_some() {
        list.push(translate::translate());
    }
    if cfg.tickets.is_some() {
        list.push(tickets::ticketpanel());
    }
    if cfg.invites.is_some() {
        list.push(invites::invites());
    }
    if cfg.giveaways.is_some() {
        list.push(giveaways::giveaway());
    }
    if cfg.creators.is_some() {
        list.push(creators::creator());
    }
    if cfg.announce.is_some() {
        list.push(announce::announce());
    }
    if cfg.move_to_forum.is_some() {
        list.push(move_forum::move_to_help());
    }
    if cfg.faq.is_some() {
        list.push(faq::faq());
    }
    if cfg.expire.is_some() {
        list.push(expire::delete_later());
    }
    if cfg.events.is_some() {
        list.push(events::event());
    }
    if cfg.chatmodes.is_some() {
        list.push(chatmodes::slowmode());
        list.push(chatmodes::emojimode());
    }
    if cfg.fund.is_some() {
        list.push(fund::fund());
        list.push(fund::supporter());
    }
    if cfg.temp_voice.is_some() {
        list.push(room::room());
    }
    if cfg.discord.admin_only {
        for cmd in &mut list {
            cmd.default_member_permissions = serenity::Permissions::ADMINISTRATOR;
        }
    }
    list
}

/// Background tasks of enabled modules
pub async fn start(ctx: &serenity::Context, data: &Data) {
    let cfg = &data.cfg;
    let on = |b: bool| if b { "on" } else { "—" };
    println!(
        "[Modules] log: {} · welcome: {} · voice rooms: {} · role panels: {} · timed: {} · status: {} · translate: {}",
        on(cfg.mod_log.is_some()),
        on(cfg.welcome.is_some()),
        on(cfg.temp_voice.is_some()),
        cfg.role_panel.len(),
        cfg.timed.len(),
        on(cfg.status.is_some()),
        on(cfg.translate.is_some()),
    );
    println!("[Modules] fund: {}", on(cfg.fund.is_some()));
    println!(
        "[Modules] tickets: {} · invites: {} · starboard: {} · giveaways: {} · feeds: {} · twitch: {} · creators: {} · announce: {} · to forum: {} · faq: {} · bug reports: {} · patch notes: {} · images only: {} · timed delete: {} · events: {} · channel modes: {}",
        on(cfg.tickets.is_some()),
        on(cfg.invites.is_some()),
        on(cfg.starboard.is_some()),
        on(cfg.giveaways.is_some()),
        cfg.feed.len(),
        on(cfg.twitch.is_some()),
        on(cfg.creators.is_some()),
        on(cfg.announce.is_some()),
        on(cfg.move_to_forum.is_some()),
        on(cfg.faq.is_some()),
        on(cfg.bug_reports.is_some()),
        on(cfg.patch_notes.is_some()),
        on(cfg.media_only.is_some()),
        on(cfg.expire.is_some()),
        on(cfg.events.is_some()),
        on(cfg.chatmodes.is_some()),
    );
    mod_log::spawn(ctx.clone(), data.clone());
    if cfg.temp_voice.is_some() {
        temp_voice::spawn_cleanup(ctx.clone(), data.clone());
    }
    if !cfg.timed.is_empty() {
        timed::spawn(ctx.clone(), data.clone());
    }
    status::spawn(ctx.clone(), data.clone());
    fund::spawn(ctx.clone(), data.clone());
    if cfg.invites.is_some() {
        invites::snapshot(ctx, data).await;
    }
    if cfg.giveaways.is_some() {
        giveaways::spawn(ctx.clone(), data.clone());
    }
    feeds::spawn(ctx.clone(), data.clone());
    bug_reports::spawn(ctx.clone(), data.clone());
    patch_notes::spawn(ctx.clone(), data.clone());
    expire::spawn(ctx.clone(), data.clone());
    events::spawn(ctx.clone(), data.clone());
    chatmodes::spawn(ctx.clone(), data.clone());
}

#[cfg(test)]
pub mod testing {
    use crate::config::Config;
    use crate::{db::BotDb, names::Names, Data};
    use poise::serenity_prelude as serenity;
    use std::sync::Arc;

    /// Real SurrealKV in a temp folder + a config with the log enabled; the caller deletes the folder
    pub async fn data(name: &str, extra_toml: &str) -> (Data, String) {
        let dir = format!("target/test_db_{}_{}", name, std::process::id());
        let _ = std::fs::remove_dir_all(&dir);
        let toml = format!("[discord]\nguild_id = 1\n[mod_log]\nchannel = \"1\"\n{}", extra_toml);
        let cfg: Config = toml::from_str(&toml).unwrap();
        let db = BotDb::init(&dir).await.unwrap();
        let data = Data {
            cfg: Arc::new(cfg),
            db,
            guild: serenity::GuildId::new(1),
            names: Arc::new(Names::empty()),
            server_name: "Test".into(),
            http: reqwest::Client::new(),
        };
        (data, dir)
    }

    pub fn user(id: u64, name: &str) -> serenity::User {
        let mut u = serenity::User::default();
        u.id = serenity::UserId::new(id);
        u.name = name.into();
        u
    }
}
