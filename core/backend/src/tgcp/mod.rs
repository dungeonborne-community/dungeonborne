pub mod crypto;

use bytes::{BufMut, BytesMut};

/// TConnd header magic number (0x3366 = "3f")
pub const TGCP_MAGIC: u16 = 0x3366;

/// Ready-made template of packet 0x1002 (BinBingo) containing SERVER_SESSION_KEY
pub const BIN_BINGO_0006: [u8; 48] = [
    0x33, 0x66, 0x00, 0x0b, 0x00, 0x0c, 0x10, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x30, 0x00, 0x00, 0x00, 0x00, 0x02, 0x10, 0x10, 0xf0, 0x04, 0x7c, 0xb7, 0x49, 0x27, 0x6f, 0xdb,
    0xbc, 0x43, 0x02, 0x1d, 0x58, 0x8b, 0xfc, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Reply 0x2002 (TConnd authorization) — a PVE SoloLobby frame (rpc_dump/PVE_tgcp.jsonl 02.11), open_id at [28..45]
/// (an 18-byte field: 17 SteamID digits + 0) is substituted per client. Tokens "LOCAL_ATK"/"LOCAL_RTK" with expiry 86400 — as in PVE.
/// Before 02.11 we sent someone else's capture (dump_0008_GATEWAY_S2C.bin: open_id 76561198000000901, binary tokens) to all clients.
/// The 0x6002 after it is from the same old capture in full (21 + 49 body bytes): the PVE sniffer cut it to 21 bytes
/// (try_read_frame treated 0x6002 as a frame without a body), and with the truncated one the client dropped the connection.
const AUTH_RSP_2002_6002: [u8; 160] = [
    0x33, 0x66, 0x00, 0x0b, 0x00, 0x0c, 0x20, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
    0x15, 0x00, 0x00, 0x00, 0x45, 0x00, 0x15, 0x03, 0x00, 0x00, 0x00, 0x12, 0x30, 0x30, 0x30, 0x30,
    0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x4c, 0x4f, 0x43, 0x41, 0x4c, 0x5f,
    0x41, 0x54, 0x4b, 0x00, 0x01, 0x51, 0x80, 0x00, 0x09, 0x4c, 0x4f, 0x43, 0x41, 0x4c, 0x5f, 0x52,
    0x54, 0x4b, 0x00, 0x01, 0x51, 0x80, 0x00, 0x00, 0x00, 0x00, 0x33, 0x66, 0x00, 0x0b, 0x00, 0x0c,
    0x60, 0x02, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x15, 0x00, 0x00, 0x00, 0x31, 0x00,
    0x00, 0x00, 0x00, 0x1b, 0x9c, 0x8b, 0xc5, 0x97, 0xb4, 0xeb, 0x4b, 0x65, 0x4f, 0xc2, 0x79, 0x3b,
    0x5b, 0xcc, 0xe9, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// 0x2002 + 0x6002 for the client with this open_id
pub fn auth_rsp_2002(open_id: &str) -> Vec<u8> {
    let mut out = AUTH_RSP_2002_6002.to_vec();
    let id = open_id.as_bytes();
    let n = id.len().min(17);
    out[28..46].fill(0);
    out[28..28 + n].copy_from_slice(&id[..n]);
    out
}

/// Unpacked TGCP protocol frame
pub struct TgcpFrame {
    pub cmd: u16,
    pub raw_frame: BytesMut,
}

/// Tries to read one complete TGCP frame from the TCP buffer
pub fn try_read_frame(buf: &mut BytesMut) -> Result<Option<TgcpFrame>, String> {
    if buf.len() < 21 {
        return Ok(None);
    }

    let magic = u16::from_be_bytes([buf[0], buf[1]]);
    if magic != TGCP_MAGIC {
        return Err(format!("Bad TGCP magic: 0x{:04x}", magic));
    }

    let cmd = u16::from_be_bytes([buf[6], buf[7]]);
    let total_len = match cmd {
        0x1001 | 0x1002 => {
            u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]) as usize
        }
        0x2001 | 0x2002 => {
            let blen = u32::from_be_bytes([buf[17], buf[18], buf[19], buf[20]]) as usize;
            21 + blen
        }
        0x4013 => {
            if buf.len() < 25 {
                return Ok(None);
            }
            let blen = u32::from_be_bytes([buf[17], buf[18], buf[19], buf[20]]) as usize;
            25 + blen
        }
        0x6001 | 0x6002 => {
            // body length at [17..21] BE, as in 0x2001/0x2002 (0x6002 after authorization — 49 body bytes)
            let blen = u32::from_be_bytes([buf[17], buf[18], buf[19], buf[20]]) as usize;
            21 + blen
        }
        0x5001 => buf.len(),
        _ => {
            return Err(format!("Unknown TGCP cmd: 0x{:04x}", cmd));
        }
    };

    if buf.len() < total_len {
        return Ok(None); // Wait for the rest of the fragment over TCP
    }

    let raw_frame = buf.split_to(total_len);
    Ok(Some(TgcpFrame { cmd, raw_frame }))
}

