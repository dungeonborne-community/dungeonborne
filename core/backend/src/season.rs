//! Seasons (docs/SEASONS_FORMAT.md): core flags from `seasons/base.toml` and the active season's manifest
//! `seasons/<id>/season.toml`. A season is enabled only by the `SEASON=<id>` variable (`SEASONS_DIR` — a different root).
//! Flag value: from the season manifest, otherwise from base.toml, otherwise off. Read once at startup.
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Reward for killing a player: `per_kill` units of currency `currency` by mail after the killer's raid
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct KillReward {
    pub currency: i32,
    pub per_kill: i64,
}

/// Gear insurance (08.11, as in the original with the user's normalisation): price = item market price × rarity
/// percent / 100. Market price — average trade price over a week, without trades — price_def × `market_mult`. Percent 0 —
/// cannot be insured. The game's gear_insurance_item table (sp_rate) is considered a developer mistake.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Insurance {
    pub market_mult: f64,
    pub rotted: u32,
    pub normal: u32,
    pub uncommon: u32,
    pub enhanced: u32,
    pub epic: u32,
    pub exceptional: u32,
    pub legendary: u32,
    pub unique: u32,
}

impl Default for Insurance {
    fn default() -> Self {
        Insurance { market_mult: 3.0, rotted: 0, normal: 0, uncommon: 35, enhanced: 30, epic: 25, exceptional: 22, legendary: 20, unique: 15 }
    }
}

/// `[features.stash]` (owner's item 1, 18.11): a new account gets `base_pages` stash pages; each next page — the product
/// "Exquisite box" (720001, item 1032 auto-used) for `page_price` gold (0 — the game's price ladder market_special_cost),
/// up to `max_pages` (0 — const inventory_max_page = 8). Without the section — as before: 8 pages, nothing to buy.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Stash {
    pub base_pages: i64,
    pub page_price: i64,
    pub max_pages: i64,
}

impl Default for Stash {
    fn default() -> Self {
        Stash { base_pages: 8, page_price: 0, max_pages: 0 }
    }
}

impl Insurance {
    /// Percent for EnItemRarity (0 — not insurable)
    pub fn pct(&self, rarity: i32) -> u32 {
        use crate::tgf::EnItemRarity as R;
        match R::try_from(rarity) {
            Ok(R::Rotted) => self.rotted,
            Ok(R::Normal) => self.normal,
            Ok(R::Uncommon) => self.uncommon,
            Ok(R::Enhanced) => self.enhanced,
            Ok(R::Epic) => self.epic,
            Ok(R::Exceptional) => self.exceptional,
            Ok(R::Legendary) => self.legendary,
            Ok(R::Unique) => self.unique,
            _ => 0,
        }
    }
}

/// Coin or material given once at login
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Grant {
    pub item: i32,
    pub count: i64,
}

/// Flags in the file: missing ones are unset (taken from the lower layer)
#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
struct FeaturesToml {
    auto_sell_extracted: Option<bool>,
    relic_lower_quality: Option<bool>,
    kill_reward: Option<KillReward>,
    start_materials: Option<Vec<Grant>>,
    start_gold: Option<i64>,
    merchant_refresh_secs: Option<i64>,
    mixed_party_rooms: Option<bool>,
    insurance: Option<Insurance>,
    stash: Option<Stash>, // ext: stash_pages
}

impl FeaturesToml {
    /// `self` on top of `base`
    fn over(self, base: FeaturesToml) -> FeaturesToml {
        FeaturesToml {
            auto_sell_extracted: self.auto_sell_extracted.or(base.auto_sell_extracted),
            relic_lower_quality: self.relic_lower_quality.or(base.relic_lower_quality),
            kill_reward: self.kill_reward.or(base.kill_reward),
            start_materials: self.start_materials.or(base.start_materials),
            start_gold: self.start_gold.or(base.start_gold),
            merchant_refresh_secs: self.merchant_refresh_secs.or(base.merchant_refresh_secs),
            mixed_party_rooms: self.mixed_party_rooms.or(base.mixed_party_rooms),
            insurance: self.insurance.or(base.insurance),
            stash: self.stash.or(base.stash), // ext: stash_pages
        }
    }
}

