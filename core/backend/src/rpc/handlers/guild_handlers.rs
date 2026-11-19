//! Guilds (guild_*). The handlers in handlers.rs only delegate here (`handle_<rpc>` → `<rpc>_impl`).
//!
//! Format facts: guild_* is in neither rpc_dump (PVE/OUR) nor the official server sniffer; no guild UI assets
//! were found in the client. Sources — only proto (`DGuild`, `DUserGuildData`, `DGuildEventLog`, `EnGuildRoleType`,
//! `EnGuildSubscribeResultType`, codes `EN_ERR_GUILD_*` -3401..-3425) and the `guild_*` keys in const.json (limits, timings,
//! impeachment thresholds; the client reads the same ones via ExcelConstConfig). There are no guild_create_cost_item /
//! guild_rename_cost_item keys in const.json → creating and renaming are free.
//!
//! All guild_* responses are empty and without ret_code: a rejection — EnErrorCode in RpcHead.error_code (`set_rpc_error`) + log.
//! There is no Lua guild client: no guild scripts/assets were found in pakchunk0 (Content/Script) or other paks,
//! guild_* exists only in the protocol tables (Table/Proto2NetworkEnum/Options.lua, a copy in tools/research/guild/lua_orig).
//! So the feature was never finished in the client, and there is nowhere to verify the field semantics (CreateGuild/DestroyGuild, confirmed_event_id, name limit,
//! pay_off).
//!
//! Storage:
//! - DB table `guild_data`, id = guild_id → GuildRecord (the whole DGuild as prost bytes, like RoleModel.data).
//!   Membership, applications and invitations are stored only in the guild records (source of truth).
//! - the player's `state["guild"]`: { guild_id (cache), next_join_unlock_timepoint, impeachment: { next_unlock_timepoint,
//!   last_vote_event_id, last_vote_agree } } — written only by the player's own session.
//!
//! Pushes — ScPlayerDirtyChgSync: to guild members (and to the kicked — the event of their kick) — `dirty_guild_event`
//! (new DGuildEventLog) + `dirty_user_guild` (their DUserGuildData); to applicants/invitees — only `dirty_user_guild`.
//! All operations run under a shared mutex: read all guilds → process deadlines → change → save → pushes.

use super::common;
use super::GatewaySession;
use crate::lobby::hub;
use crate::structures::PlayerModel;
use crate::tgf;
use crate::tgf::d_guild_event_log::Event;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

type GErr = tgf::EnErrorCode;
type GResult<T = ()> = Result<T, GErr>;

const TABLE: &str = "guild_data";
const FIRST_GUILD_ID: u64 = 100001;

const ROLE_MEMBER: i32 = tgf::EnGuildRoleType::Member as i32;
const ROLE_OWNER: i32 = tgf::EnGuildRoleType::Owner as i32;
const ROLE_ADMIN: i32 = tgf::EnGuildRoleType::Admin as i32;

const SUB_MEMBER: i32 = tgf::EnGuildSubscribeResultType::EnGuildSrtMember as i32;
const SUB_APPLY: i32 = tgf::EnGuildSubscribeResultType::EnGuildSrtApply as i32;
const SUB_INVITATION: i32 = tgf::EnGuildSubscribeResultType::EnGuildSrtInvitation as i32;

fn guild_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn ukey(user_id: u64) -> tgf::DPlayerIdKey {
    tgf::DPlayerIdKey { user_id, zone_id: 1 }
}

fn uid_of(k: &Option<tgf::DPlayerIdKey>) -> u64 {
    k.as_ref().map(|k| k.user_id).unwrap_or(0)
}

/// Player level for the guild limit: the highest level among their characters
fn guild_user_level(p: &PlayerModel) -> i32 {
    p.roles
        .iter()
        .map(|r| {
            let deploy = r.data.deploy_role.as_ref().map(|d| d.role_level).unwrap_or(0);
            deploy.max(r.role_level as i32)
        })
        .max()
        .unwrap_or(1)
        .max(1)
}

fn next_join_unlock(p: &PlayerModel) -> i64 {
    p.state["guild"]["next_join_unlock_timepoint"].as_i64().unwrap_or(0)
}

/// "30d" / "3h" / "2m" / "10s" / "60" → seconds
fn parse_duration(s: &str) -> Option<i64> {
    let s = s.trim();
    let (num, mul) = match s.chars().last()? {
        'd' => (&s[..s.len() - 1], 86_400),
        'h' => (&s[..s.len() - 1], 3_600),
        'm' => (&s[..s.len() - 1], 60),
        's' => (&s[..s.len() - 1], 1),
        _ => (s, 1),
    };
    num.trim().parse::<i64>().ok().map(|n| n * mul)
}

/// DGuild is stored as prost bytes (oneof events and nested messages do not break SurrealDB this way)
mod guild_pb {
    use prost::Message;
    use serde::{de::Visitor, Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<T: Message, S: Serializer>(val: &T, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&val.encode_to_vec())
    }

    pub fn deserialize<'de, T: Message + Default, D: Deserializer<'de>>(deserializer: D) -> Result<T, D::Error> {
        struct BytesVisitor;
        impl<'de> Visitor<'de> for BytesVisitor {
            type Value = Vec<u8>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("byte array or sequence of bytes")
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                Ok(v.to_vec())
            }
            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
                Ok(v)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut vec = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(b) = seq.next_element()? {
                    vec.push(b);
                }
                Ok(vec)
            }
        }
        let bytes = deserializer.deserialize_any(BytesVisitor)?;
        T::decode(bytes.as_slice()).map_err(serde::de::Error::custom)
    }
}

/// Limits and timings from const.json (`guild_*`); defaults = the current values from the table
struct GuildCfg {
    max_members: i32,
    apply_expire: i64,
    apply_max: usize,
    apply_max_per_user: usize,
    invitation_expire: i64,
    invitation_max: usize,
    invitation_max_per_user: usize,
    log_max: usize,
    log_show: usize,
    set_owner_lock: i64,
    exit_lock: i64,
    batch_max: usize,
    name_len: usize,
    announcement_len: usize,
    imp_offline: i64,
    imp_duration: i64,
    imp_interval: i64,
    imp_show: i64,
    /// share of voters (‰ of the members, excluding the leader) without which an impeachment does not pass
    imp_vote_pm: i64,
    /// share of "for" among the voters (‰) that must be exceeded
    imp_agree_pm: i64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
struct GuildRecord {
    guild_id: u64,
    /// copy of basic_profile.name (for search)
    name: String,
    destroyed: bool,
    /// the last issued event_id of the log
    next_event_id: i64,
    /// (user_id, join time) — DGuildMember has no join date
    joined: Vec<(u64, i64)>,
    #[serde(with = "guild_pb")]
    guild: tgf::DGuild,
}

impl GuildRecord {
    fn profile_name(&self) -> String {
        self.guild.basic_profile.as_ref().map(|p| p.name.clone()).unwrap_or_default()
    }

    fn member(&self, uid: u64) -> Option<&tgf::DGuildMember> {
        self.guild.members.iter().find(|m| uid_of(&m.user_key) == uid)
    }

    fn role_of(&self, uid: u64) -> i32 {
        self.member(uid).map(|m| m.role_type).unwrap_or(0)
    }

    fn is_member(&self, uid: u64) -> bool {
        self.member(uid).is_some()
    }

