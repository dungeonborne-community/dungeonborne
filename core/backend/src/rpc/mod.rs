pub mod handlers;

use prost::Message;
use std::time::{SystemTime, UNIX_EPOCH};

/// RPC route in the header: rpc_request (31), rpc_response (32), rpc_stream (33) — shared fields version/rpc_name/type_url
#[derive(Clone, PartialEq, Message)]
pub struct RpcRouting {
    #[prost(string, tag = "1")]
    pub version: String,
    #[prost(string, tag = "21")]
    pub method: String,
    #[prost(string, tag = "22")]
    pub type_url: String,
}

/// RPC message header = atframework.CSMsgHead (schema_dump.proto, Network.pb). Field numbers — as in the client.
/// Before 02.11 the fields were guessed: field 3 carried our S2C counter, 4 — the request number, the error — 6 (that is op_type).
/// The client matches a response to a request by client_sequence (3): while there were no pushes the counters matched and the callbacks
/// fired; after the first push they did not (hall_set_role: the character select window did not close from the 2nd switch on).
#[derive(Clone, PartialEq, Message)]
pub struct RpcHead {
    #[prost(sint32, tag = "1")]
    pub error_code: i32,
    #[prost(uint64, tag = "2")]
    pub timestamp: u64,
    /// client request number; echoed in a response, 0 in a push
    #[prost(uint64, tag = "3")]
    pub client_sequence: u64,
    /// our outgoing counter (responses + pushes)
    #[prost(uint64, tag = "4")]
    pub server_sequence: u64,
    #[prost(uint64, tag = "5")]
    pub session_sequence: u64,
    #[prost(int32, tag = "6")]
    pub op_type: i32,
    #[prost(uint64, tag = "7")]
    pub sequence_ack: u64,
    #[prost(uint64, tag = "11")]
    pub session_node_id: u64,
    #[prost(uint64, tag = "12")]
    pub session_id: u64,
    #[prost(message, optional, tag = "31")]
    pub c2s_routing: Option<RpcRouting>,
    #[prost(message, optional, tag = "32")]
    pub s2c_routing: Option<RpcRouting>,
    /// push route (atframework.RpcStreamMeta)
    #[prost(message, optional, tag = "33")]
    pub stream_routing: Option<RpcRouting>,
}

/// version in the route — as in PVE and the client
pub const RPC_VERSION: &str = "0.0.0";

/// Full RPC packet (Header + Protobuf Body)
#[derive(Clone, PartialEq, Message)]
pub struct RpcFrame {
    #[prost(message, optional, tag = "1")]
    pub head: Option<RpcHead>,
    #[prost(bytes = "vec", tag = "3")]
    pub body: Vec<u8>,
}

/// Builds a binary RpcFrame to send to the client (Response or Server Push)
pub fn build_rpc_frame(
    rpc_seq: u64,
    req_seq: u64,
    method: &str,
    type_url: &str,
    body: Vec<u8>,
) -> Vec<u8> {
    build_rpc_frame_with_error(rpc_seq, req_seq, method, type_url, body, 0)
}

/// The same, with an error code in the header (RpcHead.error_code, codes — error_code table)
pub fn build_rpc_frame_with_error(
    rpc_seq: u64,
    req_seq: u64,
    method: &str,
    type_url: &str,
    body: Vec<u8>,
    error_code: i32,
) -> Vec<u8> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let route = RpcRouting { version: RPC_VERSION.to_string(), method: method.to_string(), type_url: type_url.to_string() };
    let frame = RpcFrame {
        // As in PVE SoloLobby (rpc_dump/PVE_head.jsonl 02.11): response — client_sequence (request echo), server_sequence,
        // timestamp, rpc_response; push (req_seq 0) — server_sequence, timestamp, rpc_stream. PVE does not send fields 5/6/7/11/12.
        head: Some(RpcHead {
            error_code,
            timestamp: now,
            client_sequence: req_seq,
            server_sequence: rpc_seq,
            s2c_routing: (req_seq > 0).then(|| route.clone()),
            stream_routing: (req_seq == 0).then_some(route),
            ..Default::default()
        }),
        body,
    };
    frame.encode_to_vec()
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Response: request number echo in field 3, our counter in 4, error in field 1 as sint32 (zigzag)
    #[test]
    fn response_head_matches_csmsghead() {
        let raw = build_rpc_frame_with_error(40, 27, "m", "t", vec![], -726);
        let head = RpcFrame::decode(raw.as_slice()).unwrap().head.unwrap();
        assert_eq!((head.client_sequence, head.server_sequence, head.error_code), (27, 40, -726));
        // head = field 1 of the frame (0x0a, length), the first field of head — tag 1 varint, zigzag(-726) = 1451
        assert_eq!(&raw[2..5], &[0x08, 0xab, 0x0b]);
        assert!(head.s2c_routing.is_some() && head.stream_routing.is_none());
    }

    /// Push: route in rpc_stream (33), without client_sequence and rpc_response
    #[test]
    fn push_head_uses_rpc_stream() {
        let raw = build_rpc_frame(41, 0, "m", "t", vec![]);
        let head = RpcFrame::decode(raw.as_slice()).unwrap().head.unwrap();
        assert_eq!((head.client_sequence, head.server_sequence), (0, 41));
        assert!(head.s2c_routing.is_none());
        assert_eq!(head.stream_routing.unwrap().version, RPC_VERSION);
    }
}
