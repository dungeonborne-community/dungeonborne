//! bot.toml. Each module has its own section; no section means the module is off.
//! Channels and roles can be given by name (as in Discord) or by ID.

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub discord: DiscordCfg,
    #[serde(default)]
    pub db: DbCfg,
    pub mod_log: Option<ModLogCfg>,
    pub welcome: Option<WelcomeCfg>,
    pub temp_voice: Option<TempVoiceCfg>,
    #[serde(default)]
    pub role_panel: Vec<RolePanelCfg>,
    #[serde(default)]
    pub timed: Vec<TimedCfg>,
    pub status: Option<StatusCfg>,
    /// Server fundraising (old section name: [patreon])
    #[serde(alias = "patreon")]
    pub fund: Option<FundCfg>,
    pub translate: Option<TranslateCfg>,
    pub tickets: Option<TicketsCfg>,
    pub invites: Option<InvitesCfg>,
    pub starboard: Option<StarboardCfg>,
    pub giveaways: Option<GiveawaysCfg>,
    #[serde(default)]
    pub feed: Vec<FeedCfg>,
    pub twitch: Option<TwitchCfg>,
    pub creators: Option<CreatorsCfg>,
    pub announce: Option<AnnounceCfg>,
    pub move_to_forum: Option<MoveToForumCfg>,
    pub faq: Option<FaqCfg>,
    pub bug_reports: Option<BugReportsCfg>,
    pub patch_notes: Option<PatchNotesCfg>,
    pub media_only: Option<MediaOnlyCfg>,
    pub expire: Option<ExpireCfg>,
    pub events: Option<EventsCfg>,
    pub chatmodes: Option<ChatModesCfg>,
    /// Translation languages (like languages in server.yaml, same order): the "🌍 …" thread, /announce, /faq
    #[serde(default)]
    pub languages: Vec<AnnounceLang>,
}

/// "Move to support" in the message menu: a question from chat → a post in the help forum
#[derive(Deserialize)]
pub struct MoveToForumCfg {
    pub forum: String,
    /// Who can move messages (besides them: the Manage Messages permission)
    #[serde(default)]
    pub roles: Vec<String>,
    /// Post language tag by the source channel; if not found, default_tag
    #[serde(default)]
    pub lang_tags: Vec<LangTagCfg>,
    pub default_tag: String,
}

#[derive(Deserialize)]
pub struct LangTagCfg {
    pub tag: String,
    pub channels: Vec<String>,
}

/// Bug reports from the game and the launcher: the bot pulls them from the backend admin API (docs/bug-report-api.md)
#[derive(Deserialize)]
pub struct BugReportsCfg {
    pub forum: String,
    /// Base URL of the bug reports admin API, e.g. http://127.0.0.1:9092/admin/v1/bug-reports
    pub api: String,
    #[serde(default)]
    pub bearer_env: Option<String>,
    #[serde(default = "default_bug_every")]
    pub every_secs: u64,
    /// report kind → forum tag (crash → Crash …)
    #[serde(default)]
    pub kind_tags: std::collections::HashMap<String, String>,
    /// client language → tag; the rest get default_lang_tag
    #[serde(default)]
    pub lang_tags: std::collections::HashMap<String, String>,
    pub default_lang_tag: String,
}

fn default_bug_every() -> u64 {
    20
}

/// Server changelog (the same one as in the launcher and on the site) → patch notes channel
#[derive(Deserialize)]
pub struct PatchNotesCfg {
    pub channel: String,
    /// Backend GET, format dbnotes/1: http://127.0.0.1:<web bind>/dbserver/patch_notes
    pub url: String,
    #[serde(default = "default_pn_every")]
    pub every_secs: u64,
    /// Post language; the other languages go to the translations thread
    #[serde(default = "default_pn_lang")]
    pub main_lang: String,
    /// Subscription role to ping (none = no ping)
    #[serde(default)]
    pub ping: Option<String>,
    /// Last line of the post (small text)
    #[serde(default = "default_pn_footer")]
    pub footer: String,
    /// First run: how many latest entries to publish anyway (the rest of the history is skipped silently)
    #[serde(default)]
    pub backfill: usize,
}

fn default_pn_every() -> u64 {
    300
}
fn default_pn_lang() -> String {
    "en".into()
}
fn default_pn_footer() -> String {
    "Same notes in the launcher and on the site".into()
}