    fn member_uids(&self) -> Vec<u64> {
        self.guild.members.iter().map(|m| uid_of(&m.user_key)).collect()
    }

    fn owner_uid(&self) -> Option<u64> {
        self.guild.members.iter().find(|m| m.role_type == ROLE_OWNER).map(|m| uid_of(&m.user_key))
    }

    fn is_full(&self) -> bool {
        let max = self.guild.configure.as_ref().map(|c| c.max_member_number).unwrap_or(0);
        max > 0 && self.guild.members.len() as i32 >= max
    }

    fn level_limit(&self) -> i32 {
        self.guild.configure.as_ref().map(|c| c.player_level_limit).unwrap_or(0)
    }

    fn joined_ts(&self, uid: u64) -> i64 {
        self.joined.iter().find(|(u, _)| *u == uid).map(|(_, t)| *t).unwrap_or(0)
    }

    fn apply_of(&self, uid: u64) -> Option<&tgf::DGuildMemberApply> {
        self.guild.apply.iter().find(|a| uid_of(&a.user_key) == uid)
    }

    fn invitation_of(&self, uid: u64) -> Option<&tgf::DGuildMemberInvitation> {
        self.guild.invitation.iter().find(|i| uid_of(&i.user_key) == uid)
    }

    /// Whether a vote is in progress (a finished one is still shown until show_expire_timepoint)
    fn impeachment_running(&self) -> bool {
        self.guild.impeachment.as_ref().map(|i| i.show_expire_timepoint == 0).unwrap_or(false)
    }

    fn stats(&self) -> tgf::DGuildBatchGetStats {
        tgf::DGuildBatchGetStats {
            owner_user_keys: self.owner_uid().map(ukey).into_iter().collect(),
            apply_count: self.guild.apply.len() as i32,
            member_count: self.guild.members.len() as i32,
            invitation_count: self.guild.invitation.len() as i32,
        }
    }

    /// For outsiders (batch_get): profile, settings, details and counters
    fn public_view(&self) -> tgf::DGuild {
        tgf::DGuild {
            basic_profile: self.guild.basic_profile.clone(),
            configure: self.guild.configure.clone(),
            detail_data: self.guild.detail_data.clone(),
            stats: Some(tgf::DGuildStats {}),
            batch_get_stats: Some(self.stats()),
            ..Default::default()
        }
    }

    /// For a member (get_all): everything, the log — the last log_show records
    fn full_view(&self, log_show: usize) -> tgf::DGuild {
        let mut g = self.guild.clone();
        let skip = g.event_logs.len().saturating_sub(log_show);
        g.event_logs.drain(..skip);
        g.stats = Some(tgf::DGuildStats {});
        g.batch_get_stats = Some(self.stats());
        g
    }
}

/// The player's DUserGuildData: membership/applications/invitations — from the guild records, timers — from their state["guild"]
fn guild_user_data(all: &[GuildRecord], p: &PlayerModel) -> tgf::DUserGuildData {
    let uid = p.user_id;
    let st = &p.state["guild"];
    let mut subscribe_list = Vec::new();
    let mut confirmed_event_id = 0;
    for g in all.iter().filter(|g| !g.destroyed) {
        if let Some(m) = g.member(uid) {
            // TODO(verify): confirmed_event_id — we assume the whole log was delivered by pushes
            confirmed_event_id = g.next_event_id;
            subscribe_list.push(tgf::DUserGuildSubscribe {
                subscribe_type: SUB_MEMBER,
                guild_id: g.guild_id,
                create_timepoint: g.joined_ts(uid),
                role_type_cache: m.role_type,
            });
            continue;
        }
        let apply = g.apply_of(uid).map(|a| a.create_timepoint);
        let inv = g.invitation_of(uid).map(|i| i.create_timepoint);
        let t = if apply.is_some() { SUB_APPLY } else { 0 } | if inv.is_some() { SUB_INVITATION } else { 0 };
        if t != 0 {
            subscribe_list.push(tgf::DUserGuildSubscribe {
                subscribe_type: t,
                guild_id: g.guild_id,
                create_timepoint: apply.into_iter().chain(inv).min().unwrap_or(0),
                role_type_cache: 0,
            });
        }
    }
    tgf::DUserGuildData {
        confirmed_event_id,
        subscribe_list,
        impeachment_data: Some(tgf::DUserGuildImpeachmentData {
            next_unlock_timepoint: st["impeachment"]["next_unlock_timepoint"].as_i64().unwrap_or(0),
            last_vote_event_id: st["impeachment"]["last_vote_event_id"].as_i64().unwrap_or(0),
            last_vote_agree: st["impeachment"]["last_vote_agree"].as_bool().unwrap_or(false),
        }),
        next_join_unlock_timepoint: st["next_join_unlock_timepoint"].as_i64().unwrap_or(0),
    }
}

/// State of one operation: all guilds in memory + what to save and whom to send what
struct GuildCtx {
    me: PlayerModel,
    me_dirty: bool,
    all: Vec<GuildRecord>,
    now: i64,
    cfg: GuildCfg,
    /// guild_ids to save
    changed: BTreeSet<u64>,
    /// guild_id → new log events
    events: BTreeMap<u64, Vec<tgf::DGuildEventLog>>,
    /// guild_id → who, besides the current members, receives the events (kicked / disbanded)
    event_extra: BTreeMap<u64, BTreeSet<u64>>,
    /// whom to send dirty_user_guild (even without events)
    notify: BTreeSet<u64>,
}

impl GuildCtx {
    fn me_id(&self) -> u64 {
        self.me.user_id
    }

    fn find_idx(&self, gid: u64) -> Option<usize> {
        self.all.iter().position(|g| g.guild_id == gid)
    }

    fn alive_idx(&self, gid: u64) -> GResult<usize> {
        if gid == 0 {
            return Err(GErr::EnErrGuildInvalidId);
        }
        let gi = self.find_idx(gid).ok_or(GErr::EnErrGuildNotFound)?;
        if self.all[gi].destroyed {
            return Err(GErr::EnErrGuildDestroyed);
        }
        Ok(gi)
    }

    fn guild_of(&self, uid: u64) -> Option<usize> {
        self.all.iter().position(|g| !g.destroyed && g.is_member(uid))
    }

    fn my_guild(&self) -> GResult<usize> {
        self.guild_of(self.me_id()).ok_or(GErr::EnErrGuildNotFound)
    }

    /// My guild_id from the request (0 = "mine") must match my guild
    fn my_guild_checked(&self, req_gid: u64) -> GResult<usize> {
        let gi = self.my_guild()?;
        if req_gid != 0 && req_gid != self.all[gi].guild_id {
            return Err(GErr::EnErrGuildInvalidId);
        }
        Ok(gi)
    }

    fn require_role(&self, gi: usize, roles: &[i32]) -> GResult {
        if roles.contains(&self.all[gi].role_of(self.me_id())) {
            Ok(())
        } else {
            Err(GErr::EnErrGuildAccessDeny)
        }
    }

    fn name_taken(&self, name: &str, except: u64) -> bool {
        let n = name.to_lowercase();
        self.all.iter().any(|g| !g.destroyed && g.guild_id != except && g.profile_name().to_lowercase() == n)
    }

