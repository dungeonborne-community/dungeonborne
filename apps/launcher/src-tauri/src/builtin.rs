//! Built-in community server: added on first start; its https and relay certificate is self-signed, so it is
//! pinned here by sha256 of the DER. Game servers (DS) are handed out by the backend.

/// Manifest URLs added automatically (once; the player may remove them). Same server: the later URL wins (`upsert`
/// by manifest id), so the domain, listed last, replaces the IP once it answers. Some RU networks freeze TLS without SNI
/// after ~16 KB (IP URLs send none); the IP stays as a fallback if the name does not resolve.
/// Moved to dungeonborne-community.com: an entry stored under the old name gets this URL by the same upsert, because the
/// new name was never in `builtin_added.json`. The old name stays in DNS until older launchers have updated.
pub const SERVERS: &[&str] = &["https://203.0.113.10/.well-known/dbserver.json", "https://dbserver.example.org/.well-known/dbserver.json"];

/// Leaf certificates accepted without a CA chain (`deploy/tls/backend.crt`). A pinned leaf is accepted for any host name,
/// so the same certificate serves the IP, the old and the new domain.
pub const CERT_SHA256: &[&str] = &["0000000000000000000000000000000000000000000000000000000000000000"];

/// Public half of the release key (ed25519): launcher updates must be signed with it (`tools/release.py`,
/// secret in `<data>\deploy\release\`). Changing it locks out launchers built with the old one.
pub const RELEASE_KEY: &str = "50e0fdf7e7ef98701d9cd363b7813774ce23b11eb4c426c2372ee63aa71588b0";
