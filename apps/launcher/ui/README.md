# Launcher UI — how to propose your own design

The Dungeonborne launcher UI is an ordinary web page: HTML, Tailwind CSS v4 and plain JavaScript, no bundlers or
frameworks. Inside the launcher it is shown by WebView2 (Tauri 2); everything else (login, mods, relay, game launch) is done by
the Rust part. You can propose a whole design of your own: different layout, colours, fonts, arrangement — as long as
all the states work and the rules below are followed.

## Files

| File | What it is |
| --- | --- |
| `index.html` | all the markup: window, navigation, server page, settings, dialogs; repeated pieces — `<template>` |
| `app.js` | logic: launcher calls and events; clones templates, fills in text, sets `data-state` / `data-tone` |
| `i18n.js` | all texts, Russian and English (`window.I18N`) |
| `input.css` | Tailwind: the Windows 11 theme (colours, Segoe fonts) and controls (AccentButton, ComboBox, ToggleSwitch, InfoBar…) |
| `api.d.ts` | **the contract with the launcher**: commands, their arguments and answers, events, data shapes, error codes |
| `dev/mock-tauri.js` | sandbox: stands in for the launcher in an ordinary browser (does nothing inside the launcher itself) |
| `styles.css` | built from `input.css`, do not edit by hand |

## Quick start without Rust or the game