    fn check_name(&self, name: &str, except: u64) -> GResult {
        if name.is_empty() {
            return Err(GErr::EnErrInvalidParam);
        }
        // TODO(verify): guild_name_length_limit = 6 — in characters or in "weights" (CJK = 2)?
        if name.chars().count() > self.cfg.name_len {
            return Err(GErr::EnErrGuildNameTooLong);
        }
        if self.name_taken(name, except) {
            return Err(GErr::EnErrGuildNameAlreadyToken);
        }
        Ok(())
    }

    fn touch(&mut self, gi: usize) {
        self.changed.insert(self.all[gi].guild_id);
    }

    /// Add an event to the guild log (trimmed to guild_log_max_count) and to the broadcast
    fn event(&mut self, gi: usize, operator: u64, event: Event) {
        let (now, log_max) = (self.now, self.cfg.log_max);
        let g = &mut self.all[gi];
        g.next_event_id += 1;
        let d = g.guild.detail_data.clone().unwrap_or_default();
        let log = tgf::DGuildEventLog {
            event_id: g.next_event_id,
            operator_user_key: (operator != 0).then(|| ukey(operator)),
            sync_data: Some(tgf::DGuildEventSync { activity_score: d.activity_score, combat: d.combat }),
            create_timepoint: now,
            event: Some(event),
        };
        g.guild.event_logs.push(log.clone());
        let over = g.guild.event_logs.len().saturating_sub(log_max);
        g.guild.event_logs.drain(..over);
        let gid = g.guild_id;
        self.changed.insert(gid);
        self.events.entry(gid).or_default().push(log);
    }

    fn member_event_data(m: &tgf::DGuildMember, require_apply: bool, require_invitation: bool) -> tgf::DGuildMemberEventData {
        tgf::DGuildMemberEventData {
            role_type: m.role_type,
            user_key: m.user_key.clone(),
            activity_score: m.activity_score,
            combat: m.combat,
            user_level: m.user_level,
            require_apply,
            require_invitation,
        }
    }

    /// Joining: removes all of the player's applications/invitations in all guilds, adds the member, AddMember event
    fn join(&mut self, gi: usize, uid: u64, role: i32, user_level: i32, operator: u64, via_apply: bool, via_invitation: bool) {
        for j in 0..self.all.len() {
            let g = &mut self.all[j];
            let before = (g.guild.apply.len(), g.guild.invitation.len());
            g.guild.apply.retain(|a| uid_of(&a.user_key) != uid);
            g.guild.invitation.retain(|i| uid_of(&i.user_key) != uid);
            if before != (g.guild.apply.len(), g.guild.invitation.len()) {
                self.changed.insert(g.guild_id);
            }
        }
        let now = self.now;
        let g = &mut self.all[gi];
        let member = tgf::DGuildMember {
            role_type: role,
            user_key: Some(ukey(uid)),
            user_level,
            operation_event_id: g.next_event_id + 1,
            last_subscribe_timepoint: now,
            ..Default::default()
        };
        let data = Self::member_event_data(&member, via_apply, via_invitation);
        g.guild.members.push(member);
        g.joined.retain(|(u, _)| *u != uid);
        g.joined.push((uid, now));
        self.event(gi, operator, Event::AddMember(data));
        self.notify.insert(uid);
    }

    fn remove_member(&mut self, gi: usize, uid: u64, operator: u64) {
        let g = &mut self.all[gi];
        let Some(pos) = g.guild.members.iter().position(|m| uid_of(&m.user_key) == uid) else { return };
        let m = g.guild.members.remove(pos);
        g.joined.retain(|(u, _)| *u != uid);
        let gid = g.guild_id;
        self.event(gi, operator, Event::RemoveMember(Self::member_event_data(&m, false, false)));
        self.event_extra.entry(gid).or_default().insert(uid);
        self.notify.insert(uid);
    }

    /// Disbanding: DestroyGuild event to all members, the guild is marked destroyed (the name is freed)
    fn destroy(&mut self, gi: usize, operator: u64) {
        let gid = self.all[gi].guild_id;
        let g = &self.all[gi];
        let mut affected: BTreeSet<u64> = g.member_uids().into_iter().collect();
        let members = affected.clone();
        affected.extend(g.guild.apply.iter().map(|a| uid_of(&a.user_key)));
        affected.extend(g.guild.invitation.iter().map(|i| uid_of(&i.user_key)));
        // TODO(verify): what DestroyGuild(u64) carries — guild_id or time; CreateGuild(i64) — we send the creation time
        self.event(gi, operator, Event::DestroyGuild(gid));
        let now = self.now;
        let g = &mut self.all[gi];
        g.destroyed = true;
        g.guild.detail_data.get_or_insert_with(Default::default).destroy_timepoint = now;
        g.guild.members.clear();
        g.guild.apply.clear();
        g.guild.invitation.clear();
        g.guild.impeachment = None;
        g.joined.clear();
        self.event_extra.entry(gid).or_default().extend(members);
        self.notify.extend(affected);
    }

    fn set_role(&mut self, gi: usize, uid: u64, role: i32, operator: u64) {
        let g = &mut self.all[gi];
        let Some(m) = g.guild.members.iter_mut().find(|m| uid_of(&m.user_key) == uid) else { return };
        let origin = m.role_type;
        m.role_type = role;
        let unlock = g.guild.detail_data.as_ref().map(|d| d.next_transfer_owner_unlock_timepoint).unwrap_or(0);
        self.event(
            gi,
            operator,
            Event::SetRole(tgf::DGuildSetRoleEventData {
                role_type: role,
                user_key: Some(ukey(uid)),
                origin_role_type: origin,
                next_transfer_owner_unlock_timepoint: unlock,
            }),
        );
    }

    /// Impeachment result: turnout ≥ imp_vote_pm of the members (excluding the leader) and "for" > imp_agree_pm of the voters.
    /// Success — the initiator becomes the leader, the former leader — a member.
    fn finish_impeachment(&mut self, gi: usize) {
        let Some(mut imp) = self.all[gi].guild.impeachment.clone() else { return };
        let target = uid_of(&imp.target_user_key);
        let initiator = uid_of(&imp.initiator);
        let eligible = self.all[gi].guild.members.iter().filter(|m| uid_of(&m.user_key) != target).count() as i64;
        let agree = imp.agree_users.len() as i64;
        let voters = agree + imp.disagree_users.len() as i64;
        let approved = eligible > 0
            && voters * 1000 >= eligible * self.cfg.imp_vote_pm
            && agree * 1000 > voters * self.cfg.imp_agree_pm;
        imp.is_approved = approved;
        imp.finish_timepoint = imp.finish_timepoint.min(self.now);
        imp.show_expire_timepoint = self.now + self.cfg.imp_show;
        imp.finish_member_size = eligible;
        self.all[gi].guild.impeachment = Some(imp.clone());
        self.event(gi, initiator, Event::FinishImpeachment(imp));
        if approved && self.all[gi].is_member(initiator) && self.all[gi].role_of(target) == ROLE_OWNER {
            self.set_role(gi, target, ROLE_MEMBER, initiator);
            self.set_role(gi, initiator, ROLE_OWNER, initiator);
        }
    }

