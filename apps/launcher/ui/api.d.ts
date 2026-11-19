/**
 * Contract between the launcher UI and its Rust side (src-tauri/src/lib.rs). Framework-agnostic: any UI that calls
 * `window.__TAURI__.core.invoke` / `window.__TAURI__.event.listen` (or `@tauri-apps/api`) with these names and shapes
 * works. Plain JS gets autocompletion with `// @ts-check` + `/// <reference path="api.d.ts" />`.
 *
 * Argument names are camelCase on this side (Tauri converts: Rust `confirm_native` = JS `confirmNative`).
 * Every command may reject with `ApiError`. Change this file together with lib.rs.
 */

// ------------------------------------------------------------------------------------------------ errors

/** Rejection of any command. `code` is stable: show `I18N[lang]["err." + code]`, then `detail` in brackets. */
export interface ApiError {
  code: string;
  /** technical detail (English, may be empty) */
  detail: string;
  /** https link the UI may offer as a button, e.g. Discord invite for `api.not_in_guild` */
  link?: string;
}

// ------------------------------------------------------------------------------------------------ data

export interface Settings {
  /** [game folder, ""]: one install since 1.6.1 (the folder that contains `Dungeonborne\Binaries\Win64`; "" = not set).
   *  The second entry stays for older versions and is always empty. */
  installs: [string, string];
  lang: "ru" | "en";
  dev_mode: boolean;
  run_as_admin: boolean;
  /** launcher stands in for the game's bootstrap (Steam "Play" opens it) */
  steam_bootstrap: boolean;
  /** server id -> key of the newest patch notes entry the player has opened */
  notes_seen: Record<string, string>;
  /** look of the launcher (input.css, [data-theme]); anything else is stored as "torch" */
  theme: "torch" | "crypt" | "blood";
  /** 1.7.2: through hosts even where the server needs none (relay.no_hosts); also set by the launcher on a failed start */
  force_hosts: boolean;
}

/** Public account data; the session token never reaches the UI. */
export interface Account {
  provider?: string | null;
  subject: string;
  display_name: string;
  /** https only */
  avatar_url?: string | null;
  /** 17 digits, SteamID64 format — what the game sees */
  open_id: string;
  nick: string;
  roles: string[];
  /** unix seconds */
  expires_at: number;
  logged_in_at: number;
}

export interface ModRef {
  id: string;
  version: string;
  sha256?: string | null;
  required: boolean;
  url?: string | null;
  size?: number | null;
}

/** `dbserver/1` manifest as the server sent it (validated). Texts come from the server: render as text, never HTML. */
/** `dbnotes/1`, newest update first; sections only "new" | "fixed" | "balance", items are plain text */
export interface Notes {
  latest: string;
  /** set when these are the notes of the server's active season */
  season?: { id: string; name: string } | null;
  /** lang: what this entry's text is in ("" from older servers); differs from the launcher's when untranslated */
  updates: { id: string; version: string; date: string; title: string; lang: string; sections: { kind: "new" | "fixed" | "balance"; items: string[] }[] }[];
}

export interface Manifest {
  schema: string;
  id: string;
  name: string;
  description: string;
  /** the description in other languages, e.g. {ru, en}; `description` is the server's main language */
  description_i18n?: Record<string, string> | null;
  region?: string | null;
  languages: string[];
  website?: string | null;
  contact?: string | null;
  game: { exe_sha256: string; label?: string | null };
  connect?: { address?: string | null; ca_cert?: string | null; ca_sha256?: string | null } | null;
  mode?: { id?: string | null; tags: string[] } | null;
  mods: ModRef[];
  policy: { allow_unlisted_client_mods?: boolean | null; accounts?: string | null };
  status_url?: string | null;
  key?: string | null;
  signature?: string | null;
  auth?: { providers: string[]; api: string } | null;
  /** no_hosts: the server's mod points the game at https://localhost — no hosts lines, INTL through the open config */
  relay?: { address: string; port: number; cert_sha256?: string | null; no_hosts?: boolean } | null;
  launcher?: { version: string; url: string; size: number; sha256: string; signature: string } | null;
  /** server patch notes; absent on servers that don't publish them */
  patch_notes?: { url: string; latest: string } | null;
  /** project support (16.11, support.rs): where this month's server goal and donation links are */
  support?: { url: string } | null;
}

