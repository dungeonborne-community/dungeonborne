//! Outgoing `tgf.DsRpcService` calls (backend → game server) through the ds-agent queue.
//! Direction per the protocol: the service lives on the DS (Configure_decoded.txt), the backend is its client.

use super::agent_link;
use crate::tgf;
use prost::Message;
use std::time::Duration;

#[derive(Debug)]
pub enum CallError {
    /// This server address has no live ds-agent (static DS_POOL, local test)
    NoAgent,
    Timeout,
    /// The agent replied with an error or the reply could not be parsed
    Failed(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::NoAgent => write!(f, "no ds-agent"),
            CallError::Timeout => write!(f, "timeout"),
            CallError::Failed(e) => write!(f, "{}", e),
        }
    }
}

/// Raw call of a DsRpcService method on server `ds_address`
pub async fn call(ds_address: &str, battle_id: &str, method: &str, payload: Vec<u8>, timeout: Duration) -> Result<Vec<u8>, CallError> {
    let agent = agent_link::agent_for(ds_address).ok_or(CallError::NoAgent)?;
    let (cmd_id, rx) = agent_link::enqueue(&agent, battle_id, method, payload).ok_or(CallError::NoAgent)?;
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(Ok(bytes))) => Ok(bytes),
        Ok(Ok(Err(e))) => Err(CallError::Failed(e)),
        Ok(Err(_)) => Err(CallError::Failed("reply channel closed".into())),
        Err(_) => {
            agent_link::forget(cmd_id);
            Err(CallError::Timeout)
        }
    }
}

/// init_player: hand the room to the server (players + match parameters), get the players' keys/tokens
pub async fn init_player(ds_address: &str, battle_id: &str, req: &tgf::DsInitPlayerListReq, timeout: Duration) -> Result<tgf::DsInitPlayerListRsp, CallError> {
    let bytes = call(ds_address, battle_id, "init_player", req.encode_to_vec(), timeout).await?;
    tgf::DsInitPlayerListRsp::decode(bytes.as_slice()).map_err(|e| CallError::Failed(format!("DSInitPlayerListRsp: {}", e)))
}