    /// Early result: everyone voted or "for" is already enough whatever the remaining votes
    fn impeachment_check_early(&mut self, gi: usize) {
        let Some(imp) = self.all[gi].guild.impeachment.as_ref() else { return };
        let target = uid_of(&imp.target_user_key);
        let eligible = self.all[gi].guild.members.iter().filter(|m| uid_of(&m.user_key) != target).count() as i64;
        let agree = imp.agree_users.len() as i64;
        let voters = agree + imp.disagree_users.len() as i64;
        let sure = agree * 1000 >= eligible * self.cfg.imp_vote_pm && agree * 1000 > eligible * self.cfg.imp_agree_pm;
        if voters >= eligible || sure {
            self.finish_impeachment(gi);
        }
    }

    /// Deadlines: expired applications/invitations, ending/hiding the impeachment
    fn housekeep(&mut self) {
        let now = self.now;
        for gi in 0..self.all.len() {
            if self.all[gi].destroyed {
                continue;
            }
            let g = &mut self.all[gi];
            let before = (g.guild.apply.len(), g.guild.invitation.len());
            g.guild.apply.retain(|a| a.expired_timepoint == 0 || a.expired_timepoint > now);
            g.guild.invitation.retain(|i| i.expired_timepoint == 0 || i.expired_timepoint > now);
            if before != (g.guild.apply.len(), g.guild.invitation.len()) {
                self.touch(gi);
            }
            let Some(imp) = self.all[gi].guild.impeachment.clone() else { continue };
            if imp.show_expire_timepoint == 0 && now >= imp.finish_timepoint {
                self.finish_impeachment(gi);
            } else if imp.show_expire_timepoint > 0 && now >= imp.show_expire_timepoint {
                self.all[gi].guild.impeachment = None;
                self.touch(gi);
            }
        }
    }
}

impl GatewaySession {
    fn guild_const(&self, key: &str) -> Option<&str> {
        self.game_data.r#const.iter().find(|c| c.key == key).map(|c| c.value.as_str())
    }

    fn guild_cfg(&self) -> GuildCfg {
        let int = |k: &str, d: i64| self.guild_const(k).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(d);
        let dur = |k: &str, d: i64| self.guild_const(k).and_then(parse_duration).unwrap_or(d);
        const DAY: i64 = 86_400;
        GuildCfg {
            max_members: int("guild_default_max_member_number", 50) as i32,
            apply_expire: dur("guild_apply_expire_timeout", 3 * DAY),
            apply_max: int("guild_apply_max_count", 999) as usize,
            apply_max_per_user: int("guild_apply_max_count_per_user", 5) as usize,
            invitation_expire: dur("guild_invitation_expire_timeout", 3 * DAY),
            invitation_max: int("guild_invitation_max_count", 20) as usize,
            invitation_max_per_user: int("guild_invitation_max_count_per_user", 5) as usize,
            log_max: int("guild_log_max_count", 300).max(1) as usize,
            log_show: int("guild_log_show_count", 30).max(1) as usize,
            set_owner_lock: dur("guild_set_owner_lock_time", DAY),
            exit_lock: dur("guild_exit_lock_time", DAY),
            batch_max: int("guild_batch_get_max_count", 30).max(1) as usize,
            name_len: int("guild_name_length_limit", 6).max(1) as usize,
            announcement_len: int("guild_announcement_length_limit", 60).max(1) as usize,
            imp_offline: dur("guild_impeachment_offline_time", 15 * DAY),
            imp_duration: dur("guild_impeachment_duration", 2 * DAY),
            imp_interval: dur("guild_impeachment_interval", 7 * DAY),
            imp_show: dur("guild_impeachment_show_time", 2 * DAY),
            imp_vote_pm: int("guild_impeachment_success_vote_pemillage", 600),
            imp_agree_pm: int("guild_impeachment_confirm_agree_pemillage", 500),
        }
    }

    async fn guild_load_all(&self) -> Vec<GuildRecord> {
        match self.db.db.query("SELECT * FROM guild_data").await {
            Ok(mut res) => res.take::<Vec<GuildRecord>>(0).unwrap_or_else(|e| {
                eprintln!("[Guild] read error {}: {}", TABLE, e);
                Vec::new()
            }),
            Err(e) => {
                eprintln!("[Guild] query error {}: {}", TABLE, e);
                Vec::new()
            }
        }
    }

    async fn guild_begin(&self) -> GuildCtx {
        let mut ctx = GuildCtx {
            me: self.load_me().await,
            me_dirty: false,
            all: self.guild_load_all().await,
            now: Self::now_ts(),
            cfg: self.guild_cfg(),
            changed: BTreeSet::new(),
            events: BTreeMap::new(),
            event_extra: BTreeMap::new(),
            notify: BTreeSet::new(),
        };
        ctx.housekeep();
        ctx
    }

    /// Save the changes and send events/DUserGuildData
    async fn guild_commit(&self, mut ctx: GuildCtx) {
        let my_gid = ctx.guild_of(ctx.me_id()).map(|i| ctx.all[i].guild_id).unwrap_or(0);
        if ctx.me.state["guild"]["guild_id"].as_u64().unwrap_or(0) != my_gid {
            if !ctx.me.state["guild"].is_object() {
                ctx.me.state["guild"] = serde_json::json!({});
            }
            ctx.me.state["guild"]["guild_id"] = serde_json::json!(my_gid);
            ctx.me_dirty = true;
        }
        for gid in &ctx.changed {
            if let Some(g) = ctx.all.iter().find(|g| g.guild_id == *gid) {
                self.db.save(TABLE, &gid.to_string(), g).await;
            }
        }
        if ctx.me_dirty {
            self.save_me(&ctx.me).await;
        }

        let mut per_user: BTreeMap<u64, Vec<tgf::DGuildEventLog>> = BTreeMap::new();
        for (gid, logs) in &ctx.events {
            let mut rcpt: BTreeSet<u64> =
                ctx.all.iter().find(|g| g.guild_id == *gid).map(|g| g.member_uids()).unwrap_or_default().into_iter().collect();
            if let Some(extra) = ctx.event_extra.get(gid) {
                rcpt.extend(extra.iter().copied());
            }
            for uid in rcpt {
                per_user.entry(uid).or_default().extend(logs.iter().cloned());
            }
        }
        for uid in &ctx.notify {
            per_user.entry(*uid).or_default();
        }
        let me_id = ctx.me_id();
        for (uid, logs) in per_user {
            if uid == 0 {
                continue;
            }
            let player = if uid == me_id { Some(ctx.me.clone()) } else { self.load_by_user_id(uid).await };
            let Some(p) = player else {
                println!("[Guild] no player user_id={} in DB", uid);
                continue;
            };
            let n = logs.len();
            let msg = tgf::ScPlayerDirtyChgSync {
                dirty_guild_event: logs,
                dirty_user_guild: Some(guild_user_data(&ctx.all, &p)),
                ..Default::default()
            };
            if uid == me_id {
                self.push_dirty(msg);
            } else {
                let online = common::push_to(&p.open_id, hub::DIRTY_METHOD, hub::DIRTY_TYPE, &msg);
                println!("[Guild] push -> {} (user_id={}, events={}, online={})", p.open_id, uid, n, online);
            }
        }
    }

    async fn guild_finish(&self, rpc: &str, ctx: GuildCtx, r: GResult) {
        match r {
            Ok(()) => println!("[Guild] {} user_id={} ok", rpc, ctx.me_id()),
            Err(e) => {
                println!("[Guild] {} user_id={} rejected {} ({})", rpc, ctx.me_id(), e.as_str_name(), e as i32);
                self.set_rpc_error(e as i32);
            }
        }
        self.guild_commit(ctx).await;
    }