/// Packs an outgoing S2C Protobuf packet (RpcFrame) into the 25-byte TGCP header (0x4013)
pub fn encode_tgcp_s2c(s2c_seq: u32, rpc_frame_bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(25 + rpc_frame_bytes.len());
    out.put_u16(TGCP_MAGIC);
    out.put_u16(0x000b);
    out.put_u16(0x000c);
    out.put_u16(0x4013);
    // Frame number — u32 BE at [9..13] (as the client's C2S: 0xff → 0x100 gives `00 00 00 ff` → `00 00 01 00`).
    // Before 05.11 we wrote LE at [12..16]: up to 255 the bytes match, from frame 256 the client read number 0 and stopped
    // accepting pushes (replies still arrived) — no results window, a dead "Start" after search/hall (Sh4g/Sh4f).
    out.put_u8(0);                                 // [8]
    out.put_u32(s2c_seq);                          // [9..13]: Sequence (BE)
    out.extend_from_slice(&[0, 0, 0]);             // [13..16]
    out.put_u8(0x19);                              // [16]: Data Tag 0x19
    out.put_u32(rpc_frame_bytes.len() as u32);     // [17..21]: Body length (BE)
    out.put_u32(0x00000000);                       // [21..25]: Flag 0 = Plaintext
    out.extend_from_slice(rpc_frame_bytes);        // [25..]: Payload
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn auth_rsp_has_client_open_id() {
        let b = super::auth_rsp_2002("76561198000000902");
        assert_eq!(&b[6..8], &[0x20, 0x02]);
        assert_eq!(&b[28..46], b"76561198000000902\0");
        assert_eq!(&b[90 + 6..90 + 8], &[0x60, 0x02]);
        assert_eq!(b.len(), 90 + 21 + u32::from_be_bytes([b[90 + 17], b[90 + 18], b[90 + 19], b[90 + 20]]) as usize);
    }

    /// Frame number as the client's: BE u32 at [9..13]; up to 255 — the same bytes as before
    #[test]
    fn s2c_seq_big_endian_at_9() {
        let f = super::encode_tgcp_s2c(0xff, b"x");
        assert_eq!(&f[8..17], &[0, 0, 0, 0, 0xff, 0, 0, 0, 0x19]);
        let f = super::encode_tgcp_s2c(0x100, b"x");
        assert_eq!(&f[8..17], &[0, 0, 0, 1, 0, 0, 0, 0, 0x19]);
        let f = super::encode_tgcp_s2c(0x0102_0304, b"xy");
        assert_eq!(&f[9..13], &[1, 2, 3, 4]);
        assert_eq!(u32::from_be_bytes([f[17], f[18], f[19], f[20]]), 2);
        assert_eq!(f.len(), 25 + 2);
    }
}
