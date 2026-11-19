use aes::cipher::{BlockDecryptMut, KeyIvInit};

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// Fixed TSF4G / TAES initialization vector (0x00..0x0F)
pub const TAES_IV: [u8; 16] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
    0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
];

/// AES-128 session key that the server passes to the client in packet 0x1002 (BinBingo)
pub const SERVER_SESSION_KEY: [u8; 16] = [
    0x10, 0xf0, 0x04, 0x7c, 0xb7, 0x49, 0x27, 0x6f,
    0xdb, 0xbc, 0x43, 0x02, 0x1d, 0x58, 0x8b, 0xfc,
];

/// Decrypts an incoming C2S buffer (after the 25-byte 0x4013 header)
/// and removes the 4-byte prefix and the TSF4G padding tail ("tsf4g" + pad_len).
pub fn taes_decrypt_c2s(ciphertext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>, &'static str> {
    if ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
        return Err("Ciphertext length must be a non-zero multiple of 16");
    }

    let mut buf = ciphertext.to_vec();
    Aes128CbcDec::new(key.into(), &TAES_IV.into())
        .decrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf)
        .map_err(|_| "AES-128-CBC decrypt error")?;

    let pad_len = *buf.last().unwrap() as usize;
    if pad_len == 0 || pad_len > 32 || buf.len() <= 4 + pad_len {
        return Err("Invalid TSF4G padding length");
    }

    Ok(buf[4..(buf.len() - pad_len)].to_vec())
}