    // ---------------------------------------------------------------- operations

    fn guild_op_create(ctx: &mut GuildCtx, req: tgf::CsGuildCreateReq) -> GResult {
        let uid = ctx.me_id();
        if ctx.guild_of(uid).is_some() {
            return Err(GErr::EnErrGuildAlreadyInGuild);
        }
        if next_join_unlock(&ctx.me) > ctx.now {
            return Err(GErr::EnErrGuildNextJoinLocked);
        }
        let profile = req.basic_profile.unwrap_or_default();
        let name = profile.name.trim().to_string();
        ctx.check_name(&name, 0)?;
        let announcement = req.detail_data.map(|d| d.announcement).unwrap_or_default();
        if announcement.chars().count() > ctx.cfg.announcement_len {
            return Err(GErr::EnErrGuildAnnouncementTooLong);
        }
        // TODO(verify): creation cost (guild_create_cost_item) — no key in const.json, free; bill_no is not checked
        let conf = req.configure.unwrap_or_default();
        let gid = ctx.all.iter().map(|g| g.guild_id + 1).max().unwrap_or(FIRST_GUILD_ID).max(FIRST_GUILD_ID);
        let now = ctx.now;
        ctx.all.push(GuildRecord {
            guild_id: gid,
            name: name.clone(),
            guild: tgf::DGuild {
                basic_profile: Some(tgf::DGuildBasicProfile { guild_id: gid, name, logo: profile.logo }),
                configure: Some(tgf::DGuildConfigure {
                    auto_approve_apply: conf.auto_approve_apply,
                    player_level_limit: conf.player_level_limit.max(0),
                    max_member_number: ctx.cfg.max_members,
                }),
                detail_data: Some(tgf::DGuildDetailData { announcement, create_timepoint: now, ..Default::default() }),
                stats: Some(tgf::DGuildStats {}),
                ..Default::default()
            },
            ..Default::default()
        });
        let gi = ctx.all.len() - 1;
        ctx.event(gi, uid, Event::CreateGuild(now));
        let level = guild_user_level(&ctx.me);
        ctx.join(gi, uid, ROLE_OWNER, level, uid, false, false);
        Ok(())
    }

    /// Leaving: the leader can leave only last (the guild is disbanded), otherwise — transfer leadership first
    fn guild_op_exit(ctx: &mut GuildCtx) -> GResult {
        let uid = ctx.me_id();
        let gi = ctx.my_guild()?;
        if ctx.all[gi].role_of(uid) == ROLE_OWNER {
            if ctx.all[gi].guild.members.len() > 1 {
                return Err(GErr::EnErrGuildAccessDeny);
            }
            ctx.destroy(gi, uid);
        } else {
            ctx.remove_member(gi, uid, uid);
        }
        if !ctx.me.state["guild"].is_object() {
            ctx.me.state["guild"] = serde_json::json!({});
        }
        ctx.me.state["guild"]["next_join_unlock_timepoint"] = serde_json::json!(ctx.now + ctx.cfg.exit_lock);
        ctx.me_dirty = true;
        Ok(())
    }

    /// pay_off (遣散 — "disband") — disbanding of the guild by the leader. TODO(verify): meaning of the RPC (the request has no fields)
    fn guild_op_pay_off(ctx: &mut GuildCtx) -> GResult {
        let gi = ctx.my_guild()?;
        ctx.require_role(gi, &[ROLE_OWNER])?;
        let uid = ctx.me_id();
        ctx.destroy(gi, uid);
        Ok(())
    }

    fn guild_op_set_role(ctx: &mut GuildCtx, req: tgf::CsGuildSetRoleReq) -> GResult {
        let me = ctx.me_id();
        let gi = ctx.my_guild()?;
        ctx.require_role(gi, &[ROLE_OWNER])?;
        let target = uid_of(&req.user_key);
        if target == me {
            return Err(GErr::EnErrInvalidParam);
        }
        let current = ctx.all[gi].role_of(target);
        if current == 0 {
            return Err(GErr::EnErrGuildMemberNotFound);
        }
        match req.role_type {
            ROLE_OWNER => {
                let d = ctx.all[gi].guild.detail_data.get_or_insert_with(Default::default);
                if d.next_transfer_owner_unlock_timepoint > ctx.now {
                    return Err(GErr::EnErrGuildTransferOwnerLocked);
                }
                d.next_transfer_owner_unlock_timepoint = ctx.now + ctx.cfg.set_owner_lock;
                // TODO(verify): the former leader becomes a member (not an officer)
                ctx.set_role(gi, target, ROLE_OWNER, me);
                ctx.set_role(gi, me, ROLE_MEMBER, me);
            }
            ROLE_ADMIN | ROLE_MEMBER => {
                if current != req.role_type {
                    ctx.set_role(gi, target, req.role_type, me);
                }
            }
            _ => return Err(GErr::EnErrInvalidParam),
        }
        Ok(())
    }

    fn guild_op_rename(ctx: &mut GuildCtx, req: tgf::CsGuildRenameReq) -> GResult {
        let me = ctx.me_id();
        let gi = ctx.my_guild()?;
        ctx.require_role(gi, &[ROLE_OWNER])?;
        let name = req.set_name.trim().to_string();
        ctx.check_name(&name, ctx.all[gi].guild_id)?;
        // TODO(verify): rename cost (guild_rename_cost_item) — no key in const.json, free
        let g = &mut ctx.all[gi];
        g.name = name.clone();
        g.guild.basic_profile.get_or_insert_with(Default::default).name = name.clone();
        ctx.event(gi, me, Event::SetName(name));
        Ok(())
    }

    /// Empty logo/announcement strings mean "do not change" (proto3 has no presence flag). TODO(verify)
    fn guild_op_set_configure(ctx: &mut GuildCtx, req: tgf::CsGuildSetConfigureReq) -> GResult {
        let me = ctx.me_id();
        let gi = ctx.my_guild()?;
        ctx.require_role(gi, &[ROLE_OWNER, ROLE_ADMIN])?;
        if req.set_announcement.chars().count() > ctx.cfg.announcement_len {
            return Err(GErr::EnErrGuildAnnouncementTooLong);
        }
        let g = &ctx.all[gi];
        let logo = g.guild.basic_profile.as_ref().map(|p| p.logo.clone()).unwrap_or_default();
        let ann = g.guild.detail_data.as_ref().map(|d| d.announcement.clone()).unwrap_or_default();
        let conf = g.guild.configure.clone().unwrap_or_default();
        if !req.set_logo.is_empty() && req.set_logo != logo {
            ctx.all[gi].guild.basic_profile.get_or_insert_with(Default::default).logo = req.set_logo.clone();
            ctx.event(gi, me, Event::SetLogo(req.set_logo.clone()));
        }
        if !req.set_announcement.is_empty() && req.set_announcement != ann {
            ctx.all[gi].guild.detail_data.get_or_insert_with(Default::default).announcement = req.set_announcement.clone();
            ctx.event(gi, me, Event::SetAnnouncement(req.set_announcement.clone()));
        }
        if req.auto_approve_apply != conf.auto_approve_apply {
            ctx.all[gi].guild.configure.get_or_insert_with(Default::default).auto_approve_apply = req.auto_approve_apply;
            ctx.event(gi, me, Event::SetConfigureAutoApproveApply(req.auto_approve_apply));
        }
        let limit = req.player_level_limit.max(0);
        if limit != conf.player_level_limit {
            ctx.all[gi].guild.configure.get_or_insert_with(Default::default).player_level_limit = limit;
            ctx.event(gi, me, Event::SetConfigurePlayerLevelLimit(limit));
        }
        Ok(())
    }