/// Effective core flags
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Features {
    /// on leaving a raid, sell what was extracted in bags (own items from before the raid and equipped ones are not sold)
    pub auto_sell_extracted: bool,
    /// crafting a lower-quality relic if a higher one is unlocked (the client needs the zz_RelicQuality_P pak)
    pub relic_lower_quality: bool,
    /// reward for killing a player; None or per_kill <= 0 — off
    pub kill_reward: Option<KillReward>,
    /// forge materials, once at login (state.playtest_materials)
    pub start_materials: Vec<Grant>,
    /// new player's gold; None — structures::START_GOLD
    pub start_gold: Option<i64>,
    /// merchant tab refresh period (s); 0 — as in the tab config (market_tab_info.period)
    pub merchant_refresh_secs: i64,
    /// rooms not strictly solo/duo/trio: room size is a ceiling (a duo room admits duos and solos, trio — everyone)
    pub mixed_party_rooms: bool,
    /// gear insurance: percents per rarity and market multiplier
    pub insurance: Insurance,
    /// stash pages: base for a new account, price of the next page, max (owner's decision 18.11)
    pub stash: Stash, // ext: stash_pages
}

impl From<FeaturesToml> for Features {
    fn from(t: FeaturesToml) -> Self {
        Features {
            auto_sell_extracted: t.auto_sell_extracted.unwrap_or(false),
            relic_lower_quality: t.relic_lower_quality.unwrap_or(false),
            kill_reward: t.kill_reward.filter(|k| k.per_kill > 0),
            start_materials: t.start_materials.unwrap_or_default(),
            start_gold: t.start_gold.filter(|g| *g >= 0),
            merchant_refresh_secs: t.merchant_refresh_secs.filter(|s| *s > 0).unwrap_or(0),
            mixed_party_rooms: t.mixed_party_rooms.unwrap_or(false),
            insurance: t.insurance.unwrap_or_default(),
            stash: t.stash.unwrap_or_default(), // ext: stash_pages
        }
    }
}

#[derive(Deserialize, Debug)]
struct BaseToml {
    #[serde(default)]
    features: FeaturesToml,
    #[serde(default)]
    client: Option<toml::Table>,
}

/// 12.11: client mod settings `[client.<mod>.<section>]` (e.g. `[client.DungeonborneRuClient.stealth]`) —
/// go into the `dbserver/1` manifest (`client_settings`), the launcher writes them to `Mods/<mod>/server_settings.ini` before launch.
/// Season on top of the base: season keys replace base keys, the rest stay.
fn merge_client(base: Option<toml::Table>, over: Option<toml::Table>) -> serde_json::Value {
    fn merge(b: &mut toml::Table, o: toml::Table) {
        for (k, v) in o {
            match (b.get_mut(&k), v) {
                (Some(toml::Value::Table(bt)), toml::Value::Table(ot)) => merge(bt, ot),
                (_, v) => {
                    b.insert(k, v);
                }
            }
        }
    }
    let mut t = base.unwrap_or_default();
    if let Some(o) = over {
        merge(&mut t, o);
    }
    serde_json::to_value(&t).unwrap_or_else(|_| serde_json::json!({}))
}

#[derive(Deserialize, Debug)]
struct Manifest {
    id: String,
    /// a string or `{ ru = "…", en = "…" }` (src/text.rs)
    name: crate::text::Text,
    /// season rules module (src/season/<plugin>.rs); there are no modules yet — only the empty one
    #[serde(default)]
    plugin: String,
    #[serde(default)]
    features: FeaturesToml,
    #[serde(default)]
    client: Option<toml::Table>,
    /// the rest of the manifest (config, state_key, [server]) is not needed by the core yet
    #[serde(flatten)]
    _rest: std::collections::BTreeMap<String, toml::Value>,
}

#[derive(Debug, Default)]
pub struct Active {
    /// id of the active season; None — base game
    pub id: Option<String>,
    /// name in the primary language
    pub name: String,
    /// name with translations (the changelog serves it in the launcher's language)
    pub title: crate::text::Text,
    pub features: Features,
    /// client mod settings (`[client]` of the base and season, see `merge_client`), JSON object
    pub client: serde_json::Value,
}

