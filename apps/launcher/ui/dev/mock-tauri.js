"use strict";
// Sandbox for UI work: stands in for window.__TAURI__ when the page is opened in a plain browser, so the whole UI can
// be designed without Rust, Tauri or the game. Inside the launcher window.__TAURI__ already exists and this file does
// nothing. The contract it imitates: ../api.d.ts. A small panel (bottom left) switches scenarios and fires events.

(() => {
  if (window.__TAURI__) return;

  const EXE = "62d509a00d984696e021fb5ddba9f87fd1b27c84f695877345ff5fe9401f58b3";
  const listeners = {};
  const emit = (name, payload = null) => (listeners[name] || []).forEach((h) => h({ payload }));
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const fail = (code, detail = "", link) => Object.assign({ code, detail }, link ? { link } : {});
  const now = () => Math.floor(Date.now() / 1000);

  // ------------------------------------------------------------------------------------------ lobby (spec section 12)

  const lobbyMock = (() => {
    const MAPS = [40003015, 40003017, 40003019, 40003016, 40003018, 40003020];
    const blank = (id) => ({ id, phase: "idle", ends_at: 0, players: 0, in_hall: 0, seats: 0, party_size: 0, level_id: 0, votes: { sizes: {}, maps: {} } });
    const inst = [blank(1), blank(2), blank(3), blank(4)];
    const MIN = 8; // lobby_state.min_players: voting starts when a tab has this many players (08.11)
    // lobby_state.rules: Hard — skirmish and the epic limit, the rest — strictly by size and "challenge mode"
    const RULES = Object.fromEntries(MAPS.map((id) => {
      const hard = id === 40003019 || id === 40003020;
      return [id, { skirmish: hard, gear: hard ? "epic" : null, challenger: !hard }];
    }));
    Object.assign(inst[0], { phase: "gather", players: 5 });
    Object.assign(inst[1], { phase: "vote_map", ends_at: now() + 25, players: 6, party_size: 3, votes: { sizes: { 3: 4, 1: 2 }, maps: { 40003016: 2, 40003019: 1 } } });
    Object.assign(inst[2], { phase: "waitroom", ends_at: now() + 240, players: 11, in_hall: 7, seats: 18, party_size: 3, level_id: 40003017 });
    Object.assign(inst[3], { phase: "battle", players: 0, in_hall: 16, seats: 18, party_size: 1, level_id: 40003020 });
    // Trial of Clotho tab (backend 51bc865): ?trial=0|1|2|none — no run / run on / reward waiting / unknown
    const clotho = { id: 100, kind: "clotho", phase: "gather", min_players: 6, players: 2, in_hall: 0, seats: 6, party_size: 3, level_id: 50000006, arena_map_id: 30102, ends_at: 0, votes: { sizes: {}, maps: {} } };
    const trialArg = new URLSearchParams(location.search).get("trial") ?? "1";
    const trial = trialArg === "none" ? null : { activity_id: 16600, status: Number(trialArg), wins: trialArg === "0" ? 0 : 3, fails: 1, tokens: trialArg === "0" ? 3 : 2 };
    const me = { instance: null, size_vote: null, map_vote: null, game_online: false, trial };
    const refused = (code) => fail(`api.${code}`);
    const step = () => {
      for (const i of inst) {
        if (i.ends_at && i.ends_at > now()) continue;
        if (i.phase === "vote_size") Object.assign(i, { phase: "vote_map", ends_at: now() + 20, party_size: me.size_vote ?? 3 });
        else if (i.phase === "vote_map") Object.assign(i, { phase: "starting", ends_at: 0, level_id: me.map_vote ?? 40003016, seats: 18 });
        else if (i.phase === "starting") Object.assign(i, { phase: "waitroom", ends_at: now() + 275 });
      }
    };
    setInterval(step, 4000);
    return {
      // ?maint=<text> in the URL shows the maintenance banner (with an English translation, as the server sends it)
      view: () => ({ enabled: true, maintenance: new URLSearchParams(location.search).get("maint"),
        maintenance_i18n: new URLSearchParams(location.search).get("maint") ? { ru: new URLSearchParams(location.search).get("maint"), en: "Maintenance: server update. Back soon." } : null, now: now(), maps: MAPS, sizes: [1, 2, 3], min_players: MIN, rules: RULES, instances: [...inst.map((i) => ({ ...i, kind: "vote", min_players: MIN })), { ...clotho }], me }),
      join(n) {
        if (n === 100) {
          if (clotho.phase === "battle" || clotho.phase === "starting") throw refused("instance_battle");
          if (me.trial?.status === 2) throw refused("trial_reward");
          if (!me.trial || me.trial.status === 0) throw refused("trial_not_open");
          if (me.instance && me.instance !== 100) inst[me.instance - 1].players--;
          me.instance = 100;
          clotho.players++;
          if (clotho.players >= clotho.min_players) Object.assign(clotho, { phase: "battle", in_hall: clotho.players });
          return this.view();
        }
        const i = inst[n - 1];
        if (i.phase === "battle") throw refused("instance_battle");
        if (me.instance === 100) clotho.players--;
        else if (me.instance) inst[me.instance - 1].players--;
        me.instance = n;
        me.size_vote = me.map_vote = null;
        i.players++;
        if (i.phase === "idle" || i.phase === "gather") Object.assign(i, i.players >= MIN ? { phase: "vote_size", ends_at: now() + 20 } : { phase: "gather" });
        return this.view();
      },
      vote(size, level) {
        const i = inst[me.instance - 1];
        if (size) {
          if (me.size_vote) i.votes.sizes[me.size_vote]--;
          me.size_vote = size;
          i.votes.sizes[size] = (i.votes.sizes[size] || 0) + 1;
        }
        if (level) {
          if (me.map_vote) i.votes.maps[me.map_vote]--;
          me.map_vote = level;
          i.votes.maps[level] = (i.votes.maps[level] || 0) + 1;
        }
        return this.view();
      },
      leave() {
        if (me.instance === 100) clotho.players--;
        else if (me.instance) inst[me.instance - 1].players--;
        me.instance = null;
        return this.view();
      },
    };
  })();

  // ?lobby=fill — the backend of 11.11: instances by party size (solo ×3, duo ×2, trio ×1, battles included), kind "fill",
  // no size vote, map vote all through gather, start when full or by the timer (?fill_secs=30 for a short one)
  const fillMock = (() => {
    const MAPS = [40003015, 40003017, 40003019, 40003016, 40003018, 40003020];
    const RULES = Object.fromEntries(MAPS.map((id) => {
      const hard = id === 40003019 || id === 40003020;
      return [id, { skirmish: hard, gear: hard ? "epic" : null, challenger: !hard }];
    }));
    const FILL = Number(new URLSearchParams(location.search).get("fill_secs") ?? 480);
    const SEATS = { 1: 16, 2: 18, 3: 18 };
    let nextId = 1;
    const make = (size, extra = {}) =>
      Object.assign({ id: nextId++, kind: "fill", phase: "idle", min_players: 1, ends_at: 0, players: 0, in_hall: 0, seats: SEATS[size], party_size: size, leader: null, level_id: 0, votes: { sizes: {}, maps: {} } }, extra);
    // ?lanes=1: solo lanes (backend 13.11) — one solo instance per difficulty, the map set from the start, alternating
    const LANES = new URLSearchParams(location.search).get("lanes") === "1";
    const LANE_MAPS = { 1: [40003015, 40003016], 2: [40003017, 40003018], 3: [40003019, 40003020] };
    const DIFF = { 1: "easy", 2: "classic", 3: "hard" };
    const laneTurn = { 1: 0, 2: 0, 3: 0 };
    const makeLane = (lane, extra = {}) =>
      make(1, { lane, difficulty: DIFF[lane], level_id: LANE_MAPS[lane][laneTurn[lane]++ % 2], ...extra });
    let inst = LANES
      ? [
          makeLane(1, { phase: "gather", players: 5, ends_at: now() + 300 }),
          makeLane(2, { phase: "closed", ends_at: now() + 45 }), // opens in 45 s (backend 13.11: tabs open one by one)
          makeLane(3, { phase: "battle", ends_at: now() + 20, in_hall: 16 }), // goes away in 20 s
          makeLane(3),
          make(2),
          make(3, { phase: "waitroom", ends_at: now() + 50, players: 9, in_hall: 6, seats: 18, level_id: 40003019 }),
        ]
      : [
          make(1, { phase: "gather", players: 5, ends_at: now() + 300, leader: 40003016, votes: { sizes: { 1: 5 }, maps: { 40003016: 3, 40003015: 1 } } }),
          make(2, { phase: "battle", ends_at: now() + 20, in_hall: 18, level_id: 40003018, seats: 18 }), // goes away in 20 s
          make(2),
          make(3, { phase: "waitroom", ends_at: now() + 50, players: 9, in_hall: 6, seats: 18, level_id: 40003019 }),
          make(1),
        ];
    const sorted = () => [...inst].sort((a, b) => a.party_size - b.party_size || (a.lane ?? 0) - (b.lane ?? 0) || a.id - b.id);
    const me = { instance: null, size_vote: null, map_vote: null, game_online: false, trial: null };
    const refused = (code) => fail(`api.${code}`);
    const leader = (i) => {
      const best = Object.entries(i.votes.maps).filter(([, n]) => n > 0).sort((a, b) => b[1] - a[1])[0];
      return best ? Number(best[0]) : null;
    };
    const start = (i) => {
      if (i.lane) {
        Object.assign(i, { phase: "starting", ends_at: 0 });
        inst.push(makeLane(i.lane)); // the lane's next instance, on the other map
        return;
      }
      Object.assign(i, { phase: "starting", ends_at: 0, level_id: i.leader ?? 40003016, leader: null, seats: SEATS[i.party_size] });
      inst.push(make(i.party_size)); // a fresh instance of the same size appears at once
    };
    const step = () => {
      for (const i of [...inst]) {
        if (i.phase === "closed" && i.ends_at <= now()) Object.assign(i, { phase: "idle", ends_at: 0 });
        else if (i.phase === "gather" && (i.players >= i.seats || i.ends_at <= now())) start(i);
        else if (i.phase === "starting") Object.assign(i, { phase: "waitroom", ends_at: now() + 180, in_hall: Math.min(i.players, 1) });
        else if (i.phase === "waitroom" && i.ends_at <= now()) Object.assign(i, { phase: "battle", ends_at: now() + 40 });
        else if (i.phase === "battle" && i.ends_at && i.ends_at <= now()) {
          inst = inst.filter((x) => x !== i); // the finished battle's tab goes away
          if (me.instance === i.id) Object.assign(me, { instance: null, size_vote: null, map_vote: null });
        }
      }
    };
    setInterval(step, 3000);
    const find = (n) => inst.find((i) => i.id === n);
    return {
      view: () => ({ enabled: true, maintenance: null, maintenance_i18n: null, now: now(), maps: MAPS, sizes: [1, 2, 3], min_players: 1, fill_secs: FILL, rules: RULES, instances: sorted().map((i) => ({ ...i })), me }),
      join(n) {
        const i = find(n);
        if (!i) throw refused("instance_unknown");
        if (i.phase === "closed") throw refused("instance_not_open");
        if (i.phase === "battle" || i.phase === "starting" || i.phase === "waitroom") throw refused("instance_battle");
        if (me.instance) this.leave();
        me.instance = n;
        me.size_vote = i.party_size;
        i.players++;
        i.votes.sizes[i.party_size] = (i.votes.sizes[i.party_size] || 0) + 1;
        if (i.phase === "idle") Object.assign(i, { phase: "gather", ends_at: now() + FILL });
        return this.view();
      },
      vote(size, level) {
        const i = find(me.instance);
        if (!i || i.phase !== "gather") throw refused("vote_over");
        if (size && size !== i.party_size) throw refused("bad_vote");
        if (level) {
          if (me.map_vote) i.votes.maps[me.map_vote]--;
          me.map_vote = level;
          i.votes.maps[level] = (i.votes.maps[level] || 0) + 1;
          i.leader = leader(i);
        }
        return this.view();
      },
      leave() {
        const i = find(me.instance);
        if (i) {
          i.players--;
          i.votes.sizes[i.party_size]--;
          if (me.map_vote) i.votes.maps[me.map_vote]--;
          i.leader = leader(i);
          if (i.phase === "gather" && i.players === 0) Object.assign(i, { phase: "idle", ends_at: 0 });
        }
        Object.assign(me, { instance: null, size_vote: null, map_vote: null });
        return this.view();
      },
    };
  })();
  const lobbyApi = new URLSearchParams(location.search).get("lobby") === "fill" ? fillMock : lobbyMock;

  // ------------------------------------------------------------------------------------------ fake data

  const manifest = (id, name, extra = {}) =>
    Object.assign(
      {
        schema: "dbserver/1",
        id,
        name,
        description: "Свой гейммод, кооп-подземелья.\nВход через Discord, моды ставятся сами.",
        description_i18n: { ru: "Свой гейммод, кооп-подземелья.\nВход через Discord, моды ставятся сами.", en: "Our own game mode, co-op dungeons.\nLog in with Discord, mods install themselves." },
        region: "eu-central",
        languages: ["ru", "en"],
        website: "https://example.org",
        contact: "https://discord.gg/example",
        game: { exe_sha256: EXE, label: "Dungeonborne Win64 final" },
        connect: { address: "203.0.113.10" },
        mode: { id: "ru-coop", tags: ["pve", "coop", "season-2"] },
        mods: [
          { id: "ru.balance", version: "2026.1104.1341", required: true, size: 55925 },
          { id: "ru.r14-fix", version: "2026.1104.1341", required: true, size: 34760 },
          { id: "ru.client-hooks", version: "2026.1104.1341", required: true, size: 133327 },
          { id: "ru.client-probe", version: "2026.1104.1341", required: false, size: 12715 },
        ],
        policy: { allow_unlisted_client_mods: true },
        status_url: "https://example.org/dbserver/status",
        key: "ed25519:AAA",
        auth: { providers: ["discord"], api: "https://example.org/api/launcher/v1" },
        relay: { address: "203.0.113.10", port: 7443 },
        launcher: null,
        patch_notes: { url: `https://${id}/dbserver/patch_notes`, latest: "2026.11.07" },
      },
      extra,
    );

  const account = (name) => ({
    provider: "discord",
    subject: "100000000000000001",
    display_name: name,
    avatar_url: null,
    open_id: "76561197960265729",
    nick: name,
    roles: ["player", "whitelist"],
    expires_at: now() + 30 * 86400,
    logged_in_at: now(),
  });

  const server = (m, opts = {}) =>
    Object.assign({ url: `https://${m.id}/.well-known/dbserver.json`, manifest: m, key_changed: false, trusted_key: m.key, account: null, logged_in: false }, opts);

  const okInstall = { exe_sha256: EXE, running: false, dirty: false };
  const NOTES_EN = {
    latest: "2026.11.07",
    season: { id: "playtest", name: "Playtest" },
    updates: [
      {
        id: "2026.11.07", version: "2026.11.07", date: "2026-11-07", title: "Update of 7 November", lang: "en",
        sections: [
          { kind: "new", items: ["Messages about red and purple portals.", "Gear of a teammate that another player carried out comes back to its owner by mail."] },
          { kind: "fixed", items: ["The teammate mark no longer sits on everyone's items: it now marks only your team's gear."] },
        ],
      },
      { id: "2026-11-01", version: "", date: "2026-11-01", title: "Начало сезона", lang: "ru", sections: [{ kind: "new", items: ["Сезон 2."] }] },
    ],
  };
  const NOTES = {
    latest: "2026.11.07",
    season: { id: "playtest", name: "Плейтест" },
    updates: [
      {
        id: "2026.11.07", version: "2026.11.07", date: "2026-11-07", title: "Обновление 7 ноября",
        sections: [
          { kind: "new", items: ["Сообщения о красных и фиолетовых порталах.", "Снаряжение напарника, которое вынес другой игрок, приходит владельцу по почте."] },
          { kind: "fixed", items: ["Значок команды на снаряжении показывается у своей команды.", "Падения игры при долгой игре."] },
          { kind: "balance", items: ["Снято ограничение «Все слоты для оборудования только…» при входе в рейд."] },
        ],
      },
      { id: "2026-11-01", version: "", date: "2026-11-01", title: "Начало сезона", sections: [{ kind: "new", items: ["Сезон 2."] }] },
    ],
  };
  const settings = (extra = {}) =>
    Object.assign({ installs: ["C:\\Steam\\steamapps\\common\\Dungeonborne", "C:\\Games\\Dungeonborne"], lang: "ru", dev_mode: false, run_as_admin: true, steam_bootstrap: true, notes_seen: {}, theme: "torch", force_hosts: false }, extra);
  const online = { online: 14, capacity: 64, backend: "dbserver-rs 0.1.0", motd: "Сезон 2: новые подземелья", motd_i18n: { ru: "Сезон 2: новые подземелья", en: "Season 2: new dungeons" }, ping_ms: 38 };

  /** One world per scenario: everything the commands read and change. */
  const SCENARIOS = {
    ready: {
      title: "Ready to play",
      make: () => ({
        settings: settings(),
        hosts: true,
        boot_install: 0,
        servers: [
          server(manifest("ru.dungeonborne.main", "Dungeonborne RU"), { account: account("Player"), logged_in: true }),
          server(manifest("eu.hardcore", "EU Hardcore", { key: "ed25519:BBB", mode: { tags: ["pvp", "hardcore"] } }), { key_changed: true, trusted_key: "ed25519:AAA" }),
        ],
        installs: [okInstall, { exe_sha256: "deadbeef", running: false, dirty: false }],
        status: { "ru.dungeonborne.main": online, "eu.hardcore": fail("network", "timeout") },
        update: { version: "1.3.0", current: "1.2.0" },
        cachedMods: ["ru.balance", "ru.r14-fix"],
      }),
    },
    fresh: {
      title: "First launch",
      make: () => ({ settings: settings({ installs: ["", ""], steam_bootstrap: false }), hosts: false, servers: [], installs: [fail("install_not_set"), fail("install_not_set")], status: {}, update: null }),
    },
    login: {
      title: "Not logged in → login",
      make: () => ({ ...SCENARIOS.ready.make(), update: null, servers: [server(manifest("ru.dungeonborne.main", "Dungeonborne RU"))], login: "ok" }),
    },
    guild: {
      title: "Login: not in the guild",
      make: () => ({ ...SCENARIOS.login.make(), login: "guild" }),
    },
    native: {
      title: "Play: native code question",
      make: () => ({ ...SCENARIOS.ready.make(), update: null, play: "native" }),
    },
    port: {
      title: "Play: port busy",
      make: () => ({ ...SCENARIOS.ready.make(), update: null, play: "port_busy" }),
    },
    running: {
      title: "Game already running, profile not cleaned up",
      make: () => ({ ...SCENARIOS.ready.make(), update: null, installs: [{ ...okInstall, running: true }, { ...okInstall, dirty: true }] }),
    },
    nonotes: {
      title: "Server without a change list",
      make: () => ({ ...SCENARIOS.ready.make(), update: null, servers: [server(manifest("ru.dungeonborne.main", "Dungeonborne RU", { patch_notes: null }), { account: account("Player"), logged_in: true })] }),
    },
    dev: {
      title: "Developer mode",
      make: () => ({ ...SCENARIOS.ready.make(), update: null, settings: settings({ dev_mode: true }) }),
    },
  };

  let W;
  let gameExit = null;

  function load(name) {
    const key = SCENARIOS[name] ? name : "ready";
    W = SCENARIOS[key].make();
    W.native_ok = [];
    W.cachedMods ??= [];
    try {
      localStorage.setItem("dbl-mock-scenario", key);
    } catch {}
    return key;
  }

  const findServer = (id) => {
    const s = W.servers.find((x) => x.manifest.id === id);
    if (!s) throw fail("server_unknown", id);
    return s;
  };

  // ------------------------------------------------------------------------------------------ play

  async function play({ id, install, optional = [], confirmNative = [] }) {
    const s = findServer(id);
    const step = (step, state, text = "", error) => {
      emit("play", { install, step, state, text, ...(error ? { error } : {}) });
      emit("log", { scope: `play${install + 1}`, level: state === "error" ? "error" : "info", text: `${step} ${state} ${text}` });
    };
    const stop = (e) => {
      step("done", "error", `${e.code}: ${e.detail}`, e);
      throw e;
    };
    step("check", "start");
    await sleep(400);
    if (s.key_changed) stop(fail("key_changed"));
    if (W.installs[install]?.code) stop(W.installs[install]);
    if (W.installs[install].exe_sha256 !== EXE) stop(fail("exe_mismatch", W.installs[install].exe_sha256));
    if (W.installs[install].running) stop(fail("game_running"));
    step("check", "done");

    step("mods", "start");
    const want = s.manifest.mods.filter((m) => m.required || optional.includes(m.id));
    for (const [n, m] of want.entries()) {
      if (!W.cachedMods.includes(m.id)) {
        for (let done = 0; done <= m.size; done += Math.ceil(m.size / 8)) {
          emit("mod_progress", { install, id: m.id, done: Math.min(done, m.size), total: m.size });
          await sleep(120);
        }
        W.cachedMods.push(m.id);
      }
      step("mods", "progress", `${n + 1}/${want.length}`);
    }
    if (W.play === "native" && !W.native_ok.includes("ru.client-hooks")) {
      if (!confirmNative.includes("ru.client-hooks")) stop(fail("native_confirm", "ru.client-hooks"));
      W.native_ok.push("ru.client-hooks");
    }
    step("mods", "done", "modset c33d40f054c4");
    step("profile", "start");
    await sleep(300);
    step("profile", "done");
    if (!W.settings.dev_mode) {
      step("relay", "start", "203.0.113.10:7443");
      await sleep(300);
      if (W.play === "port_busy") stop(fail("port_busy", "443: dbserver-rs.exe (pid 5512)"));
      step("relay", "done");
    }
    step("game", "start");
    await sleep(500);
    step("game", "running");
    const relayTick = setInterval(() => emit("relay", { ts: now(), port: 443, up: 524, down: 1494, ms: 20, reason: "closed" }), 1500);
    await new Promise((r) => (gameExit = r));
    clearInterval(relayTick);
    gameExit = null;
    step("game", "done");
    step("cleanup", "start");
    await sleep(300);
    step("cleanup", "done");
    step("done", "done");
  }

  // ------------------------------------------------------------------------------------------ commands

  const commands = {
    get_state: () => ({
      version: "1.2.0",
      settings: structuredClone(W.settings),
      servers: structuredClone(W.servers),
      hosts_installed: W.hosts,
      hosts_path: "C:\\Windows\\System32\\drivers\\etc\\hosts",
      boot_install: W.boot_install ?? null,
    }),
    save_settings: ({ settings }) => {
      W.settings = structuredClone(settings);
      W.installs = settings.installs.map((p) => (p ? okInstall : fail("install_not_set")));
    },
    check_install: ({ index }) => {
      const i = W.installs[index];
      if (i?.code) throw i;
      return i;
    },
    restore_pending: () => [],
    // ?game=none — nothing found automatically; the picker answers after a pause, as a real dialog would
    find_game: async () => (await sleep(300), new URLSearchParams(location.search).get("game") === "none" ? null : "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Dungeonborne"),
    pick_game_folder: async () => (await sleep(500), "X:\\Games\\Dungeonborne"),
    add_server: async ({ url }) => {
      await sleep(400);
      if (!/^https?:\/\//.test(url)) throw fail("bad_url", url);
      const id = `new.server.${W.servers.length + 1}`;
      W.servers.push(server(manifest(id, "New server", { mods: [] })));
      W.status[id] = { online: 3, capacity: 32, ping_ms: 71 };
      return [id];
    },
    refresh_server: () => sleep(300),
    remove_server: ({ id }) => {
      W.servers = W.servers.filter((s) => s.manifest.id !== id);
    },
    accept_key: ({ id }) => {
      const s = findServer(id);
      s.key_changed = false;
      s.trusted_key = s.manifest.key;
    },
    lobby_state: () => lobbyApi.view(),
    lobby_join: ({ instance }) => lobbyApi.join(instance),
    lobby_vote: ({ partySize, levelId }) => lobbyApi.vote(partySize, levelId),
    lobby_leave: () => lobbyApi.leave(),
    server_status: async ({ id }) => {
      await sleep(250);
      const st = W.status[id];
      if (!st || st.code) throw st || fail("network");
      return st;
    },
    net_state: async ({ id }) => {
      await sleep(150);
      return { udp_tunnel: !!W.tunnel?.[id], capable: true };
    },
    // ?bug=crash — the last run crashed; ?bug=limit — the 4th report an hour; ?bug=slow — the bot never posts
    bug_report_info: async ({ id }) => {
      await sleep(150);
      const s = findServer(id);
      const crash = new URLSearchParams(location.search).get("bug") === "crash";
      return {
        files: [
          { name: "launcher.log", size: 48211 },
          { name: "relay.log", size: 183204 },
          { name: "crash_ue4ss.log", size: 8927 },
          { name: "Dungeonborne.log", size: 2_403_118 },
        ],
        crash,
        logged_in: !!(s.logged_in && s.account),
        title_max: 100,
        description_max: 1500,
      };
    },
    bug_report: async ({ id, kind, title }) => {
      await sleep(1200);
      const s = findServer(id);
      if (!(s.logged_in && s.account)) throw fail("login_required");
      if (!title.trim()) throw fail("bug_title_required");
      W.bugs ??= {};
      if (new URLSearchParams(location.search).get("bug") === "limit" || Object.keys(W.bugs).length >= 3) throw fail("api.rate_limited", "1740");
      const rid = `r_mock${Object.keys(W.bugs).length + 1}`;
      W.bugs[rid] = { at: Date.now(), kind };
      return rid;
    },
    bug_report_status: async ({ report }) => {
      await sleep(150);
      const b = W.bugs?.[report];
      if (!b) throw fail("api.not_found");
      const posted = new URLSearchParams(location.search).get("bug") !== "slow" && Date.now() - b.at > 8000;
      return posted ? { status: "posted", url: "https://discord.com/channels/1/2/3" } : { status: "new", url: null };
    },
    net_report_text: async ({ id }) =>
      `Dungeonborne Launcher 1.5.2 (mock) - network report
server: ${id}
seen: reset - tls to dbserver.example.org: connection reset right after ClientHello

relay.log:
1762000000 port=443 up=643 down=1422 ms=19500 reason=io down`,
    set_udp_tunnel: async ({ id, on }) => {
      await sleep(200);
      W.tunnel = { ...W.tunnel, [id]: on };
      return { udp_tunnel: on, capable: true };
    },
    server_notes: async ({ id }) => {
      await sleep(200);
      if (!findServer(id).manifest.patch_notes) return null;
      return W.notes ?? (W.settings.lang === "en" ? NOTES_EN : NOTES);
    },
    // project support (support.rs): ?support=0 — none, soon — no links yet ("Coming soon"), 1 — Patreon only,
    // 2 (default) — Patreon and Boosty; ?raised=N (split 60/40 between Patreon and Boosty)
    server_support: async () => {
      await sleep(150);
      const q = new URLSearchParams(location.search);
      const n = q.get("support") === "soon" ? 0 : Number(q.get("support") ?? 2);
      if (q.get("support") === "0") return null;
      const links = [["patreon", "https://www.patreon.com/dungeonbornecommunity"], ["boosty", "https://boosty.to/hipach"]].slice(0, n);
      return {
        currency: "USD",
        goal: { id: "servers-2026-11", kind: "monthly", title: { ru: "Серверы — ноябрь", en: "Servers — November" }, raised: Number(q.get("raised") ?? 120), target: 220, ends_at: 0, covered: Number(q.get("raised") ?? 120) >= 220,
          sources: { patreon: Math.round(Number(q.get("raised") ?? 120) * 0.6), boosty: Number(q.get("raised") ?? 120) - Math.round(Number(q.get("raised") ?? 120) * 0.6) } },
        links,
      };
    },
    notes_seen: ({ id, latest }) => {
      W.settings.notes_seen = { ...W.settings.notes_seen, [id]: latest };
    },
    server_mods: ({ id }) =>
      findServer(id).manifest.mods.map((m) => ({ id: m.id, version: m.version, required: m.required, cached: W.cachedMods.includes(m.id), downloadable: true })),
    mod_cache_info: () => ({ bytes: W.cachedMods.length * 48000, packages: W.cachedMods.length }),
    clear_mod_cache: () => {
      W.cachedMods = [];
    },
    update_check: () => W.update,
    update_apply: async () => {
      for (let done = 0; done <= 18907136; done += 1890713) {
        emit("update_progress", { done, total: 18907136 });
        await sleep(150);
      }
      alert("Here the launcher would restart into the new version (sandbox).");
      W.update = null;
      emit("servers_changed");
    },
    bootstrap_status: () =>
      W.settings.installs.map((p) => (!p ? null : W.settings.steam_bootstrap ? { kind: "launcher", version: "1.2.0" } : { kind: "original" })),
    set_bootstrap: ({ enable }) => {
      W.settings.steam_bootstrap = enable;
      return W.settings.installs.filter(Boolean).map((p) => `${p}: ${enable ? "installed" : "restored"}`);
    },
    open_link: ({ url }) => {
      window.open(url, "_blank");
    },
    login: async ({ id }) => {
      const s = findServer(id);
      if (s.key_changed) throw fail("key_changed");
      setTimeout(() => emit("login", { server_id: id, state: "browser", user_code: "K7Q-4M2" }), 300);
      setTimeout(() => {
        if (W.login === "guild") {
          emit("login", { server_id: id, state: "error", error: fail("api.not_in_guild", "Вступите в Discord-сервер проекта", "https://discord.gg/example") });
        } else {
          s.account = account("Tester");
          s.logged_in = true;
          emit("login", { server_id: id, state: "ok" });
        }
      }, 3000);
    },
    cancel_login: () => {},
    logout: ({ id }) => {
      const s = findServer(id);
      s.account = null;
      s.logged_in = false;
    },
    hosts_install: async () => {
      await sleep(800);
      W.hosts = true;
      return true;
    },
    hosts_uninstall: async () => {
      await sleep(500);
      W.hosts = false;
      return false;
    },
    play,
  };

  // the title bar's window buttons: nothing to do in a browser tab
  const mockWindow = {
    minimize: async () => console.info("[sandbox] minimize"),
    toggleMaximize: async () => console.info("[sandbox] toggle maximize"),
    close: async () => console.info("[sandbox] close"),
    isMaximized: async () => false,
    onResized: async () => () => {},
  };
  window.__TAURI__ = {
    window: { getCurrentWindow: () => mockWindow },
    core: {
      invoke: async (cmd, args = {}) => {
        const f = commands[cmd];
        if (!f) throw fail("internal", `mock: no command ${cmd}`);
        return f(args);
      },
    },
    event: {
      listen: async (name, handler) => {
        (listeners[name] ??= []).push(handler);
        return () => (listeners[name] = listeners[name].filter((h) => h !== handler));
      },
    },
  };

  let current;
  try {
    current = load(new URLSearchParams(location.search).get("scenario") || localStorage.getItem("dbl-mock-scenario"));
  } catch {
    current = load("ready");
  }

  // ------------------------------------------------------------------------------------------ panel

  // Collapsible and movable (it must not hide the design under it): state kept in localStorage.
  function panel() {
    const pref = (k, d) => {
      try {
        return localStorage.getItem(`dbl-mock-${k}`) ?? d;
      } catch {
        return d;
      }
    };
    const save = (k, v) => {
      try {
        localStorage.setItem(`dbl-mock-${k}`, v);
      } catch {}
    };
    let side = pref("side", "left");
    let collapsed = pref("collapsed", "0") === "1";

    const box = document.createElement("div");
    Object.assign(box.style, {
      position: "fixed", bottom: "12px", zIndex: 99999, background: "#111", color: "#ddd",
      border: "1px solid #555", borderRadius: "8px", padding: "6px 8px", font: "12px system-ui, sans-serif",
      display: "flex", flexDirection: "column", gap: "6px", maxWidth: "240px", opacity: "0.94",
    });
    const place = () => {
      box.style.left = side === "left" ? "12px" : "";
      box.style.right = side === "right" ? "12px" : "";
    };
    const btn = (text, fn, title) => {
      const b = document.createElement("button");
      b.textContent = text;
      b.onclick = fn;
      if (title) b.title = title;
      return b;
    };

    const head = document.createElement("div");
    Object.assign(head.style, { display: "flex", gap: "6px", alignItems: "center" });
    const title = btn("", () => {
      collapsed = !collapsed;
      save("collapsed", collapsed ? "1" : "0");
      draw();
    }, "collapse / expand");
    Object.assign(title.style, { flex: "1", textAlign: "left", fontWeight: "bold", background: "none", border: "none", color: "inherit" });
    const move = btn("⇄", () => {
      side = side === "left" ? "right" : "left";
      save("side", side);
      place();
    }, "to the other corner");
    head.append(title, move);

    const body = document.createElement("div");
    Object.assign(body.style, { display: "flex", flexDirection: "column", gap: "6px" });
    const select = document.createElement("select");
    for (const [k, sc] of Object.entries(SCENARIOS)) {
      const o = document.createElement("option");
      o.value = k;
      o.textContent = sc.title;
      o.selected = k === current;
      select.append(o);
    }
    select.onchange = () => {
      load(select.value);
      location.search = `?scenario=${select.value}`;
    };
    body.append(
      select,
      btn("End the game", () => gameExit?.()),
      btn("Session lost (ban)", () => {
        const sv = W.servers.find((x) => x.logged_in);
        if (!sv) return;
        sv.logged_in = false;
        emit("session", { server_id: sv.manifest.id, error: fail("api.banned", "Доступ к серверу закрыт") });
      }),
      btn("Offer an update", () => {
        W.update = { version: "1.3.0", current: "1.2.0" };
        emit("servers_changed");
      }),
      btn("ISP: relay stalls", () => {
        const sv = W.servers.find((x) => x.logged_in) ?? W.servers[0];
        if (sv) emit("net_block", { server_id: sv.manifest.id, kind: "stall", tunnel: true, detail: "launcher relay.log: 4 stalled connections in 75 s (https 3, lobby 1), ~20 s each, io error" });
      }),
      btn("ISP: TLS reset", () => {
        const sv = W.servers[0];
        if (sv) emit("net_block", { server_id: sv.manifest.id, kind: "reset", tunnel: false, detail: "tls to dbserver.example.org: connection reset right after ClientHello" });
      }),
      btn("Game: \"Support\" → form", () => {
        const sv = W.servers.find((x) => x.logged_in) ?? W.servers[0];
        if (sv) emit("bug_request", { server_id: sv.manifest.id, scene: "Setting", role: "Player" });
      }),
      btn("Game crashed", () => {
        const sv = W.servers.find((x) => x.logged_in) ?? W.servers[0];
        if (sv) emit("game_crashed", { server_id: sv.manifest.id, exit_code: 0xc0000005 });
      }),
      btn("Log line", () => emit("log", { scope: "relay", level: "warn", text: "session check failed: network" })),
      btn("Hide the panel until reload", () => box.remove()),
    );

    const draw = () => {
      title.textContent = `${collapsed ? "▸" : "▾"} Sandbox: ${SCENARIOS[current].title}`;
      body.style.display = collapsed ? "none" : "flex";
    };
    box.append(head, body);
    place();
    draw();
    document.body.append(box);
  }
  // ?panel=hidden — no panel at all (clean screenshots of a scenario)
  if (new URLSearchParams(location.search).get("panel") !== "hidden") document.addEventListener("DOMContentLoaded", panel);
})();