export interface ServerView {
  url: string;
  manifest: Manifest;
  /** server key differs from the one seen first: login and Play are blocked until `accept_key` */
  key_changed: boolean;
  trusted_key?: string | null;
  account?: Account | null;
  /** account present, not expired, token stored */
  logged_in: boolean;
}

export interface StateView {
  version: string;
  settings: Settings;
  servers: ServerView[];
  hosts_installed: boolean;
  hosts_path: string;
  /** install slot this launcher was started for from Steam (in place of the game's bootstrap) */
  boot_install?: number | null;
}

export interface Status {
  online?: number | null;
  capacity?: number | null;
  backend?: string | null;
  modset_sha256?: string | null;
  motd?: string | null;
  /** `motd` in other languages (also the maintenance text) */
  motd_i18n?: Record<string, string> | null;
  /** measured by the launcher */
  ping_ms?: number | null;
}

export interface InstallInfo {
  exe_sha256: string;
  /** the game of this install is running */
  running: boolean;
  /** a profile of a crashed start is still laid out (will be undone before the next start) */
  dirty: boolean;
}

export interface ModView {
  id: string;
  version: string;
  required: boolean;
  cached: boolean;
  downloadable: boolean;
}

export interface CacheInfo {
  bytes: number;
  packages: number;
}

export interface UpdateInfo {
  version: string;
  current: string;
}

export interface BugReportInfo {
  /** log tails that would go, sizes after the limits (all together up to 9 MB) */
  files: { name: string; size: number }[];
  /** the last game run looks like a crash: preselect "crash" */
  crash: boolean;
  /** logged in on this server: the report goes with the player's session */
  logged_in: boolean;
  title_max: number;
  description_max: number;
}

export type BootstrapRoot =
  | { kind: "original" }
  | { kind: "launcher"; version: string }
  | { kind: "other" }
  | { kind: "missing" };

// ------------------------------------------------------------------------------------------------ commands

/** name -> [arguments, result] */
export interface Commands {
  get_state: [{}, StateView];
  save_settings: [{ settings: Settings }, void];
  /** `index` = install slot 0 | 1 */
  check_install: [{ index: number }, InstallInfo];
  /** the game folder on this PC (a running game, else Steam's libraries), null — not found */
  find_game: [{}, string | null];
  /** "Browse…": the system folder picker; null — closed; a folder without the game — error install_invalid */
  pick_game_folder: [{ title: string }, string | null];
  /** undo profiles of a crashed launcher; call once on start; returns messages for the log */
  restore_pending: [{}, string[]];
  /** manifest URL or a `dbserverlist/1` URL; returns ids added/updated */
  add_server: [{ url: string }, string[]];
  refresh_server: [{ id: string }, void];
  remove_server: [{ id: string }, void];
  accept_key: [{ id: string }, void];
  server_status: [{ id: string }, Status];
  /** battle UDP tunnel of this account; old servers fail with http_status (no endpoint) */
  net_state: [{ id: string }, { udp_tunnel: boolean; capable: boolean }];
  /** reason goes to the server log: the launcher's finding, or "settings" */
  set_udp_tunnel: [{ id: string; on: boolean; reason: string }, { udp_tunnel: boolean; capable: boolean }];
  /** plain text for support: what the network check saw, launcher version, last relay.log lines (no tokens) */
  net_report_text: [{ id: string }, string];
  /** what the "Report a bug" form needs before it opens */
  bug_report_info: [{ id: string }, BugReportInfo];
  /** sends the report (contract docs/specs/bug-report-api.md); returns its id.
   *  Errors: `login_required`, `bug_title_required`, `api.rate_limited` (detail = seconds to wait), network ones */
  /** `source`: "game" when the form came from the game's "Support" button (event `bug_request`), else "launcher" */
  bug_report: [{ id: string; kind: "crash" | "game" | "launcher"; title: string; description: string; source?: "launcher" | "game" }, string];
  /** `posted` once the bot made the Discord post; `url` — https link to it */
  bug_report_status: [{ id: string; report: string }, { status: "new" | "posted"; url: string | null }];
  /** null — the server doesn't publish patch notes */
  server_notes: [{ id: string }, Notes | null];
  /** null — nothing to show: no support source, no monthly goal */
  server_support: [{ id: string }, Support | null];
  /** the player opened the notes up to `latest` */
  notes_seen: [{ id: string; latest: string }, void];
  server_mods: [{ id: string }, ModView[]];
  mod_cache_info: [{}, CacheInfo];
  clear_mod_cache: [{}, void];
  /** newest signed launcher build any server offers, or null */
  update_check: [{}, UpdateInfo | null];
  /** downloads, swaps the exe and restarts the launcher (never resolves on success) */
  update_apply: [{}, void];
  /** per install slot; null = slot empty / not an install */
  bootstrap_status: [{}, (BootstrapRoot | null)[]];
  /** saves `steam_bootstrap` and applies it to every install; returns log lines */
  set_bootstrap: [{ enable: boolean }, string[]];
  /** https (or loopback http) in the system browser */
  open_link: [{ url: string }, void];
  /** lobby of game instances; `enabled: false` = the server has no lobby. Errors: api.instance_battle, api.hall_full, … */
  lobby_state: [{ id: string }, LobbyView];
  lobby_join: [{ id: string; instance: number }, LobbyView];
  /** vote_size: partySize 1 | 2 | 3; vote_map: levelId from `maps`. fill: levelId through gather, then api.vote_over */
  lobby_vote: [{ id: string; partySize?: number; levelId?: number }, LobbyView];
  lobby_leave: [{ id: string }, LobbyView];
  /** starts login in the browser; progress comes as `login` events */
  login: [{ id: string }, void];
  cancel_login: [{ id: string }, void];
  logout: [{ id: string }, void];
  /** UAC prompt (yellow: unsigned) — explain it before calling; returns hosts state after */
  hosts_install: [{}, boolean];
  hosts_uninstall: [{}, boolean];
  /**
   * Whole start: check -> mods -> profile -> relay -> game -> cleanup; resolves when the game has exited.
   * Steps come as `play` events. Rejects with `native_confirm` (detail = comma-separated mod ids): ask the player,
   * then call again with those ids in `confirmNative`.
   */
  play: [{ id: string; install: number; optional: string[]; confirmNative: string[] }, void];
}