    fn guild_op_send_apply(ctx: &mut GuildCtx, req: tgf::CsGuildSendApplyReq) -> GResult {
        let me = ctx.me_id();
        if ctx.guild_of(me).is_some() {
            return Err(GErr::EnErrGuildAlreadyInGuild);
        }
        if next_join_unlock(&ctx.me) > ctx.now {
            return Err(GErr::EnErrGuildNextJoinLocked);
        }
        let gi = ctx.alive_idx(req.guild_id)?;
        if ctx.all[gi].apply_of(me).is_some() {
            return Ok(());
        }
        let level = guild_user_level(&ctx.me);
        if level < ctx.all[gi].level_limit() {
            return Err(GErr::EnErrGuildPlayerLevelTooLow);
        }
        if ctx.all[gi].is_full() {
            return Err(GErr::EnErrGuildFullOfMembers);
        }
        let related: Vec<u64> = ctx.all.iter().filter(|g| !g.destroyed && g.apply_of(me).is_some()).map(|g| g.guild_id).collect();
        // TODO(verify): which code "too many applications" has (EN_ERR_GUILD_USER_LEVEL_LIMIT is used)
        if related.len() >= ctx.cfg.apply_max_per_user {
            return Err(GErr::EnErrGuildUserLevelLimit);
        }
        if ctx.all[gi].guild.apply.len() >= ctx.cfg.apply_max {
            return Err(GErr::EnErrGuildFullOfMembers);
        }
        let auto = ctx.all[gi].guild.configure.as_ref().map(|c| c.auto_approve_apply).unwrap_or(false);
        if auto || ctx.all[gi].invitation_of(me).is_some() {
            ctx.join(gi, me, ROLE_MEMBER, level, me, true, false);
            return Ok(());
        }
        let apply = tgf::DGuildMemberApply {
            user_key: Some(ukey(me)),
            related_guild_ids: related,
            user_level: level,
            create_timepoint: ctx.now,
            expired_timepoint: ctx.now + ctx.cfg.apply_expire,
            operation_event_id: ctx.all[gi].next_event_id + 1,
            ..Default::default()
        };
        ctx.all[gi].guild.apply.push(apply.clone());
        ctx.event(gi, me, Event::AddApply(apply));
        ctx.notify.insert(me);
        Ok(())
    }

    fn guild_op_approve_apply(ctx: &mut GuildCtx, req: tgf::CsGuildApproveApplyReq) -> GResult {
        let me = ctx.me_id();
        let gi = ctx.my_guild()?;
        ctx.require_role(gi, &[ROLE_OWNER, ROLE_ADMIN])?;
        let target = uid_of(&req.user_key);
        let Some(pos) = ctx.all[gi].guild.apply.iter().position(|a| uid_of(&a.user_key) == target) else {
            return Err(GErr::EnErrGuildApplyNotFound);
        };
        if ctx.guild_of(target).is_some() {
            let apply = ctx.all[gi].guild.apply.remove(pos);
            ctx.event(gi, me, Event::RejectApply(apply));
            ctx.notify.insert(target);
            return Err(GErr::EnErrGuildAlreadyInGuild);
        }
        if ctx.all[gi].is_full() {
            return Err(GErr::EnErrGuildFullOfMembers);
        }
        let apply = ctx.all[gi].guild.apply.remove(pos);
        let level = apply.user_level;
        ctx.event(gi, me, Event::ApproveApply(apply));
        ctx.join(gi, target, ROLE_MEMBER, level, me, true, false);
        Ok(())
    }

    fn guild_op_reject_apply(ctx: &mut GuildCtx, req: tgf::CsGuildRejectApplyReq) -> GResult {
        let me = ctx.me_id();
        let gi = ctx.my_guild()?;
        ctx.require_role(gi, &[ROLE_OWNER, ROLE_ADMIN])?;
        let target = uid_of(&req.user_key);
        let Some(pos) = ctx.all[gi].guild.apply.iter().position(|a| uid_of(&a.user_key) == target) else {
            return Err(GErr::EnErrGuildApplyNotFound);
        };
        let apply = ctx.all[gi].guild.apply.remove(pos);
        ctx.event(gi, me, Event::RejectApply(apply));
        ctx.notify.insert(target);
        Ok(())
    }

    /// The leader and officers invite. TODO(verify): whether a regular member can invite
    async fn guild_op_send_invitation(&self, ctx: &mut GuildCtx, req: tgf::CsGuildSendInvitationReq) -> GResult {
        let me = ctx.me_id();
        let gi = ctx.my_guild_checked(req.guild_id)?;
        ctx.require_role(gi, &[ROLE_OWNER, ROLE_ADMIN])?;
        let target = uid_of(&req.invitee);
        if target == 0 || target == me {
            return Err(GErr::EnErrInvalidParam);
        }
        let Some(invitee) = self.load_by_user_id(target).await else {
            return Err(GErr::EnErrUserNotFound);
        };
        if ctx.guild_of(target).is_some() {
            return Err(GErr::EnErrGuildAlreadyInGuild);
        }
        if ctx.all[gi].invitation_of(target).is_some() {
            return Ok(());
        }
        if ctx.all[gi].is_full() {
            return Err(GErr::EnErrGuildFullOfMembers);
        }
        let related: Vec<u64> =
            ctx.all.iter().filter(|g| !g.destroyed && g.invitation_of(target).is_some()).map(|g| g.guild_id).collect();
        if related.len() >= ctx.cfg.invitation_max_per_user {
            return Err(GErr::EnErrGuildUserLevelLimit);
        }
        if ctx.all[gi].guild.invitation.len() >= ctx.cfg.invitation_max {
            return Err(GErr::EnErrGuildFullOfMembers);
        }
        let inv = tgf::DGuildMemberInvitation {
            user_key: Some(ukey(target)),
            related_guild_ids: related,
            user_level: guild_user_level(&invitee),
            create_timepoint: ctx.now,
            expired_timepoint: ctx.now + ctx.cfg.invitation_expire,
            operation_event_id: ctx.all[gi].next_event_id + 1,
            ..Default::default()
        };
        ctx.all[gi].guild.invitation.push(inv.clone());
        ctx.event(gi, me, Event::AddInvitation(inv));
        ctx.notify.insert(target);
        Ok(())
    }

    fn guild_op_approve_invitation(ctx: &mut GuildCtx, req: tgf::CsGuildApproveInvitationReq) -> GResult {
        let me = ctx.me_id();
        if ctx.guild_of(me).is_some() {
            return Err(GErr::EnErrGuildAlreadyInGuild);
        }
        if next_join_unlock(&ctx.me) > ctx.now {
            return Err(GErr::EnErrGuildNextJoinLocked);
        }
        let gi = ctx.alive_idx(req.guild_id)?;
        let Some(pos) = ctx.all[gi].guild.invitation.iter().position(|i| uid_of(&i.user_key) == me) else {
            return Err(GErr::EnErrGuildInvitationNotFound);
        };
        if ctx.all[gi].is_full() {
            return Err(GErr::EnErrGuildFullOfMembers);
        }
        let inv = ctx.all[gi].guild.invitation.remove(pos);
        ctx.event(gi, me, Event::ApproveInvitation(inv));
        let level = guild_user_level(&ctx.me);
        ctx.join(gi, me, ROLE_MEMBER, level, me, false, true);
        Ok(())
    }

