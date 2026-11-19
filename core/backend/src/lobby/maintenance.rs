//! Maintenance mode: the file `maintenance.txt` next to the backend (env `MAINTENANCE_FILE`). File present — mode is on, players see its text.
//! The translation goes after the `[en]` line (src/text.rs): the launcher shows the text in its own language, the game shows the base one. A file, not an API: it is switched on over ssh without an admin session and survives a backend restart until removed.
//!
//! New matches are closed: votes are dropped, "Start" and private rooms refuse, while running matches play out and "Return to match"
//! works. Empty halls close immediately. Graceful restart: `deploy/vm/maintenance.sh`.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const DEFAULT_TEXT: &str = "Идут технические работы. Новые бои временно закрыты.";
/// The file is read at most once a second: `message` is called from the tick and on every launcher poll
const CHECK_EVERY: Duration = Duration::from_secs(1);

const DEFAULT_TEXT_EN: &str = "Maintenance. New battles are closed for now.";

fn parse(text: &str) -> crate::text::Text {
    let t = crate::text::Text::parse_sections(text);
    if t.is_empty() {
        let m = [(crate::text::BASE, DEFAULT_TEXT), ("en", DEFAULT_TEXT_EN)];
        crate::text::Text::ByLang(m.iter().map(|(l, s)| (l.to_string(), s.to_string())).collect())
    } else {
        t
    }
}

/// Text for players (base language), if the mode is on
pub fn message() -> Option<String> {
    text().map(|t| t.base().to_string())
}

/// Text with translations, if the mode is on
pub fn text() -> Option<crate::text::Text> {
    static CACHE: OnceLock<Mutex<Option<(Instant, Option<crate::text::Text>)>>> = OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock().unwrap();
    if let Some((at, m)) = c.as_ref() {
        if at.elapsed() < CHECK_EVERY {
            return m.clone();
        }
    }
    let path = std::env::var("MAINTENANCE_FILE").unwrap_or_else(|_| "maintenance.txt".to_string());
    let m = std::fs::read_to_string(path).ok().map(|t| parse(&t));
    *c = Some((Instant::now(), m.clone()));
    m
}

pub fn is_on() -> bool {
    message().is_some()
}

/// Our own people (Discord role `gm` or `admin`) play during maintenance too (user decision 07.11: "prod without maintenance
/// for us"). Roles are recorded on launcher requests and on game login (`note_roles`): mode checks are synchronous.
fn staff() -> &'static Mutex<std::collections::HashSet<String>> {
    static S: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

pub fn note_roles(open_id: &str, roles: &[String], banned: bool) {
    let is = !banned && roles.iter().any(|r| r == "gm" || r == "admin");
    let mut s = staff().lock().unwrap();
    if is {
        s.insert(open_id.to_string());
    } else {
        s.remove(open_id);
    }
}

pub fn is_staff(open_id: &str) -> bool {
    staff().lock().unwrap().contains(open_id)
}

/// Maintenance closes new matches to this player
pub fn blocks(open_id: &str) -> bool {
    is_on() && !is_staff(open_id)
}

/// Maintenance text for this player (none for our own people)
pub fn message_for(open_id: &str) -> Option<String> {
    if is_staff(open_id) { None } else { message() }
}

/// `maintenance_i18n` of the lobby snapshot: translations of the maintenance text for this player, `null` — no mode or no translations
pub fn i18n_for(open_id: &str) -> serde_json::Value {
    match text() {
        Some(t) if !is_staff(open_id) => t.i18n_json(),
        _ => serde_json::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn staff_roles() {
        super::note_roles("st_a", &["player".into(), "gm".into()], false);
        super::note_roles("st_b", &["player".into()], false);
        super::note_roles("st_c", &["admin".into()], true);
        assert!(super::is_staff("st_a") && !super::is_staff("st_b") && !super::is_staff("st_c"));
        super::note_roles("st_a", &["player".into()], false); // role removed
        assert!(!super::is_staff("st_a"));
    }

    #[test]
    fn empty_file_gets_default_text() {
        assert_eq!(super::parse("  \n").base(), super::DEFAULT_TEXT);
        assert_eq!(super::parse("  \n").get(Some("en")), super::DEFAULT_TEXT_EN);
        assert_eq!(super::parse("Обновление, 10 минут\n").base(), "Обновление, 10 минут");
        assert_eq!(super::parse("Обновление\n[en]\nUpdate\n").get(Some("en")), "Update");
    }
}