/// "Images and GIFs only" channels: the bot deletes everything else with a short hint
#[derive(Deserialize)]
pub struct MediaOnlyCfg {
    pub channels: Vec<String>,
    /// Who is exempt (besides the Manage Messages permission)
    #[serde(default)]
    pub exempt_roles: Vec<String>,
    /// Hint; {user} = mention, {#channel} = channel link
    pub notice: String,
    /// Seconds before the hint disappears
    #[serde(default = "default_notice_secs")]
    pub notice_secs: u64,
}

fn default_notice_secs() -> u64 {
    10
}

/// Timed message deletion: "Delete later", delete_after of /announce, script requests in the queue folder
#[derive(Deserialize)]
pub struct ExpireCfg {
    #[serde(default = "default_expire_queue")]
    pub queue: String,
}

fn default_expire_queue() -> String {
    "data/expire_queue".into()
}

/// /event: a Discord scheduled event + a temporary category from a template, a participant role, auto-close to the archive
#[derive(Deserialize)]
pub struct EventsCfg {
    /// Where text channels of a closed event are moved
    pub archive: String,
    /// Who can write in the event's read-only channels and see the archive
    #[serde(default)]
    pub staff_roles: Vec<String>,
    /// Time zone in which times are entered in the form, and its label
    #[serde(default = "default_utc_offset")]
    pub utc_offset: String,
    #[serde(default = "default_tz_label")]
    pub tz_label: String,
    pub template: Vec<EventTemplate>,
}

#[derive(Deserialize)]
pub struct EventTemplate {
    pub name: String,
    /// Temporary category name; {name} / {NAME} = event name
    #[serde(default = "default_event_category")]
    pub category: String,
    /// Participant role given by the join button
    #[serde(default)]
    pub role: bool,
    #[serde(default = "default_event_role")]
    pub role_name: String,
    /// Location (for an "elsewhere" event)
    #[serde(default)]
    pub location: Option<String>,
    /// Stage channel for a broadcast: the event is held there, without a temporary category
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub channels: Vec<EventChannel>,
}

#[derive(Deserialize)]
pub struct EventChannel {
    pub name: String,
    /// text | voice
    #[serde(default = "default_event_kind")]
    pub kind: String,
    /// Only the team can write
    #[serde(default)]
    pub readonly: bool,
    /// The rules from the form and the join button go here
    #[serde(default)]
    pub rules: bool,
    #[serde(default)]
    pub user_limit: Option<u32>,
    #[serde(default)]
    pub topic: Option<String>,
}

fn default_utc_offset() -> String {
    "+03:00".into()
}
fn default_tz_label() -> String {
    "МСК / UTC+3".into()
}
fn default_event_category() -> String {
    "🎉 {NAME}".into()
}
fn default_event_role() -> String {
    "🎟 {name}".into()
}
fn default_event_kind() -> String {
    "text".into()
}

/// /slowmode and /emojimode: channel modes for moderators
#[derive(Deserialize)]
pub struct ChatModesCfg {
    /// Who is exempt from emoji-only mode
    #[serde(default)]
    pub exempt_roles: Vec<String>,
}

/// /faq: answer with a section from the FAQ channel
#[derive(Deserialize)]
pub struct FaqCfg {
    pub channel: String,
    /// Who answers publicly in the channel (others see the answer only themselves); besides them: the Manage Messages permission
    #[serde(default)]
    pub public_roles: Vec<String>,
}

/// /announce: a post to a news channel with a preview, a ping of one role and translations in the "🌍 …" thread
#[derive(Deserialize)]
pub struct AnnounceCfg {
    /// Translations come from the shared languages; the form fits up to 4
    pub channel: Vec<AnnounceChannelCfg>,
}

