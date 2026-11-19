---
name: dungeonborne-workarounds
description: "Deliberate simplifications (\"workarounds\") live in backups/docs/WORKAROUNDS.md, separate from bugs in FIX_LIST.md"
metadata:
  visibility: team
  node_type: memory
  type: project
---

Since 2026-11-02 the user wants accepted simplifications recorded as "workarounds" in `<repo>\docs\WORKAROUNDS.md`
(table K1…: what, why, risk, when to revisit), not left as open bugs. Bugs stay in `docs/FIX_LIST.md`.

**Why:** user said "let it stay simple, just record it in the workarounds" about instant crafting (PVE reference behaviour).

**How to apply:** when a fix is "good enough" or follows PVE without an official reference, add a K-row there and mark
the FIX_LIST item done with a pointer to it. See [[dungeonborne-handlers-tooling]].
