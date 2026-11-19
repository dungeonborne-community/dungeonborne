//! Self-update. Servers offer a launcher build in their manifest (`launcher`); it is taken only when it is newer than
//! this one and signed with the release key built into the exe (`builtin::RELEASE_KEY`, `tools/release.py`), so a
//! server can host updates but cannot make them. The new exe is downloaded next to the running one; the running one is
//! renamed to `<name>.old.exe` (Windows allows renaming a running image), the new one takes its name and is started.
//! The `.old.exe` is removed on the next start.

use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use reqwest::Client;

use crate::error::{bail, Error, Result};
use crate::manifest::LauncherRelease;
use crate::net::{self, check_url, is_sha256_hex};

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
const LIMIT: u64 = 256 << 20;

fn message(r: &LauncherRelease) -> String {
    format!("dbl-update/1\n{}\n{}\n{}", r.version, r.sha256.to_ascii_lowercase(), r.size)
}

/// Signature check against a public key (hex, 32 bytes).
pub fn verify_with(r: &LauncherRelease, public_hex: &str) -> Result<()> {
    let mut pk = [0u8; 32];
    hex::decode_to_slice(public_hex, &mut pk).map_err(|e| Error::new("internal", format!("release key: {e}")))?;
    let key = VerifyingKey::from_bytes(&pk).map_err(|e| Error::new("internal", format!("release key: {e}")))?;
    let mut sig = [0u8; 64];
    hex::decode_to_slice(r.signature.trim(), &mut sig).map_err(|_| Error::new("update_signature", "not hex"))?;
    key.verify(message(r).as_bytes(), &Signature::from_bytes(&sig))
        .map_err(|_| Error::new("update_signature", r.version.clone()))
}

/// A release worth installing: well-formed, newer than `current`, signed with `public_hex`.
pub fn acceptable(r: &LauncherRelease, current: &str, public_hex: &str) -> Result<semver::Version> {
    let v = semver::Version::parse(&r.version).map_err(|e| Error::new("update_invalid", e.to_string()))?;
    let cur = semver::Version::parse(current).map_err(|e| Error::new("internal", e.to_string()))?;
    if v <= cur {
        bail!("update_not_newer", "{v}");
    }
    if !is_sha256_hex(&r.sha256) || r.size == 0 || r.size > LIMIT {
        bail!("update_invalid", "sha256/size");
    }
    check_url(&r.url)?;
    verify_with(r, public_hex)?;
    Ok(v)
}

/// The newest acceptable release among all offers.
pub fn best<'a>(offers: impl IntoIterator<Item = &'a LauncherRelease>) -> Option<LauncherRelease> {
    offers
        .into_iter()
        .filter_map(|r| acceptable(r, CURRENT, crate::builtin::RELEASE_KEY).ok().map(|v| (v, r)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, r)| r.clone())
}

fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let stem = exe.file_stem().unwrap_or_default().to_string_lossy();
    exe.with_file_name(format!("{stem}.{suffix}"))
}

/// Leftovers of a previous update (`.old.exe`, half-downloaded `.new.exe`).
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        for s in ["old.exe", "new.exe", "new.part"] {
            let _ = std::fs::remove_file(sibling(&exe, s));
        }
    }
}

/// Downloads, verifies and swaps the exe. Returns the path to start; the caller exits right after starting it.
pub async fn install(client: &Client, r: &LauncherRelease, progress: impl FnMut(u64, Option<u64>)) -> Result<PathBuf> {
    install_at(client, r, &std::env::current_exe()?, CURRENT, crate::builtin::RELEASE_KEY, progress).await
}

/// `install` for a given exe path, version and key (tests swap a temporary file).
pub async fn install_at(
    client: &Client,
    r: &LauncherRelease,
    exe: &Path,
    current: &str,
    public_hex: &str,
    progress: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf> {
    acceptable(r, current, public_hex)?;
    let exe = exe.to_path_buf();
    let new = sibling(&exe, "new.exe");
    let old = sibling(&exe, "old.exe");
    let (sha, size) = net::download_to(client, &check_url(&r.url)?, &new, r.size, progress)
        .await
        .map_err(|e| if e.code == "io" { Error::new("update_no_write", e.detail) } else { e })?;
    if size != r.size || sha != r.sha256.to_ascii_lowercase() {
        let _ = std::fs::remove_file(&new);
        bail!("update_invalid", "downloaded file differs from the signed release");
    }
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| Error::new("update_no_write", e.to_string()))?;
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe); // put the running build back
        return Err(Error::new("update_no_write", e.to_string()));
    }
    Ok(exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed(version: &str, key: &SigningKey) -> LauncherRelease {
        let mut r = LauncherRelease {
            version: version.into(),
            url: "https://example.org/dungeonborne-launcher.exe".into(),
            size: 1234,
            sha256: "ab".repeat(32),
            signature: String::new(),
        };
        r.signature = hex::encode(key.sign(message(&r).as_bytes()).to_bytes());
        r
    }

    #[test]
    fn accepts_only_newer_signed_builds() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let public = hex::encode(key.verifying_key().to_bytes());
        let other = SigningKey::from_bytes(&[8u8; 32]);

        assert!(acceptable(&signed("1.2.0", &key), "1.1.1", &public).is_ok());
        assert_eq!(acceptable(&signed("1.1.1", &key), "1.1.1", &public).unwrap_err().code, "update_not_newer");
        assert_eq!(acceptable(&signed("1.0.0", &key), "1.1.1", &public).unwrap_err().code, "update_not_newer");
        assert_eq!(acceptable(&signed("1.2.0", &other), "1.1.1", &public).unwrap_err().code, "update_signature");

        // any field changed after signing breaks the signature
        let mut r = signed("1.2.0", &key);
        r.sha256 = "cd".repeat(32);
        assert_eq!(acceptable(&r, "1.1.1", &public).unwrap_err().code, "update_signature");
        let mut r = signed("1.2.0", &key);
        r.version = "9.9.9".into();
        assert_eq!(acceptable(&r, "1.1.1", &public).unwrap_err().code, "update_signature");
    }

    /// Same message and key format as tools/release.py (Python `cryptography`).
    #[test]
    fn builtin_key_is_valid() {
        let mut pk = [0u8; 32];
        hex::decode_to_slice(crate::builtin::RELEASE_KEY, &mut pk).unwrap();
        VerifyingKey::from_bytes(&pk).unwrap();
    }
}