    fn guild_op_reject_invitation(ctx: &mut GuildCtx, req: tgf::CsGuildRejectInvitationReq) -> GResult {
        let me = ctx.me_id();
        let gi = ctx.alive_idx(req.guild_id)?;
        let Some(pos) = ctx.all[gi].guild.invitation.iter().position(|i| uid_of(&i.user_key) == me) else {
            return Err(GErr::EnErrGuildInvitationNotFound);
        };
        let inv = ctx.all[gi].guild.invitation.remove(pos);
        ctx.event(gi, me, Event::RejectInvitation(inv));
        ctx.notify.insert(me);
        Ok(())
    }

    /// Revoking an invitation — proto has no separate event, we write RejectInvitation with the revoking operator
    fn guild_op_revoke_invitation(ctx: &mut GuildCtx, req: tgf::CsGuildRevokeInvitationReq) -> GResult {
        let me = ctx.me_id();
        let gi = ctx.my_guild_checked(req.guild_id)?;
        ctx.require_role(gi, &[ROLE_OWNER, ROLE_ADMIN])?;
        let target = uid_of(&req.invitee);
        let Some(pos) = ctx.all[gi].guild.invitation.iter().position(|i| uid_of(&i.user_key) == target) else {
            return Err(GErr::EnErrGuildInvitationNotFound);
        };
        let inv = ctx.all[gi].guild.invitation.remove(pos);
        ctx.event(gi, me, Event::RejectInvitation(inv));
        ctx.notify.insert(target);
        Ok(())
    }

    /// The leader kicks anyone, an officer — only regular members
    fn guild_op_remove_member(ctx: &mut GuildCtx, req: tgf::CsGuildRemoveMemberReq) -> GResult {
        let me = ctx.me_id();
        let gi = ctx.my_guild()?;
        let target = uid_of(&req.user_key);
        if target == me {
            return Err(GErr::EnErrInvalidParam);
        }
        let target_role = ctx.all[gi].role_of(target);
        if target_role == 0 {
            return Err(GErr::EnErrGuildMemberNotFound);
        }
        let allowed = match ctx.all[gi].role_of(me) {
            ROLE_OWNER => true,
            ROLE_ADMIN => target_role == ROLE_MEMBER,
            _ => false,
        };
        if !allowed {
            return Err(GErr::EnErrGuildAccessDeny);
        }
        ctx.remove_member(gi, target, me);
        Ok(())
    }

    /// Impeachment of the leader: the leader is offline and has not entered the guild for guild_impeachment_offline_time;
    /// the initiator votes "for" automatically. TODO(verify): the initiator's auto-vote, the "long absent" check
    async fn guild_op_add_impeachment(&self, ctx: &mut GuildCtx) -> GResult {
        let me = ctx.me_id();
        let gi = ctx.my_guild()?;
        if ctx.all[gi].role_of(me) == ROLE_OWNER {
            return Err(GErr::EnErrGuildImpeachmentCanNotChallengeSelf);
        }
        if ctx.all[gi].impeachment_running() {
            return Err(GErr::EnErrGuildImpeachmentAnotherRunning);
        }
        if ctx.me.state["guild"]["impeachment"]["next_unlock_timepoint"].as_i64().unwrap_or(0) > ctx.now {
            return Err(GErr::EnErrGuildImpeachmentLocked);
        }
        let Some(owner) = ctx.all[gi].owner_uid() else { return Err(GErr::EnErrGuildMemberNotFound) };
        let owner_online = match self.load_by_user_id(owner).await {
            Some(p) => hub::is_online(&p.open_id),
            None => false,
        };
        let last_seen = ctx.all[gi].member(owner).map(|m| m.last_subscribe_timepoint).unwrap_or(0);
        if owner_online || last_seen + ctx.cfg.imp_offline > ctx.now {
            return Err(GErr::EnErrGuildImpeachmentDenyOwnerOnline);
        }
        let event_id = ctx.all[gi].next_event_id + 1;
        let eligible = ctx.all[gi].guild.members.len().saturating_sub(1) as i64;
        let imp = tgf::DGuildImpeachmentData {
            initiator: Some(ukey(me)),
            event_id,
            target_user_key: Some(ukey(owner)),
            start_timepoint: ctx.now,
            finish_timepoint: ctx.now + ctx.cfg.imp_duration,
            show_expire_timepoint: 0,
            is_approved: false,
            finish_member_size: eligible,
            agree_users: vec![ukey(me)],
            disagree_users: vec![],
        };
        ctx.all[gi].guild.impeachment = Some(imp.clone());
        ctx.event(gi, me, Event::StartImpeachment(imp));
        if !ctx.me.state["guild"].is_object() {
            ctx.me.state["guild"] = serde_json::json!({});
        }
        ctx.me.state["guild"]["impeachment"] = serde_json::json!({
            "next_unlock_timepoint": ctx.now + ctx.cfg.imp_interval,
            "last_vote_event_id": event_id,
            "last_vote_agree": true,
        });
        ctx.me_dirty = true;
        ctx.impeachment_check_early(gi);
        Ok(())
    }

    fn guild_op_vote_impeachment(ctx: &mut GuildCtx, req: tgf::CsGuildVoteImpeachmentReq) -> GResult {
        let me = ctx.me_id();
        let gi = ctx.my_guild()?;
        if !ctx.all[gi].impeachment_running() {
            return Err(GErr::EnErrGuildImpeachmentNotRunning);
        }
        let mut imp = ctx.all[gi].guild.impeachment.clone().unwrap_or_default();
        if req.event_id != 0 && req.event_id != imp.event_id {
            return Err(GErr::EnErrGuildImpeachmentNotRunning);
        }
        if uid_of(&imp.target_user_key) == me {
            return Err(GErr::EnErrGuildImpeachmentCanNotChallengeSelf);
        }
        if imp.agree_users.iter().chain(imp.disagree_users.iter()).any(|k| k.user_id == me) {
            return Err(GErr::EnErrGuildImpeachmentAlreadyVote);
        }
        if req.agree {
            imp.agree_users.push(ukey(me));
        } else {
            imp.disagree_users.push(ukey(me));
        }
        let event_id = imp.event_id;
        ctx.all[gi].guild.impeachment = Some(imp);
        ctx.event(gi, me, Event::VoteImpeachment(tgf::DGuildImpeachmentVote { user_key: Some(ukey(me)), event_id, agree: req.agree }));
        if !ctx.me.state["guild"].is_object() {
            ctx.me.state["guild"] = serde_json::json!({});
        }
        if !ctx.me.state["guild"]["impeachment"].is_object() {
            ctx.me.state["guild"]["impeachment"] = serde_json::json!({});
        }
        ctx.me.state["guild"]["impeachment"]["last_vote_event_id"] = serde_json::json!(event_id);
        ctx.me.state["guild"]["impeachment"]["last_vote_agree"] = serde_json::json!(req.agree);
        ctx.me_dirty = true;
        ctx.impeachment_check_early(gi);
        Ok(())
    }

    // ---------------------------------------------------------------- RPC