// ------------------------------------------------------------------------------------------------ events

export type PlayStep = "check" | "mods" | "profile" | "relay" | "game" | "cleanup" | "done";
export type PlayState = "start" | "progress" | "running" | "done" | "warn" | "error";

/** name -> payload */
export interface Events {
  /** log line for the journal (`scope`: play1/play2, login, relay, mods, servers, bundle, steam, update…) */
  log: { scope: string; level: "info" | "warn" | "error"; text: string };
  /** step of `play`; step "done" ends it (`state` "done" or "error", error text in `text`) */
  /** `error` — only with step "done", state "error": the failure as a code to translate (`text` is the raw log line) */
  play: { install: number; step: PlayStep; state: PlayState; text: string; error?: ApiError };
  /** one finished relay connection (no secrets) */
  relay: { ts: number; port: number; up: number; down: number; ms: number; reason: string };
  /** login progress: "browser" (waiting; show `user_code`), then "ok" or "error" */
  login: { server_id: string; state: "browser" | "ok" | "error"; error?: ApiError; user_code?: string };
  /** session died during play (expired / banned): show the login screen with `error` */
  session: { server_id: string; error: ApiError };
  /** manifests refreshed or a built-in server added: reload state, mods, update_check */
  servers_changed: null;
  /** the game's "Support" button (UnLua pack, file `Saved\bug_report_request.json`): open the report form, source "game";
   *  the launcher window is already brought to the front */
  bug_request: { server_id: string; scene: string; role: string };
  /** the game started by the launcher exited and looks crashed: offer a report (sent only by the player) */
  game_crashed: { server_id: string; exit_code: number };
  /** download progress of one mod package (`total` from the manifest when the server does not tell) */
  mod_progress: { install: number; id: string; done: number; total?: number | null };
  update_progress: { done: number; total?: number | null };
  /** the provider cuts the connection to the server (netblock.rs): recommend WARP / a VPN. kind: reset — TLS reset after
   *  the ClientHello, freeze — a download stopped at a few KB, stall — repeated ~20 s relay stalls. tunnel: also offer
   *  the battle tunnel (`detail` goes back with set_udp_tunnel). Sent again on every new finding. */
  /** the server's relay refused this launcher version (backend `[relay] min_client`): show it, offer the update */
  launcher_outdated: string;
  net_block: { server_id: string; kind: "reset" | "freeze" | "stall"; detail: string; tunnel: boolean };
}