fn root() -> PathBuf {
    PathBuf::from(std::env::var("SEASONS_DIR").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "seasons".into()))
}

/// Folder of season `id` (taking `SEASONS_DIR` into account)
pub fn dir(id: &str) -> PathBuf {
    root().join(id)
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &std::path::Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    toml::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("{}: {}", path.display(), e))
}

/// base.toml: no file — all flags off; parse error — also off (with the reason in the log)
fn load_base(root: &std::path::Path) -> (FeaturesToml, Option<toml::Table>) {
    let path = root.join("base.toml");
    if !path.exists() {
        return (FeaturesToml::default(), None);
    }
    match read_toml::<BaseToml>(&path) {
        Ok(b) => (b.features, b.client),
        Err(e) => {
            println!("[Season] {} — base flags off", e);
            (FeaturesToml::default(), None)
        }
    }
}

/// Season `id` on top of the base; Err — the reason the season was not enabled
fn load_season(root: &std::path::Path, id: &str, base: FeaturesToml, base_client: Option<toml::Table>) -> Result<Active, String> {
    let m: Manifest = read_toml(&root.join(id).join("season.toml"))?;
    if m.id != id {
        return Err(format!("manifest has id = \"{}\", but the folder is {}", m.id, id));
    }
    if !m.plugin.is_empty() {
        return Err(format!("season module \"{}\" does not exist", m.plugin));
    }
    Ok(Active {
        id: Some(m.id),
        name: m.name.base().to_string(),
        title: m.name,
        features: m.features.over(base).into(),
        client: merge_client(base_client, m.client),
    })
}

pub fn load(root: &std::path::Path, season: Option<&str>) -> Active {
    let (base, base_client) = load_base(root);
    let plain = |base: FeaturesToml, c: Option<toml::Table>| Active {
        id: None,
        name: String::new(),
        title: Default::default(),
        features: base.into(),
        client: merge_client(c, None),
    };
    match season {
        Some(id) => load_season(root, id, base.clone(), base_client.clone()).unwrap_or_else(|e| {
            println!("[Season] season {} not enabled: {} — base game", id, e);
            plain(base, base_client)
        }),
        None => plain(base, base_client),
    }
}

static ACTIVE: OnceLock<Active> = OnceLock::new();

pub fn active() -> &'static Active {
    ACTIVE.get_or_init(|| {
        let id = std::env::var("SEASON").ok().filter(|s| !s.is_empty());
        let a = load(&root(), id.as_deref());
        let f = &a.features;
        println!(
            "[Season] {}; auto-sell {}, lower relics {}, kill reward {}, materials at login {}, starting gold {}, merchant {}, rooms {}, stash {}",
            match &a.id {
                Some(id) => format!("active \"{}\" ({}, folder {})", a.name, id, root().join(id).display()),
                None => "no season — base game".into(),
            },
            on(f.auto_sell_extracted),
            on(f.relic_lower_quality),
            f.kill_reward.as_ref().map(|k| format!("{} × {}", k.per_kill, k.currency)).unwrap_or_else(|| "off".into()),
            if f.start_materials.is_empty() { "none".into() } else { format!("{} kinds", f.start_materials.len()) },
            f.start_gold.map(|g| g.to_string()).unwrap_or_else(|| "default".into()),
            if f.merchant_refresh_secs > 0 { format!("every {} s", f.merchant_refresh_secs) } else { "per config".into() },
            if f.mixed_party_rooms { "mixed (group not larger than the room)" } else { "by group size" },
            // ext: stash_pages
            format!("{} pages, next {} gold, max {}", f.stash.base_pages, if f.stash.page_price > 0 { f.stash.page_price.to_string() } else { "table".into() }, if f.stash.max_pages > 0 { f.stash.max_pages.to_string() } else { "const".into() }),
        );
        a
    })
}

