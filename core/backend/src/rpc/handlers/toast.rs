//! Server toast — the game's system notice at the top of the screen with any text (owner's item 2, 18.11).
//!
//! The game itself shows this notice only for an error code in a response (`PromptProtocolError` → text
//! `ProtcolError_<code>` from the client's strings). For any text at any moment the server sends a system chat message
//! (`DChatMessagemSysNotify`, the player's system channel) whose text starts with the invisible marker U+200B; the client
//! mod `DungeonborneToast` sees the new chat message (`TGFChatReceiveMessageEvent`) and shows the text with the same notice
//! (`UToastWindow.ShowToast`). Without the mod the player sees the same text in the system chat — nothing is lost.
//!
//! Use: `self.toast(text)` in a handler — together with the reply to the player's action (own texts on own rules);
//! `toast_to` / `toast_online` — from the admin API (`POST /admin/v1/toast`).

use super::GatewaySession;
use crate::db::GameDb;
use crate::structures::PlayerModel;
use crate::tgf;

/// Marker of a toast message: zero-width space at the start (invisible in the chat)
pub const TOAST_MARK: &str = "\u{200B}";

/// The dirty push with a toast for the player `user_id`
fn toast_dirty(user_id: u64, text: &str) -> tgf::ScPlayerDirtyChgSync {
    super::social_handlers::sys_notify_dirty(user_id, &format!("{}{}", TOAST_MARK, text.trim()))
}

impl GatewaySession {
    /// Toast to the player of this session (goes out after the reply)
    #[allow(dead_code)] // for own texts on actions: call where a rule refuses an action
    pub(super) async fn toast(&self, text: &str) {
        let me = self.load_me().await;
        self.push_dirty(toast_dirty(me.user_id, text));
    }
}

/// Toast to one player by open_id: Ok(true) — delivered online, Ok(false) — offline (the text stays in their system chat
/// for a day), Err — no such player
pub async fn toast_to(db: &GameDb, open_id: &str, text: &str) -> Result<bool, String> {
    let p = db.get::<PlayerModel>("players", open_id).await.ok_or_else(|| format!("no player {}", open_id))?;
    let msg = toast_dirty(p.user_id, text);
    let online = crate::lobby::hub::push_to(open_id, crate::lobby::hub::DIRTY_METHOD, crate::lobby::hub::DIRTY_TYPE, &msg);
    println!("[Toast] {} ({}): {}", open_id, if online { "online" } else { "offline" }, text);
    Ok(online)
}

/// Toast to everyone online; returns how many got it
pub async fn toast_online(db: &GameDb, text: &str) -> usize {
    let mut n = 0;
    for open_id in crate::lobby::hub::online_ids() {
        if toast_to(db, &open_id, text).await.unwrap_or(false) {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The toast is a system chat message whose texts (all languages) start with the marker; the marker is not doubled,
    /// the text is trimmed
    #[test]
    fn toast_is_marked_system_message() {
        let d = toast_dirty(4242, "  Склад заполнен  ");
        let ch = &d.dirty_chat_message[0];
        assert_eq!(ch.channel_id, "sys_4242");
        let m = &ch.messages[0];
        let Some(tgf::d_chat_message_detail::Command::SysNotify(n)) = m.detail.as_ref().and_then(|x| x.command.as_ref()) else {
            panic!("not a system message");
        };
        assert_eq!(n.text, "\u{200B}Склад заполнен");
        assert_eq!(n.texts.len(), 10, "all client languages");
        assert!(n.texts.iter().all(|t| t.content == n.text));
    }

    /// An unknown player — an error; a known one offline — delivered = false
    #[tokio::test]
    async fn toast_to_offline_and_unknown() {
        let dir = format!("target/test_toast_db_{}", crate::auth::sessions::random_hex(4));
        let db = GameDb::init(&dir).await;
        assert!(toast_to(&db, "nobody", "x").await.is_err());
        db.save("players", "toast-off", &PlayerModel::new("toast-off", 777)).await;
        assert_eq!(toast_to(&db, "toast-off", "Привет").await, Ok(false));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
