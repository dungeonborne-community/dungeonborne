//! Channels and roles from the config → IDs. A number in the config is treated as an ID, otherwise the name is looked up on the server.
//! Discord stores text channel names lowercase with hyphens instead of spaces, so we search both ways.

use poise::serenity_prelude as serenity;
use serenity::{ChannelId, GuildId, RoleId, UserId};
use std::collections::HashMap;

pub struct Names {
    channels: HashMap<String, ChannelId>,
    roles: HashMap<String, RoleId>,
}

fn norm(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

/// Name → channel. Duplicate names happen (channels of an event category: 💬・chat, ➕・Trio) —
/// the channel from the category higher in the list wins, i.e. the main one, not the event one.
fn by_name(chans: impl Iterator<Item = (ChannelId, String, Option<u16>, u16)>) -> HashMap<String, ChannelId> {
    let mut list: Vec<_> = chans.collect();
    list.sort_by_key(|(id, _, cat, pos)| (cat.map_or(0, |c| c as u32 + 1), *pos, id.get()));
    let mut out = HashMap::new();
    for (id, name, _, _) in list {
        out.entry(norm(&name)).or_insert(id);
        out.entry(name).or_insert(id);
    }
    out
}

impl Names {
    pub async fn load(http: &serenity::Http, guild: GuildId) -> Result<Self, serenity::Error> {
        let all: Vec<_> = guild.channels(http).await?.into_values().map(|c| (c.id, c.name, c.parent_id, c.position)).collect();
        let cat_pos: HashMap<_, _> = all.iter().map(|(id, _, _, pos)| (*id, *pos)).collect();
        let channels = by_name(all.into_iter().map(|(id, name, parent, pos)| {
            (id, name, parent.and_then(|p| cat_pos.get(&p).copied()), pos)
        }));
        let roles = guild.roles(http).await?.into_iter().map(|(id, r)| (r.name, id)).collect();
        Ok(Self { channels, roles })
    }

    pub fn channel(&self, key: &str) -> Option<ChannelId> {
        if let Ok(id) = key.parse::<u64>() {
            return Some(ChannelId::new(id));
        }
        self.channels.get(key).or_else(|| self.channels.get(&norm(key))).copied()
    }

    pub fn role(&self, key: &str) -> Option<RoleId> {
        if let Ok(id) = key.parse::<u64>() {
            return Some(RoleId::new(id));
        }
        self.roles.get(key).copied()
    }

    /// Module channel; if missing, a warning goes to the log and the module skips it
    pub fn need_channel(&self, key: &str, module: &str) -> Option<ChannelId> {
        let id = self.channel(key);
        if id.is_none() {
            eprintln!("[{}] channel '{}' not found on the server", module, key);
        }
        id
    }

    pub fn need_role(&self, key: &str, module: &str) -> Option<RoleId> {
        let id = self.role(key);
        if id.is_none() {
            eprintln!("[{}] role '{}' not found on the server", module, key);
        }
        id
    }

    /// Text substitutions: {user} = mention, {server} = server name,
    /// {#channel} = clickable channel link, {@role} = role mention.
    pub fn render(&self, text: &str, user: Option<UserId>, server: &str) -> String {
        let mut text = text.replace("{server}", server);
        if let Some(u) = user {
            text = text.replace("{user}", &format!("<@{}>", u));
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(start) = rest.find('{') {
            out.push_str(&rest[..start]);
            let tail = &rest[start..];
            let (Some(end), Some(kind)) = (tail.find('}'), tail[1..].chars().next()) else {
                out.push_str(tail);
                rest = "";
                break;
            };
            if end < 1 + kind.len_utf8() {
                out.push('{');
                rest = &tail[1..];
                continue;
            }
            let key = &tail[1 + kind.len_utf8()..end];
            let resolved = match kind {
                '#' => self.channel(key).map(|id| format!("<#{}>", id)),
                '@' => self.role(key).map(|id| format!("<@&{}>", id)),
                _ => None,
            };
            out.push_str(&resolved.unwrap_or_else(|| tail[..=end].to_string()));
            rest = &tail[end + 1..];
        }
        out.push_str(rest);
        out
    }
}

#[cfg(test)]
impl Names {
    pub fn empty() -> Self {
        Self { channels: HashMap::new(), roles: HashMap::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_placeholders() {
        let names = Names {
            channels: HashMap::from([("💬・chat-ru".to_string(), ChannelId::new(10))]),
            roles: HashMap::from([("Events".to_string(), RoleId::new(20))]),
        };
        let out = names.render("Привет, {user}! {#💬・chat-ru} {@Events} {#нет} {} {server} {x", Some(UserId::new(5)), "TestServer");
        assert_eq!(out, "Привет, <@5>! <#10> <@&20> {#нет} {} TestServer {x");
    }

    #[test]
    fn same_name_prefers_upper_category() {
        let ch = |id, name: &str, cat, pos| (ChannelId::new(id), name.to_string(), cat, pos);
        // the event category (position 11) came first, but the main one (position 2) still wins
        let map = by_name([ch(7, "💬・chat", Some(11), 48), ch(3, "💬・chat", Some(2), 8), ch(1, "📌 START", None, 0)].into_iter());
        assert_eq!(map["💬・chat"], ChannelId::new(3));
        assert_eq!(map["📌-start"], ChannelId::new(1));
    }
}
