---
name: dungeonborne-launcher-support
description: "16.11 server fundraising bar in the launcher (manifest support.url → bot JSON); do not format the launcher with cargo fmt"
metadata:
  visibility: team
  node_type: memory
  type: project
---

**Server fundraising (owner decision 16.11).** "Support" in the launcher = financial support of the project, not tech support.
One source for Discord, the site and the launcher — the bot JSON (`/dbserver/support`, format: currency, goals[], links{}).
The launcher takes the address from the server manifest (`support: { url }`, owner's choice — do not hard-code, the domain is moving),
the request is made from Rust (`support.rs`, command `server_support`), it shows only goals[0] with kind "monthly" and the links
links.patreon / links.boosty over https. The bar is under the server card, colour — the project gold (#e0a84a / #f6cd7a, tokens
`patron*`, the same in all themes; it was purple at first, the Discord session corrected it). The cross hides it until the goal of
the next month (localStorage). Do not duplicate the button anywhere else; "Report a bug" is separate.
Branch `launcher-support` (804a884 + recolour); the backend must serve `support.url` — not yet.

**Formatting.** The launcher has no rustfmt.toml, the code is written with lines up to 120; `apps/launcher` is excluded from the root
Cargo workspace, `cargo xtask ci` does not format it. `cargo fmt` in the launcher (width 100) reformats every file —
do not run it. Related to [[dungeonborne-launcher-ui-style]], [[dungeonborne-bug-report]].