// ------------------------------------------------------------------------------------------------ globals

declare global {
  interface Window {
    __TAURI__: {
      core: {
        invoke<K extends keyof Commands>(cmd: K, args?: Commands[K][0]): Promise<Commands[K][1]>;
      };
      event: {
        listen<K extends keyof Events>(event: K, handler: (ev: { payload: Events[K] }) => void): Promise<() => void>;
      };
    };
    /** i18n.js: lang -> key -> text with {placeholders} */
    I18N: Record<"ru" | "en", Record<string, string>>;
  }
}

/** Lobby of game instances (LAUNCHER_SPEC section 12) */
export interface LobbyInstance {
  id: number;
  /** "vote" — an ordinary tab (votes for the party size and the map); "clotho" — Trial of Clotho (id 100, backend
   *  51bc865): a recruitment up to `min_players` for a 3v3 arena match, no votes, phases idle|gather|starting|battle.
   *  Older servers send no kind: an ordinary tab. "fill" (backend 11.11) — an instance of one party size from the start
   *  (solo up to 3 at once, duo 2, trio 1, battles included): no size vote, ids 1…99 go round, a started one gets a new
   *  one of its size beside it at once, a finished battle leaves the list; sorted by (party_size, id). Phases
   *  idle → gather (no minimum, `ends_at` = start deadline, start when `players >= seats` or by the timer) → starting →
   *  waitroom → battle. The map vote is open all through gather (a vote can be changed). */
  kind?: "vote" | "clotho" | "fill";
  /** fill, gather: the leading map of the vote (the room's seats come from it), null — no votes yet */
  leader?: number | null;
  /** fill, solo lanes (backend 13.11, LOBBY_SOLO_LANES): 1|2|3 — a solo instance of one fixed difficulty, its map is set
   *  from the start (`level_id`, alternating dungeon/castle) and there is no map vote; 0 or absent — an ordinary tab */
  lane?: number;
  /** solo lane: the instance's difficulty; null on other tabs */
  difficulty?: "easy" | "classic" | "hard" | null;
  /** "closed" (backend 13.11): the tab is not open yet, `ends_at` = when it opens (tabs open one by one at start) */
  phase: "closed" | "idle" | "gather" | "vote_size" | "vote_map" | "starting" | "waitroom" | "battle";
  /** players needed (the vote starts / the Clotho match starts) */
  min_players?: number;
  /** Clotho: the arena map (map_detail id) */
  arena_map_id?: number;
  /** unix seconds: end of the vote, or end of the window for pressing Start in the game; 0 = no timer */
  ends_at: number;
  players: number;
  /** already pressed Start and are in the waiting room */
  in_hall: number;
  /** 0 until the map is chosen. fill: in gather — how many start it by filling (the leading map's room), after — the room's */
  seats: number;
  /** 0 until chosen */
  party_size: number;
  level_id: number;
  votes: { sizes: Record<string, number>; maps: Record<string, number> };
}

export interface LobbyView {
  enabled: boolean;
  /** text of the maintenance banner; new battles are closed while set */
  maintenance?: string | null;
  /** the maintenance text in other languages, e.g. {ru, en} */
  maintenance_i18n?: Record<string, string> | null;
  now?: number;
  maps?: number[];
  sizes?: number[];
  /** fill: the gathering timer, seconds (480) */
  fill_secs?: number;
  instances?: LobbyInstance[];
  me?: {
    instance: number | null;
    size_vote: number | null;
    map_vote: number | null;
    game_online: boolean;
    /** the player's Trial of Clotho run; status 0 — none, 1 — on, 2 — reward waiting; wins of 7; tokens — losses left of 3 */
    trial?: { activity_id: number; status: 0 | 1 | 2; wins: number; fails: number; tokens: number } | null;
  };
}

/** Project support (support.rs): this month's server goal and the donation links (patreon, boosty; https only) */
export interface Support {
  currency: string;
  goal: { id: string; kind: string; title: Record<string, string>; raised: number; target: number; ends_at: number };
  links: [string, string][];
}
