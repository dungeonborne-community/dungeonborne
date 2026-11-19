# Security

## Reporting a vulnerability
Please **do not open a public issue** for a vulnerability (account takeover, item duplication, remote code
execution through the launcher or mods, leaking other players' data). Use the hosting platform's private
vulnerability reporting for this repository. Describe the steps to reproduce and the impact; we answer as soon as we
can and credit you if you wish.

## What we consider in scope
- `core/backend` — authentication, sessions, the relay, admin API, trade and inventory integrity.
- `apps/launcher` — update and mod download verification (signatures, hashes, pinned certificates).
- `bootstrap.ps1`, `xtask`, `tools/ws` — anything that downloads or runs code without verification.

## For server operators
- Secrets never live in git: `config/secrets.toml` lists them, values stay in your KeePassXC database
  (`cargo ws secrets`).
- Generate **your own** keys: relay TLS, launcher release signing, `cargo xtask dist keygen`. Keys and pins found in
  this repository are placeholders or public halves only.