fn on(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

pub fn features() -> &'static Features {
    &active().features
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("season_test_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn client_settings_season_over_base() {
        let d = dir("client");
        std::fs::write(
            d.join("base.toml"),
            "[client.DungeonborneRuClient.stealth]
distance = 1000
curve = 1
[client.Other.x]
a = true
",
        )
        .unwrap();
        std::fs::create_dir_all(d.join("s1")).unwrap();
        std::fs::write(d.join("s1/season.toml"), "id = \"s1\"
name = \"S\"
[client.DungeonborneRuClient.stealth]
distance = 700
").unwrap();
        let a = load(&d, Some("s1"));
        assert_eq!(a.client["DungeonborneRuClient"]["stealth"]["distance"], 700, "season on top of the base");
        assert_eq!(a.client["DungeonborneRuClient"]["stealth"]["curve"], 1, "base key stays");
        assert_eq!(a.client["Other"]["x"]["a"], true);
        assert_eq!(load(&d, None).client["DungeonborneRuClient"]["stealth"]["distance"], 1000, "no season — base");
        assert_eq!(load(&dir("client_empty"), None).client, serde_json::json!({}));
    }

    #[test]
    fn base_without_files_is_all_off() {
        let d = dir("empty");
        let a = load(&d, None);
        assert_eq!(a.id, None);
        assert_eq!(a.features, Features::default());
    }

    #[test]
    fn season_overrides_base_and_falls_back() {
        let d = dir("over");
        std::fs::write(d.join("base.toml"), "[features]\nauto_sell_extracted = true\nrelic_lower_quality = true\n").unwrap();
        std::fs::create_dir_all(d.join("s1")).unwrap();
        std::fs::write(
            d.join("s1/season.toml"),
            "id = \"s1\"\nname = \"S\"\nstate_key = \"s1\"\n[features]\nauto_sell_extracted = false\n\
             kill_reward = { currency = 1012, per_kill = 2 }\nstart_materials = [{ item = 1037, count = 5 }]\n[server]\nmaps = []\n",
        )
        .unwrap();
        let a = load(&d, Some("s1"));
        assert_eq!(a.id.as_deref(), Some("s1"));
        assert!(!a.features.auto_sell_extracted, "season overrides the base");
        assert!(a.features.relic_lower_quality, "not in the season — from the base");
        assert_eq!(a.features.kill_reward, Some(KillReward { currency: 1012, per_kill: 2 }));
        assert_eq!(a.features.start_materials, vec![Grant { item: 1037, count: 5 }]);
        // no season / wrong id / unknown module — base
        assert!(load(&d, Some("nope")).id.is_none());
        std::fs::write(d.join("s1/season.toml"), "id = \"s1\"\nname = \"S\"\nplugin = \"x\"\n").unwrap();
        let b = load(&d, Some("s1"));
        assert!(b.id.is_none() && b.features.auto_sell_extracted);
    }

    #[test]
    fn kill_reward_zero_is_off() {
        let d = dir("kr");
        std::fs::write(d.join("base.toml"), "[features]\nkill_reward = { currency = 1012, per_kill = 0 }\n").unwrap();
        assert_eq!(load(&d, None).features.kill_reward, None);
    }

    /// Seasons in the repository parse without errors
    #[test]
    fn repo_seasons_parse() {
        // seasons live in the workspace's content/ (docs/WORKSPACE_PLAN.md); the prod server reads ./seasons (SEASONS_DIR)
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/seasons");
        assert!(read_toml::<BaseToml>(&root.join("base.toml")).is_ok());
        let a = load(&root, Some("playtest"));
        assert_eq!(a.id.as_deref(), Some("playtest"));
        assert!(a.features.relic_lower_quality && !a.features.auto_sell_extracted && a.features.kill_reward.is_none());
        // 12.11: no starting relic resources (since 09.11 there were 5000 each)
        assert!(a.features.start_materials.is_empty());
        assert_eq!(a.features.start_gold, Some(10000)); // 08.11 evening: 10 000 gold
        assert_eq!(a.features.merchant_refresh_secs, 3600);
        assert!(!a.features.mixed_party_rooms); // 08.11: as in the original — group equals room size
        // base: gold and merchant by default
        let b = load(&root, None);
        assert_eq!((b.features.start_gold, b.features.merchant_refresh_secs, b.features.mixed_party_rooms), (None, 0, false));
    }
}