#[derive(Deserialize, Clone)]
pub struct AnnounceLang {
    pub code: String,
    pub flag: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct AnnounceChannelCfg {
    pub name: String,
    /// Roles that may be pinged in this channel (besides them, only @everyone, with a warning)
    #[serde(default)]
    pub pings: Vec<String>,
    /// Prefilled text in the form
    #[serde(default)]
    pub template: Option<String>,
    /// Announcement channel: crosspost to following servers after publishing
    #[serde(default = "yes")]
    pub crosspost: bool,
    /// Copies in other languages: language code → channel in that language's category (e.g. ru = "📢・анонсы").
    /// The whole translated post goes there, without a ping, with a link to the original; if the original is deleted, the bot deletes the copy.
    #[serde(default)]
    pub mirror: std::collections::BTreeMap<String, String>,
}

fn yes() -> bool {
    true
}

/// Content creators announce themselves: a button panel in a private channel → form → announcement in the announcements channel
#[derive(Deserialize)]
pub struct CreatorsCfg {
    /// Announcements channel (🔴・live-now)
    pub channel: String,
    /// Where the button panel lives (🎬・creator-hub, visible only to creators)
    pub hub: String,
    /// Creator role: only it (and administrators) can announce; /creator add grants it, /creator remove takes it away
    pub role: String,
    /// Role pinged in announcements (opt-in via Channels & Roles)
    #[serde(default)]
    pub ping: Option<String>,
    /// Cooldowns are SurrealDB durations (30m, 4h, 1d), separate for streams and videos
    #[serde(default = "default_stream_cooldown")]
    pub stream_cooldown: String,
    #[serde(default = "default_video_cooldown")]
    pub video_cooldown: String,
    /// Allowed platforms: a link to any other domain is rejected (protection against phishing in the bot's name)
    #[serde(default = "default_allowed_sites")]
    pub allowed: Vec<String>,
}

fn default_stream_cooldown() -> String {
    "4h".into()
}
fn default_video_cooldown() -> String {
    "8h".into()
}
fn default_allowed_sites() -> Vec<String> {
    ["youtube.com", "youtu.be", "twitch.tv", "vkvideo.ru", "vk.com", "kick.com", "tiktok.com", "trovo.live", "rutube.ru"]
        .map(String::from)
        .to_vec()
}

#[derive(Deserialize)]
pub struct TicketsCfg {
    /// Channel with the panel; private ticket threads are created in it
    pub channel: String,
    /// Where to send the transcript of a closed ticket (usually the moderation log)
    pub transcripts: String,
    pub button: Vec<TicketButtonCfg>,
}

#[derive(Deserialize)]
pub struct TicketButtonCfg {
    pub lang: String,
    pub label: String,
    #[serde(default)]
    pub emoji: Option<String>,
    /// Role (or list of roles) the ticket calls: the mention adds them to the private thread
    pub staff: OneOrMany,
    /// Custom form labels (e.g. for a creator application); defaults to Ticket / Subject / Details (in Russian)
    #[serde(default)]
    pub form_title: Option<String>,
    #[serde(default)]
    pub topic_label: Option<String>,
    #[serde(default)]
    pub details_label: Option<String>,
    /// A custom questionnaire instead of "subject + details" (up to 5 fields, labels up to 45 characters): e.g. a Helper application.
    /// The ticket subject is form_title (or label); the answers become embed fields in the thread.
    #[serde(default)]
    pub fields: Vec<TicketFieldCfg>,
}

#[derive(Deserialize)]
pub struct TicketFieldCfg {
    pub label: String,
    /// true = paragraph, false = single line
    #[serde(default)]
    pub long: bool,
    #[serde(default = "yes")]
    pub required: bool,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub max: Option<u16>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn list(&self) -> Vec<&str> {
        match self {
            OneOrMany::One(s) => vec![s.as_str()],
            OneOrMany::Many(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

/// Section without fields: `[invites]` turns the tracker on
#[derive(Deserialize)]
pub struct InvitesCfg {}

#[derive(Deserialize)]
pub struct StarboardCfg {
    pub channel: String,
    #[serde(default = "default_star")]
    pub emoji: String,
    #[serde(default = "default_threshold")]
    pub threshold: u32,
    #[serde(default)]
    pub ignore: Vec<String>,
}

#[derive(Deserialize)]
pub struct GiveawaysCfg {}

/// Atom/RSS feed: a YouTube channel, GitHub releases or any URL
#[derive(Deserialize)]
pub struct FeedCfg {
    pub name: String,
    pub channel: String,
    /// YouTube channel ID (UC…)
    #[serde(default)]
    pub youtube: Option<String>,
    /// owner/repo: GitHub releases
    #[serde(default)]
    pub github: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default = "default_feed_text")]
    pub text: String,
    /// Role to mention in the post
    #[serde(default)]
    pub ping: Option<String>,
    #[serde(default = "default_feed_every")]
    pub every_mins: u64,
}

impl FeedCfg {
    pub fn feed_url(&self) -> Option<String> {
        if let Some(id) = &self.youtube {
            return Some(format!("https://www.youtube.com/feeds/videos.xml?channel_id={}", id));
        }
        if let Some(repo) = &self.github {
            return Some(format!("https://github.com/{}/releases.atom", repo));
        }
        self.url.clone()
    }
}

#[derive(Deserialize)]
pub struct TwitchCfg {
    #[serde(default = "default_twitch_id_env")]
    pub client_id_env: String,
    #[serde(default = "default_twitch_secret_env")]
    pub client_secret_env: String,
    pub channel: String,
    pub logins: Vec<String>,
    #[serde(default = "default_twitch_text")]
    pub text: String,
    #[serde(default)]
    pub ping: Option<String>,
    #[serde(default = "default_twitch_every")]
    pub every_secs: u64,
}

fn default_star() -> String {
    "⭐".into()
}
fn default_threshold() -> u32 {
    3
}
fn default_feed_text() -> String {
    "**{title}**\n{link}".into()
}
fn default_feed_every() -> u64 {
    10
}
fn default_twitch_id_env() -> String {
    "TWITCH_CLIENT_ID".into()
}
fn default_twitch_secret_env() -> String {
    "TWITCH_CLIENT_SECRET".into()
}
fn default_twitch_text() -> String {
    "🔴 **{name}** is live / в эфире: {title}\nhttps://twitch.tv/{login}".into()
}
fn default_twitch_every() -> u64 {
    120
}

#[derive(Deserialize)]
pub struct DiscordCfg {
    pub guild_id: u64,
    /// Environment variable holding the bot token
    #[serde(default = "default_token_env")]
    pub token_env: String,
    /// Staging: all bot commands are visible only to server administrators
    #[serde(default)]
    pub admin_only: bool,
}

#[derive(Deserialize)]
pub struct DbCfg {
    pub path: String,
}

impl Default for DbCfg {
    fn default() -> Self {
        Self { path: "data/bot.db".into() }
    }
}

#[derive(Deserialize)]
pub struct ModLogCfg {
    pub channel: String,
}

#[derive(Deserialize)]
pub struct WelcomeCfg {
    pub greet: Vec<GreetCfg>,
}

/// A member got a role (e.g. a language role from onboarding) → the bot greets them once in the channel.
/// Without role: greet everyone after they finish onboarding.
#[derive(Deserialize)]
pub struct GreetCfg {
    #[serde(default)]
    pub role: Option<String>,
    pub channel: String,
    pub text: String,
}

#[derive(Deserialize)]
pub struct TempVoiceCfg {
    /// Default room name for all generators
    #[serde(default = "default_room_name")]
    pub name: String,
    /// Generators `[[temp_voice.rooms]]`: own channel, name and user limit (Duo = 2, Trio = 3)
    pub rooms: Vec<TempRoomCfg>,
}

#[derive(Deserialize, Clone)]
pub struct TempRoomCfg {
    /// Generator channel: join it and the bot creates a room next to it and moves you there
    pub generator: String,
    /// Room name; defaults to the shared `name` from [temp_voice]
    #[serde(default)]
    pub name: Option<String>,
    /// 0 = no limit, the room owner changes it
    #[serde(default)]
    pub user_limit: u32,
}

#[derive(Deserialize)]
pub struct RolePanelCfg {
    pub name: String,
    pub channel: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub buttons: Vec<RoleButtonCfg>,
}

#[derive(Deserialize)]
pub struct RoleButtonCfg {
    pub role: String,
    pub label: String,
    #[serde(default)]
    pub emoji: Option<String>,
}

#[derive(Deserialize)]
pub struct TimedCfg {
    pub name: String,
    pub channel: String,
    /// SurrealDB duration: 30m, 12h, 1d, 1w
    pub every: String,
    pub text: String,
    /// Delete the previous message of this reminder so they don't pile up
    #[serde(default = "yes")]
    pub replace_previous: bool,
}

#[derive(Deserialize)]
pub struct StatusCfg {
    pub channel: String,
    /// Public backend status: GET /dbserver/status → {online, capacity, maintenance, motd, …}, no token.
    /// If set, it is used and players_url is not needed.
    #[serde(default)]
    pub status_url: Option<String>,
    /// Accept a self-signed status_url certificate (testing from a PC: the VM certificate is not for this domain). On the VM use http://127.0.0.1:8090 without this.
    #[serde(default)]
    pub accept_invalid_certs: bool,
    /// Backend admin API: GET → JSON array of online players (old way, VM only)
    #[serde(default)]
    pub players_url: Option<String>,
    /// Environment variable with the admin Bearer token (needed with [auth] require = "relay")
    #[serde(default)]
    pub bearer_env: Option<String>,
    #[serde(default = "default_status_every")]
    pub every_secs: u64,
    pub title: String,
    #[serde(default)]
    pub capacity: Option<u32>,
    #[serde(default)]
    pub address: Option<String>,
}

/// Server fundraising: Patreon + manual amounts against a monthly goal, one-off goals, supporters (fund.rs)
#[derive(Deserialize)]
pub struct FundCfg {
    pub channel: String,
    /// Monthly goal in $ (server costs)
    pub goal: f64,
    #[serde(default = "default_fund_currency")]
    pub currency: String,
    #[serde(default = "default_patreon_title")]
    pub title: String,
    /// Line above the progress bar: what the money is for
    #[serde(default)]
    pub text: Option<String>,
    /// Breakdown of the monthly cost (optional)
    #[serde(default)]
    pub breakdown: Vec<FundPart>,
    /// Patreon button; without it, the campaign URL from the API
    #[serde(default, alias = "link")]
    pub patreon_link: Option<String>,
    #[serde(default)]
    pub boosty_link: Option<String>,
    /// Boosty blog (boosty.to/<blog>): the bot reads the public goal and computes this month's increase
    #[serde(default)]
    pub boosty_blog: Option<String>,
    /// Server reserve: how much of the surplus to save (defaults to the monthly goal); anything above goes to the team as thanks
    #[serde(default)]
    pub reserve_cap: Option<f64>,
    /// Boosty goal id; without it, the blog's first money goal
    #[serde(default)]
    pub boosty_target_id: Option<u64>,
    /// Patreon/Boosty still under review: no links anywhere (links in support.json are empty), the card shows "Coming soon" instead
    #[serde(default)]
    pub coming_soon: bool,
    /// Supporter role: only members with it can use `/supporter show` (unset = anyone)
    #[serde(default)]
    pub supporter_role: Option<String>,
    /// support.json for the site and the launcher (on the VM: /var/www/status/support.json, nginx serves /dbserver/support)
    #[serde(default)]
    pub json_path: Option<String>,
    #[serde(default = "default_patreon_token_env")]
    pub token_env: String,
    /// How often to poll Patreon
    #[serde(default = "default_patreon_every")]
    pub every_secs: u64,
}

#[derive(Deserialize)]
pub struct FundPart {
    pub en: String,
    pub ru: String,
    pub usd: f64,
}

fn default_fund_currency() -> String {
    "USD".into()
}
fn default_patreon_title() -> String {
    "💜 Servers this month / Серверы в этом месяце".into()
}
fn default_patreon_token_env() -> String {
    "PATREON_TOKEN".into()
}
fn default_patreon_every() -> u64 {
    3600
}

#[derive(Deserialize)]
pub struct TranslateCfg {
    #[serde(default = "default_deepl_env")]
    pub deepl_key_env: String,
}

fn default_token_env() -> String {
    "DISCORD_TOKEN".into()
}
fn default_room_name() -> String {
    "🔊 {user}".into()
}
fn default_status_every() -> u64 {
    60
}
fn default_deepl_env() -> String {
    "DEEPL_KEY".into()
}
impl Config {
    pub fn load(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path, e))?;
        toml::from_str(&text).map_err(|e| format!("{}: {}", path, e))
    }
}

#[cfg(test)]
mod real_configs {
    /// The example config (and a local bot.prod.toml, if present) parses without errors
    #[test]
    fn bot_toml_and_prod_parse() {
        for f in ["bot.toml", "bot.prod.toml"] {
            if f != "bot.toml" && !std::path::Path::new(f).exists() {
                continue;
            }
            let text = std::fs::read_to_string(f).unwrap_or_else(|e| panic!("{}: {}", f, e));
            let cfg: super::Config = toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {}", f, e));
            assert!(cfg.fund.is_some() && cfg.status.is_some(), "{}", f);
        }
    }
}
