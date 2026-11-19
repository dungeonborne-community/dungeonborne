// Sandbox for a plain browser (trunk serve): fake answers for the DevKit commands. Inside the app it does nothing.
(function () {
  if (window.__TAURI__) return;
  const items = [
    { level: "ok", rule: "home", path: "X:\\DBDev\\home", msg: "all 18 folders present" },
    { level: "warn", rule: "local", path: "local.toml [backup] host", msg: "placeholder from local.example.toml - set the real value" },
    { level: "error", rule: "tools", path: "tool tailwindcss", msg: "not found (tailwindcss) - standalone CLI v4 in PATH" },
    { level: "ok", rule: "tools", path: "tool rust", msg: "rustc 1.98.1" },
  ];
  const answers = {
    info: { root: "X:\\DBDev\\workspace", home: "X:\\DBDev\\home", vault: "X:\\DBDev\\vault", members: 21, branch: "main", head: "abc1234 demo", dirty: 0, origin: "https://github.com/dungeonborne-community/dungeonborne.git" },
    doctor: items,
    check: [],
    members: [{ path: "core/backend", name: "backend", kind: "service", depends_on: [], visibility: "public" }, { path: "content", name: "content", kind: "data", depends_on: [], visibility: "team" }, { path: "ops/deploy", name: "deploy", kind: "ops", depends_on: ["core/backend"], visibility: "owner" }],
    secrets_list: ["X:\\DBDev\\vault\\dungeonborne.kdbx", [{ name: "relay_tls_key", place: "Prod VM / Relay TLS key", kind: "file", target: "backend.key", env: "RELAY_TLS_KEY", used_by: ["core/backend"], description: "demo", required: true, rotate: true }]],
    secrets_check: [{ level: "ok", rule: "secrets", path: "relay_tls_key", msg: "Prod VM/Relay TLS key: attachment 'backend.key' present (246 bytes)" }],
    backup: "snapshot 75e85f73 (demo)",
    components: {
      sparse: true,
      components: [
        { name: "base", title: "Base: ws, xtask, DevKit, docs, agent rules", about: "Always installed.", base: true, members: ["tools/ws", "xtask", "apps/devkit"], needs: [], files: 40, installed: true, external: [] },
        { name: "server", title: "Server and game data", about: "Backend and the data it serves.", base: false, members: ["core/backend", "content"], needs: [], files: 779, installed: true, external: [] },
        { name: "studio", title: "Balance Studio", about: "The designer's app.", base: false, members: ["apps/studio"], needs: ["server"], files: 40, installed: false, external: [] },
        { name: "site", title: "Community site", about: "web/landing.", base: false, members: ["web/landing"], needs: ["server"], files: 61, installed: true, external: [] },
        { name: "mods", title: "Game mods", about: "UE4SS mods and dbnative.", base: false, members: ["game/ue4ss"], needs: ["server"], files: 89, installed: false, external: ["visual-studio"] },
        { name: "research", title: "Client research", about: "Notes, tools/py, rpc-evidence.", base: false, members: ["research"], needs: ["server"], files: 1181, installed: false, external: ["ghidra"] },
      ],
      presets: [
        { name: "full", title: "Everything", components: ["all"] },
        { name: "balance", title: "Balance designer: Studio and the tables", components: ["studio"] },
        { name: "web", title: "Site", components: ["site"] },
        { name: "re", title: "Client research", components: ["research", "mods"] },
      ],
    },
    components_set: [{ level: "warn", rule: "components", path: "plan", msg: "base, server, site -> base, server, studio; leaving the disk: web/landing. Run with --apply" }],
    me: { level: "dev", lang: "ru", detail: "normal", machine: "dev", claude: "" },
    set_me: null,
    start_agent: "X:\\DBDev\\home\\runs\\agents\\2026-11-18-1012-fine-sword-cyclops.task.md",
    open_preview: null,
    work: {
      previews: [
        { name: "site", member: "web/landing", url: "http://127.0.0.1:8821/", note: "the site from the files, as on the hosting (no cache)" },
        { name: "balance-studio-ui", member: "apps/studio", url: "http://127.0.0.1:1430/", note: "the studio UI in a browser with a mock of the Tauri API - not the real window" },
      ],
      branch: "feat/sword-drop",
      head: "ac9d809 balance: Fine sword drops more often from the Cyclops",
      changes: [" M content/balance/drops.toml", "?? content/balance/notes.md"],
      branches: [
        { name: "feat/sword-drop", when: "10 minutes ago", subject: "balance: Fine sword drops more often from the Cyclops" },
        { name: "feat/site-news", when: "40 minutes ago", subject: "site: news page for the new season" },
        { name: "main", when: "3 hours ago", subject: "Update team edition" },
      ],
      reports: [
        { file: "2026-11-18-1012-fine-sword-cyclops.md", title: "Fine sword drops more often from the Cyclops", pending: false,
          text: "Result: the Fine sword drops from the Cyclops 10.0 % (was 6.8 %).\nWhat changed: ...\nHow checked: cargo xtask balance verify - ok\nNot checked: in the game" },
        { file: "2026-11-18-1030-site-news.task.md", title: "A news page for the new season", pending: true, text: "# Task from DevKit ..." },
      ],
    },
    game: {
      installed: "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Dungeonborne",
      installed_ok: true,
      reference_note: "",
      references: [],
      steam_command: "download_depot 2448970 2448971 4035623128766163436",
      steam_download: "C:\\Steam\\steamapps\\content\\app_2448970\\depot_2448971",
      steam_download_ready: false,
      build: "16716489",
      dump_dir: "X:\\DBDev\\home\\dumps\\16716489",
      dump_files: 4087,
      dump_when: "3",
      sets: [
        { name: "icons", ok: 2072, skip: 65, error: 0 },
        { name: "monsters", ok: 1352, skip: 0, error: 0 },
        { name: "curves", ok: 67, skip: 0, error: 0 },
        { name: "heroes", ok: 596, skip: 0, error: 0 },
        { name: "abilities_common", ok: 0, skip: 0, error: 0 },
      ],
    },
    game_action: [{ level: "warn", rule: "dump", path: "plan", msg: "build 16716489: sets icons, monsters, curves, heroes -> X:\\DBDev\\home\\dumps\\16716489\\export. Run with --apply" }],
    vault_state: { unlocked: false, minutes_left: 0, idle_minutes_left: 0, names: [] },
    vault_unlock: { unlocked: true, minutes_left: 240, idle_minutes_left: 60, names: ["game_aes_key", "artifact_store"] },
    vault_lock: { unlocked: false, minutes_left: 0, idle_minutes_left: 0, names: [] },
    open: null,
    pick_root: "X:\\DBDev\\workspace",
  };
  window.__TAURI__ = { core: { invoke: (cmd) => new Promise((ok) => setTimeout(() => ok(answers[cmd]), 300)) } };
})();
