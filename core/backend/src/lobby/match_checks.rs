//! Server-side match search checks: the [server] section of match_checks.toml (MATCH_CHECKS_FILE).
//! No file or no key — the check is on. Client-side checks from the same file are built by `cargo xtask balance build`.

use std::sync::OnceLock;

#[derive(Clone, Debug, serde::Deserialize, PartialEq)]
#[serde(default)]
pub struct Server {
    pub battle_access: bool,
    pub lobby_party_exact: bool,
    pub lobby_same_tab: bool,
    /// [gear_rarity] section (client-side check, built into competition_level): whether the rarity limit is on — for
    /// the rules hint in the launcher (08.11)
    #[serde(skip)]
    pub gear_rarity: bool,
    /// 08.11 evening: per-level rarity limits (`[gear_rarity] levels`, level_id → list of EN_ITEM_RARITY_*) —
    /// for the launcher hint; they apply with `enabled = false` too, as in the table build
    #[serde(skip)]
    pub gear_levels: std::collections::BTreeMap<String, Vec<String>>,
}

impl Default for Server {
    fn default() -> Self {
        Self { battle_access: true, lobby_party_exact: true, lobby_same_tab: true, gear_rarity: true, gear_levels: Default::default() }
    }
}

#[derive(serde::Deserialize)]
#[serde(default)]
struct Rarity {
    enabled: bool,
    levels: std::collections::BTreeMap<String, Vec<String>>,
}

impl Default for Rarity {
    fn default() -> Self {
        Self { enabled: true, levels: Default::default() }
    }
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct File {
    server: Server,
    gear_rarity: Rarity,
}

fn parse(text: &str) -> Result<Server, String> {
    toml::from_str::<File>(text)
        .map(|f| Server { gear_rarity: f.gear_rarity.enabled, gear_levels: f.gear_rarity.levels, ..f.server })
        .map_err(|e| e.to_string())
}

pub fn server() -> &'static Server {
    static S: OnceLock<Server> = OnceLock::new();
    S.get_or_init(|| {
        let path = std::env::var("MATCH_CHECKS_FILE").unwrap_or_else(|_| "match_checks.toml".into());
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Server::default();
        };
        match parse(&text) {
            Ok(s) => {
                println!("[MatchChecks] {}: {:?}", path, s);
                s
            }
            Err(e) => {
                println!("[MatchChecks] {}: error, all checks enabled: {}", path, e);
                Server::default()
            }
        }
    })
}

/// Epic and above — the original Hard limit (season 1 competition_level rows for 40003019/20)
const EPIC_UP: [&str; 4] = ["EN_ITEM_RARITY_EPIC", "EN_ITEM_RARITY_EXCEPTIONAL", "EN_ITEM_RARITY_LEGENDARY", "EN_ITEM_RARITY_UNIQUE"];

/// 08.11 evening: allowed gear rarities for a level (EnItemRarity) for the server check on "Start" in the launcher
/// lobby — the client checks against the map it selected itself, not the vote result. Same source as the launcher
/// hint (`launcher_lobby::map_rules`): the level's own list from `[gear_rarity] levels`, otherwise Hard (skirmish) — epic and
/// above when `enabled`. None — no limit.
pub fn gear_rule(level_id: i64) -> Option<Vec<i32>> {
    let s = server();
    rule_with(s.gear_rarity, &s.gear_levels, crate::lobby::matchmaking::skirmish(level_id), level_id)
}

fn rule_with(on: bool, levels: &std::collections::BTreeMap<String, Vec<String>>, skirmish: bool, level_id: i64) -> Option<Vec<i32>> {
    let names: Vec<String> = match levels.get(&level_id.to_string()) {
        Some(list) => list.clone(),
        None if on && skirmish => EPIC_UP.iter().map(|s| s.to_string()).collect(),
        None => return None,
    };
    let ids: Vec<i32> = names.iter().filter_map(|n| crate::tgf::EnItemRarity::from_str_name(n)).map(|r| r as i32).collect();
    (!ids.is_empty()).then_some(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gear_rule_by_level() {
        use crate::tgf::EnItemRarity as R;
        let green = std::collections::BTreeMap::from([(
            "40003016".to_string(),
            ["EN_ITEM_RARITY_NONE", "EN_ITEM_RARITY_ROTTED", "EN_ITEM_RARITY_NORMAL", "EN_ITEM_RARITY_UNCOMMON"].map(String::from).to_vec(),
        )]);
        let r = rule_with(true, &green, false, 40003016).unwrap();
        assert!(r.contains(&(R::Uncommon as i32)) && !r.contains(&(R::Enhanced as i32)));
        assert_eq!(rule_with(true, &green, true, 40003019), Some(vec![R::Epic as i32, R::Exceptional as i32, R::Legendary as i32, R::Unique as i32]));
        assert_eq!(rule_with(false, &green, true, 40003019), None, "Hard without enabled — no limit");
        assert!(rule_with(false, &green, false, 40003016).is_some(), "level's own list — with enabled = false too");
        assert_eq!(rule_with(true, &green, false, 40003015), None);
    }

    #[test]
    fn missing_keys_keep_checks_on() {
        assert_eq!(parse("[gear_rarity]\nenabled = false").unwrap(), Server { gear_rarity: false, ..Server::default() });
        let s = parse("[server]\nlobby_party_exact = false").unwrap();
        assert!(!s.lobby_party_exact);
        assert!(s.battle_access && s.lobby_same_tab);
        let s = parse("[gear_rarity]\nlevels = { 40003016 = [\"EN_ITEM_RARITY_NORMAL\", \"EN_ITEM_RARITY_UNCOMMON\"] }").unwrap();
        assert_eq!(s.gear_levels["40003016"], vec!["EN_ITEM_RARITY_NORMAL", "EN_ITEM_RARITY_UNCOMMON"]);
        assert!(s.gear_rarity);
    }
}
