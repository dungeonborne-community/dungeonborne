---
name: dungeonborne-launcher-ui-style
description: "Launcher UI work — plain HTML + Tailwind + JS only, no frameworks/component libraries; \"Fluent\" = Windows 11 look"
metadata:
  visibility: team
  node_type: memory
  type: feedback
---

For the launcher UI (and design experiments) use plain HTML + Tailwind v4 + plain JS, built with the Tailwind CLI —
no Svelte/React/Vite/component libraries (Fluent UI Web Components were rejected on 04.11). "Fluent" means the
Windows 11 visual language (Mica, NavigationView, SettingsCard, AccentButton, Segoe UI Variable / Segoe Fluent Icons
system fonts), implemented with Tailwind. A "design experiment" means a design from scratch, not a restyle of the
existing ui/ markup, and it is meant to test the designer kit (ui/README.md + sandbox ui/dev/mock-tauri.js).

**Why:** the user deliberately chose HTML+Tailwind so anyone can read/modify the UI and the exe stays verifiable.

**How to apply:** current UI `launcher/ui/` is the from-scratch Windows 11 design (applied 04.11); old designs kept in `launcher/UIs/` (classic = pre-1.3.0) (templates in
index.html, app.js only sets data-state; findings in its README). See [[dungeonborne-launcher-auth]].