    pub(super) async fn guild_create_impl(&self, req: tgf::CsGuildCreateReq) -> Option<tgf::ScGuildCreateRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_create(&mut ctx, req);
        self.guild_finish("guild_create", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_exit_impl(&self, _req: tgf::CsGuildExitReq) -> Option<tgf::ScGuildExitRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_exit(&mut ctx);
        self.guild_finish("guild_exit", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_pay_off_impl(&self, _req: tgf::CsGuildPayOffReq) -> Option<tgf::ScGuildPayOffRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_pay_off(&mut ctx);
        self.guild_finish("guild_pay_off", ctx, r).await;
        Some(Default::default())
    }

    /// My whole guild (the log — the last guild_log_show_count) + my DUserGuildData.
    /// Also marks the member's last_subscribe_timepoint ("long absent" for impeachment is counted from it).
    pub(super) async fn guild_get_all_impl(&self, _req: tgf::CsGuildGetAllReq) -> Option<tgf::ScGuildGetAllRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let me = ctx.me_id();
        let now = ctx.now;
        let guild_data = match ctx.guild_of(me) {
            Some(gi) => {
                if let Some(m) = ctx.all[gi].guild.members.iter_mut().find(|m| uid_of(&m.user_key) == me) {
                    m.last_subscribe_timepoint = now;
                }
                ctx.touch(gi);
                Some(ctx.all[gi].full_view(ctx.cfg.log_show))
            }
            None => None,
        };
        let user_guild = Some(guild_user_data(&ctx.all, &ctx.me));
        self.guild_finish("guild_get_all", ctx, Ok(())).await;
        Some(tgf::ScGuildGetAllRsp { guild_data, user_guild })
    }

    /// By id and/or exact name (case-insensitive), at most guild_batch_get_max_count; disbanded ones too
    /// (with destroy_timepoint), unknown ones are skipped
    pub(super) async fn guild_batch_get_impl(&self, req: tgf::CsGuildBatchGetReq) -> Option<tgf::ScGuildBatchGetRsp> {
        let _lock = guild_lock().lock().await;
        let ctx = self.guild_begin().await;
        let names: Vec<String> = req.guild_name.iter().map(|n| n.trim().to_lowercase()).filter(|n| !n.is_empty()).collect();
        let mut ids: Vec<u64> = Vec::new();
        for id in req.guild_ids.iter().copied() {
            if ctx.find_idx(id).is_some() && !ids.contains(&id) {
                ids.push(id);
            }
        }
        for g in ctx.all.iter().filter(|g| !g.destroyed && names.contains(&g.profile_name().to_lowercase())) {
            if !ids.contains(&g.guild_id) {
                ids.push(g.guild_id);
            }
        }
        ids.truncate(ctx.cfg.batch_max);
        let guild_data =
            ids.iter().filter_map(|id| ctx.all.iter().find(|g| g.guild_id == *id)).map(|g| g.public_view()).collect();
        self.guild_finish("guild_batch_get", ctx, Ok(())).await;
        Some(tgf::ScGuildBatchGetRsp { guild_data })
    }

    /// Recommendations: live non-full guilds (not mine) we qualify for by level; more members — higher
    pub(super) async fn guild_suggest_impl(&self, _req: tgf::CsGuildSuggestReq) -> Option<tgf::ScGuildSuggestRsp> {
        let _lock = guild_lock().lock().await;
        let ctx = self.guild_begin().await;
        let me = ctx.me_id();
        let level = guild_user_level(&ctx.me);
        let mut list: Vec<&GuildRecord> = ctx
            .all
            .iter()
            .filter(|g| !g.destroyed && !g.is_member(me) && !g.is_full() && g.level_limit() <= level)
            .collect();
        list.sort_by(|a, b| b.guild.members.len().cmp(&a.guild.members.len()).then(a.guild_id.cmp(&b.guild_id)));
        let guilds: Vec<u64> = list.iter().take(ctx.cfg.batch_max).map(|g| g.guild_id).collect();
        self.guild_finish("guild_suggest", ctx, Ok(())).await;
        Some(tgf::ScGuildSuggestRsp { sugget_data: Some(tgf::DGuildSuggestData { guilds }) })
    }

    pub(super) async fn guild_set_role_impl(&self, req: tgf::CsGuildSetRoleReq) -> Option<tgf::ScGuildSetRoleRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_set_role(&mut ctx, req);
        self.guild_finish("guild_set_role", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_rename_impl(&self, req: tgf::CsGuildRenameReq) -> Option<tgf::ScGuildRenameRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_rename(&mut ctx, req);
        self.guild_finish("guild_rename", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_set_configure_impl(&self, req: tgf::CsGuildSetConfigureReq) -> Option<tgf::ScGuildSetConfigureRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_set_configure(&mut ctx, req);
        self.guild_finish("guild_set_configure", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_send_apply_impl(&self, req: tgf::CsGuildSendApplyReq) -> Option<tgf::ScGuildSendApplyRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_send_apply(&mut ctx, req);
        self.guild_finish("guild_send_apply", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_approve_apply_impl(&self, req: tgf::CsGuildApproveApplyReq) -> Option<tgf::ScGuildApproveApplyRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_approve_apply(&mut ctx, req);
        self.guild_finish("guild_approve_apply", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_reject_apply_impl(&self, req: tgf::CsGuildRejectApplyReq) -> Option<tgf::ScGuildRejectApplyRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_reject_apply(&mut ctx, req);
        self.guild_finish("guild_reject_apply", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_send_invitation_impl(&self, req: tgf::CsGuildSendInvitationReq) -> Option<tgf::ScGuildSendInvitationRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = self.guild_op_send_invitation(&mut ctx, req).await;
        self.guild_finish("guild_send_invitation", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_approve_invitation_impl(&self, req: tgf::CsGuildApproveInvitationReq) -> Option<tgf::ScGuildApproveInvitationRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_approve_invitation(&mut ctx, req);
        self.guild_finish("guild_approve_invitation", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_reject_invitation_impl(&self, req: tgf::CsGuildRejectInvitationReq) -> Option<tgf::ScGuildRejectInvitationRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_reject_invitation(&mut ctx, req);
        self.guild_finish("guild_reject_invitation", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_revoke_invitation_impl(&self, req: tgf::CsGuildRevokeInvitationReq) -> Option<tgf::ScGuildRevokeInvitationRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_revoke_invitation(&mut ctx, req);
        self.guild_finish("guild_revoke_invitation", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_remove_member_impl(&self, req: tgf::CsGuildRemoveMemberReq) -> Option<tgf::ScGuildRemoveMemberRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_remove_member(&mut ctx, req);
        self.guild_finish("guild_remove_member", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_add_impeachment_impl(&self, _req: tgf::CsGuildAddImpeachmentReq) -> Option<tgf::ScGuildAddImpeachmentRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = self.guild_op_add_impeachment(&mut ctx).await;
        self.guild_finish("guild_add_impeachment", ctx, r).await;
        Some(Default::default())
    }

    pub(super) async fn guild_vote_impeachment_impl(&self, req: tgf::CsGuildVoteImpeachmentReq) -> Option<tgf::ScGuildVoteImpeachmentRsp> {
        let _lock = guild_lock().lock().await;
        let mut ctx = self.guild_begin().await;
        let r = Self::guild_op_vote_impeachment(&mut ctx, req);
        self.guild_finish("guild_vote_impeachment", ctx, r).await;
        Some(Default::default())
    }
}