All you need is [Tailwind CLI v4](https://tailwindcss.com/blog/standalone-cli) (one exe) and a browser.

```
cd ui
tailwindcss -i input.css -o styles.css --watch       # rebuilds the CSS on every edit
python -m http.server 8811                           # or any static server; index.html can also be opened from disk
```

Open `http://127.0.0.1:8811/index.html` — this is the launcher with made-up data. The "UI sandbox" panel at the bottom left:

| Scenario (`?scenario=`) | What you see |
| --- | --- |
| `ready` — Ready to play | two servers (one with a changed key and no connection), an account, mods, the update banner, launch from Steam |
| `fresh` — First launch | no servers, no game paths, hosts not set up |
| `login` — Not logged in → login | "Log in" → waiting for the browser with a code → logged in |
| `guild` — Login: not in the guild | refusal with an invite button |
| `native` — Native code question | "Play" → confirmation window |
| `port` — Port busy | "Play" → error at the "Relay" step |
| `running` — Game already running | install states "game running" / "profile not cleaned up" |
| `dev` — Developer mode | the developer mode label and hint |

Panel buttons: "End the game" (after "Play"), "Session lost (ban)", "Offer an update", "Log line".
The panel collapses by clicking its title and moves to another corner with the ⇄ button — so it does not cover your design.
For screenshots without the panel: `?scenario=ready&panel=hidden`.
The language is switched with the EN/RU button in the interface itself.

With Rust: `trunk serve` in `ui/` (the same with auto-reload) or `cargo tauri dev` in `src-tauri/` (the real launcher).

## States that must stay

The design can look however you like, but the player must see and be able to do all of this:

- **Server list**: name, online/ping or "no answer", tags, a "server key changed" badge; add a server by link; refresh.
- **Server card**: description and message of the day (server texts — as text only), website, refresh the manifest, remove the server.
- **Server key change**: a noticeable warning, the old and new key, a "Trust the new key" button; until it, login and "Play" are unavailable.
- **Account**: log in; waiting for the browser with a code and "Cancel"; a refusal with the reason and, if there is a `link`, a "Join the server" button + "Log in again"; logged in — name, avatar, in-game SteamID, "Log out".
- **Mods**: required and optional (checkboxes), "cached", "no download link".
- **Installs**: path, "exe matches" / "does not match" / "game running" / "profile not cleaned up" / "server did not specify the version".
- **Play**: a button per install; highlighting of the install the launcher was opened for from Steam (`boot_install`).
- **Launch progress**: steps "Check → Mods → Profile → Relay → Game → Cleanup" with states (running / done / warning / error), a mod download bar, the error text.
- **Confirmations**: native code (list of mods), removing a server, Windows permission before setting up hosts.
- **Launcher update banner**: version, "Update and restart", progress.
- **Log**: tabs "Launch / Relay / All", clearing, copying the text.
- **Settings**: two installs, "as administrator", developer mode, launch from Steam (with install states), hosts (set up / remove), mod cache (size, clear), version.
- **Header**: "hosts set up / not set up", the developer mode label, language, settings.
- **Pop-up messages** about errors and successes.

Minimum window size — 920×600, normal — 1120×740.

## Tips (from experience)

- The browser caches `app.js` and `dev/mock-tauri.js`: if you do not see your edits, reload the page with **Ctrl+F5**.
- The launcher replaces the whole text of an element with `data-t`: if a button has an icon, put `data-t` on an inner `<span>`.
- `hover:` overrides `disabled:` under the cursor — for buttons write `enabled:hover:` and `enabled:active:`.
- Icons without files: the system font `Segoe Fluent Icons` (Windows 11) / `Segoe MDL2 Assets` (Windows 10),
  e.g. `<span class="ico">&#xE713;</span>`, where `.ico` has the `font-family` of these fonts.
- A handy technique: keep the markup in `index.html` (`<template>`), and from JS only set `data-state` and style via
  `data-[state=…]:` — that is how the current interface (Windows 11) is built. Past designs are in `UIs/` (they can also
  be opened in the sandbox there).

## Rules

1. **Nothing from outside.** The launcher has a CSP: `default-src 'self'`, images — `'self' https: data:`. Fonts,
   icons, scripts and styles — only as files in `ui/`, no CDNs or Google Fonts. A font — put the file next to it.
2. **No inline styles or inline scripts.** The `style="…"` attribute, the `<style>` tag and `onclick="…"` in HTML will not work.
   Styles — Tailwind classes or CSS in `input.css`; dynamic values (bar width) — via `element.style.width = …` from JS.
3. **Server texts — as text only** (`textContent`, not `innerHTML`): name, description, MOTD, nick come from other people's
   servers.
4. **All strings — in `i18n.js`, in both languages.** Errors are shown by code: `err.<code>` (the list of codes is in `i18n.js`,
   the meaning — in `api.d.ts`).
5. **New files with Tailwind classes** — add them to `@source` in `input.css`, otherwise the classes do not get into `styles.css`.
6. **Do not change the contract from the UI side.** Need a new command or field — write to us, it is a change to the Rust part.
7. Accessibility: everything can be clicked with the mouse and used from the keyboard, text contrast is readable, states differ not only by colour.

## What about React / Vue / Svelte?

Possible: the launcher does not care what built the page. It needs:

- the result to be **static files** (index.html + js/css) — no server-side rendering;
- calls to go through `window.__TAURI__.core.invoke` / `window.__TAURI__.event.listen` (or the `@tauri-apps/api` v2 package)
  with the names and shapes from `api.d.ts` — for TypeScript it can be included as is;
- the CSP to be respected: CSS-in-JS that inserts `<style>` on the fly (styled-components, emotion) will not work — CSS has to be
  a file (Tailwind, CSS modules, `<style>` in Svelte/Vue compiled into a file are fine);
- the sandbox to work there too: include `dev/mock-tauri.js` as the first script.

But keep in mind: the launcher is currently built without Node.js (trunk + Tailwind CLI). A framework-based design we will either
port to plain JS or change the build — we decide that together, better ask before starting.

## How to submit

- The whole `ui/` folder (zip) or a pull request, plus screenshots of all sandbox scenarios (RU and EN).
- By submitting a design you agree that it is distributed under the launcher's licence (MIT).
- We review everything: the UI code runs inside the launcher next to login and game launch, so there must be no extra network
  requests, trackers or obfuscated code.

## For us (sandbox maintenance)

A new command or event in `src-tauri/src/lib.rs` → update `api.d.ts` and `dev/mock-tauri.js` (the command in `commands`,
if needed — a scenario), otherwise the sandbox drifts away from the launcher.
