//! Lobby: session registry (pushes to other players), matchmaker, game server allocation, parties, private rooms.
//! All state lives in process memory (a single gateway replica); when scaling out, move it to Redis/NATS.

pub mod allocator;
pub mod hub;
pub mod launcher_lobby;
pub mod maintenance;
pub mod match_checks;
pub mod matchmaking;
pub mod room;
pub mod team;

/// Starts the lobby background tasks (matchmaker tick, returning rooms from a match).
pub fn spawn_background() {
    tokio::spawn(async {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
        loop {
            tick.tick().await;
            launcher_lobby::tick();
            matchmaking::tick();
            room::tick();
            // R1-stuck: did not reach the server within DS_ARRIVE_SECS after startBattling — out of the room, the client gets "match over"
            for (battle, open_id) in matchmaking::take_not_arrived() {
                let (marked, _) = matchmaking::members_absent(&battle, std::slice::from_ref(&open_id), false);
                for o in &marked {
                    crate::rpc::handlers::room_finish_push(o, &battle, None, None);
                }
                println!("[Matchmaking] {} did not reach the server of room {} — removed from the room, \"match over\" {:?}", open_id, battle, marked);
            }
        }
    });
}

/// A player disconnected from the gateway: take them off the search (the whole party — its ticket is shared),
/// off the party search (team_match_search) and out of the private room.
pub fn on_disconnect(open_id: &str) {
    matchmaking::cancel(open_id, "disconnect");
    // The old session was displaced by a repeated login — the player is still here, leave the room/party search alone
    if hub::is_online(open_id) {
        return;
    }
    team::on_disconnect(open_id);
    room::on_disconnect(open_id);
}
