// Dungeonborne Dev UI language: English by default, Russian optional (Settings → Language / Язык).
// The markup and app.js are written in English; RU maps an English text to its Russian version. Static texts are
// translated once at load (text nodes, title, placeholder); mixed markup — elements with data-i18n-html="key" (RU_HTML).
// Dynamic texts go through tr("English {x}", { x }). Messages that come from the Rust side stay English.
"use strict";

const RU = {
  // header, tabs
  "Profile": "Профиль",
  "backend → server (if any) → clients": "бэкенд → сервер (если есть) → клиенты",
  "▶ Run profile": "▶ Запустить профиль",
  "Stop the scenario (whatever is already running keeps running)": "Остановить сценарий (запущенное продолжит работать)",
  "Abort": "Прервать",
  "■ Stop backend": "■ Стоп бэкенда",
  "Backend": "Бэкенд",
  "Game": "Игра",
  "Tools": "Инструменты",
  "Settings": "Настройки",
  // backend tab
  "Name": "Имя",
  "(default: server.toml if present — otherwise no login)": "(по умолчанию: server.toml, если есть — иначе без входа)",
  "surreal_saves (main)": "surreal_saves (основная)",
  "Clients (in order)": "Клиенты (по порядку)",
  "Client mode": "Режим клиента",
  "game window (log in by hand)": "окно игры (вход руками)",
  "-dbclient: logs in by itself and connects to 127.0.0.1:7777": "-dbclient: сам входит и подключается к 127.0.0.1:7777",
  "Mode": "Режим",
  "none": "нет",
  "server (listen as dedicated)": "сервер (listen как dedicated)",
  "server + native match (DS)": "сервер + родной матч (DS)",
  "Server install": "Установка сервера",
  "Map": "Карта",
  "/TgfMaps/... (mod default)": "/TgfMaps/... (по умолчанию мода)",
  "observer": "наблюдатель",
  "server window": "окно сервера",
  "copy": "копировать",
  "Save profile": "Сохранить профиль",
  "Duplicate": "Копия",
  "Delete": "Удалить",
  "Backend only": "Только бэкенд",
  "Ports": "Порты",
  "check": "проверить",
  "port": "порт",
  "for": "для",
  "held by": "кто держит",
  "search…": "поиск…",
  "autoscroll": "автопрокрутка",
  "clear": "очистить",
  // game tab
  "Installs": "Установки",
  "path": "путь",
  "state": "состояние",
  "Launch mode": "Режим запуска",
  "game window": "окно игры",
  "Kill mutexes": "Снять мьютексы",
  "List mutexes (DbTools)": "Список мьютексов (DbTools)",
  "quit to client (-dbclient)": "quit клиенту (-dbclient)",
  "probe to client": "probe клиенту",
  // host tab
  "Game server": "Игровой сервер",
  "Install": "Установка",
  "window": "окно",
  "ip:port (DS SDK experiment)": "ip:port (опыт DS SDK)",
  "Start server": "Запустить сервер",
  "Writes quit to dbserver-control.txt (the backend does not pass quit)": "Пишет quit в dbserver-control.txt (бэкенд quit не передаёт)",
  "Restore match-mode": "Вернуть match-mode",
  "Server commands via the backend": "Команды серверу через бэкенд",
  "Portal in front of players": "Портал перед игроками",
  "Open all exits": "Открыть все выходы",
  "— online player —": "— игрок онлайн —",
  "refresh": "обновить",
  "Portal in front of player…": "Портал перед игроком…",
  "ds-agent (backend :9091)": "ds-agent (бэкенд :9091)",
  // gm tab
  "Admin API": "Админ-API",
  "Mock": "Мок",
  "mock with require = relay": "мок с require = relay",
  "Stop mock": "Стоп мока",
  "Log in (relay)": "Войти (relay)",
  "Log out": "Выйти",
  "or paste a Bearer token": "или вставить Bearer-токен",
  "set": "задать",
  "Online": "Онлайн",
  "every 5 s": "каждые 5 с",
  "nick": "ник",
  "roles": "роли",
  "online": "в сети",
  "GM console —": "Консоль GM —",
  "no player selected": "игрок не выбран",
  "command, e.g. item 100101 2 (↑/↓ — history)": "команда, напр. item 100101 2 (↑/↓ — история)",
  "Run": "Выполнить",
  // tools tab
  "Stop": "Стоп",
  "rpc_dump folder": "папка rpc_dump",
  "--web http://127.0.0.1:8090 (backend with probe/auth_test/server.toml)": "--web http://127.0.0.1:8090 (бэкенд с probe/auth_test/server.toml)",
  // settings tab
  "Paths and addresses": "Пути и адреса",
  "backups (cargo run, tools)": "backups (cargo run, инструменты)",
  "Install 1": "Установка 1",
  "Install 2": "Установка 2",
  "Install 3 (optional)": "Установка 3 (необязательно)",
  "Launcher web API (relay login)": "Веб-API лаунчера (вход relay)",
  "Save": "Сохранить",
  "settings folder": "папка настроек",
  // app.js: errors
  "Ports busy": "Порты заняты",
  "Port lock": "Замок портов",
  "UAC cancelled": "UAC отменён",
  "Backend already running": "Бэкенд уже запущен",
  "No game exe": "Нет exe игры",
  "401: admin Bearer needed (Log in / paste a token)": "401: нужен Bearer админа (Войти / вставить токен)",
  "403: no rights (admin role) or the request is not from loopback": "403: нет прав (роль admin) или запрос не с петли",
  "404: no player with this open_id": "404: нет игрока с таким open_id",
  "No connection": "Нет связи",
  // app.js: chips
  "backend: {profile} (pid {pid})": "бэкенд: {profile} (pid {pid})",
  "backend: stopped": "бэкенд: стоп",
  "lock: {owner}": "замок: {owner}",
  "lock: free": "замок: свободен",
  "admin": "админ",
  "dev-app runs as administrator: the game starts without UAC": "dev-app от администратора: игра стартует без UAC",
  "dev-app without administrator rights: launching the game asks for UAC": "dev-app без прав администратора: на запуск игры — запрос UAC",
  "mutexes are killed by the DbTools task without UAC": "мьютексы снимаются задачей DbTools без UAC",
  "no DbTools": "нет DbTools",
  "mutexes — killmutex.ps1 via UAC (see dbtest\\README_admin.md)": "мьютексы — killmutex.ps1 через UAC (см. dbtest\\README_admin.md)",
  "game: {n}": "игра: {n}",
  "server: {phase} ({age} s)": "сервер: {phase} ({age} с)",
  "mock :{port}{relay}": "мок :{port}{relay}",
  // app.js: profile
  "profile saved": "профиль сохранён",
  " (copy)": " (копия)",
  "the last profile cannot be deleted": "последний профиль не удаляется",
  "Delete profile \"{name}\"?": "Удалить профиль «{name}»?",
  "copied": "скопировано",
  // app.js: ports
  "Port lock: {owner} since {since} — {purpose}": "Замок портов: {owner} с {since} — {purpose}",
  "Port lock is free (portlock.ps1)": "Замок портов свободен (portlock.ps1)",
  "free": "свободен",
  "all": "все",
  // app.js: game
  "dev-app runs as administrator — the game starts without UAC.": "dev-app запущен от администратора — игра стартует без UAC.",
  "dev-app has no administrator rights — Windows asks for UAC on every game launch (the game under ACE needs the rights). Run dev-app as administrator to avoid the prompt.":
    "dev-app без прав администратора — на каждый запуск игры Windows спросит UAC (игра под ACE требует прав). Запустите dev-app от администратора, чтобы не спрашивала.",
  "running (UE4SS.log locked)": "запущена (UE4SS.log занят)",
  "not running": "не запущена",
  ", no UE4SS": ", нет UE4SS",
  "Launch": "Запустить",
  "launching install {n}…": "запуск установки {n}…",
  "mutexes:": "мьютексы:",
  "launched": "запущена",
  "killing mutexes…": "снимаю мьютексы…",
  // app.js: host
  "phase": "фаза",
  "status written": "статус записан",
  "{s} s ago": "{s} с назад",
  "no file": "файла нет",
  "<none>": "<нет>",
  "server is starting — the phase is in \"status\"": "сервер запускается — фаза в «статусе»",
  "last command": "последняя команда",
  "result": "результат",
  "when": "когда",
  "backend sees the server: {phase}{map}": "бэкенд видит сервер: {phase}{map}",
  "backend does not see the server ({root})": "бэкенд не видит сервер ({root})",
  "admin API: ": "админ-API: ",
  "— online player ({n}) —": "— игрок онлайн ({n}) —",
  "{cmd} (waiting up to 10 s for the mod's answer)…": "{cmd} (ждём ответ мода до 10 с)…",
  "sent (probe writes no result)": "отправлено (probe результата не пишет)",
  "the mod did not answer within 10 s": "мод не ответил за 10 с",
  "phase: {phase} · map: {map}": "фаза: {phase} · карта: {map}",
  "the server is not in phase ready — command rejected": "сервер не в фазе ready — команда отклонена",
  "503: the backend did not find the server (no dbserver-status.json in DB_HOST_ROOT)": "503: бэкенд не нашёл сервер (нет dbserver-status.json в DB_HOST_ROOT)",
  "pick a player": "выберите игрока",
  // app.js: GM
  "mock": "мок",
  "backend": "бэкенд",
  " · token set": " · токен задан",
  "backend (no /health: require shows up on a 401)": "бэкенд (без /health: про require узнаем по 401)",
  "no answer: ": "нет ответа: ",
  "{n} min": "{n} мин",
  " · match": " · бой",
  "nobody online": "никого онлайн",
  "pick a player in the online list": "выберите игрока в списке онлайн",
  "nobody online — the backend has no player to run the command for": "в сети никого — бэкенду не над кем выполнить команду",
  "the only one online": "единственный онлайн",
  " (offline: DB only)": " (офлайн: только в БД)",
  "code {code}: {message}": "код {code}: {message}",
  "mock admin API on 127.0.0.1:{port}{relay}": "мок админ-API на 127.0.0.1:{port}{relay}",
  "token set by hand": "токен задан вручную",
  "login: opening the browser…": "вход: открываю браузер…",
  "no token": "токена нет",
  "error: ": "ошибка: ",
  " · code: {code}": " · код: {code}",
  // app.js: settings
  "saved": "сохранено",
};

const RU_HTML = {
  host_intro: `Headless-экземпляр игры с модом DungeonborneRuServer (<code>-dbserver</code>), как <code>dbtest\\run.ps1</code> /
          <code>play.ps1</code>: статус — <code>dbtools\\run\\dbserver-status.json</code>, команды — <code>dbserver-control.txt</code>.
          DS-режим: клиенты жмут родную кнопку матча (<code>match-mode.txt = native</code>), бэкенд сам отправляет их на
          сервер (<code>DS_POOL</code> по умолчанию <code>127.0.0.1:7777@*</code>, docs/DS_PIPELINE.md).`,
  hc_intro: `dev-app → бэкенд (<code>POST /admin/v1/host/command</code>) → сервер; мод исполняет через родной
          <code>UTgfCheatComponent</code> и отвечает в <code>dbserver-command-result.json</code>. Сервер должен быть в фазе
          <code>ready</code>. Ответ мода бэкенд ждёт до 10 с.`,
  agents_intro: `Только чтение: <code>GET /ds/agents</code>. Своего ds-agent dev-app не регистрирует — агент без опроса
          <code>/ds/next</code> задержал бы матч до запасного режима.`,
  tools_title: `Инструменты backups (<code>python -I tools/…</code>)`,
};

let LANG = "en";
try {
  LANG = localStorage.getItem("devapp.lang") === "ru" ? "ru" : "en";
} catch {}

/** English text → the current language; `{name}` placeholders are filled from `vars` */
function tr(text, vars) {
  let s = (LANG === "ru" && RU[text]) || text;
  if (vars) s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
  return s;
}

/** Static markup into Russian: text nodes, title / placeholder, data-i18n-html blocks. English needs nothing. */
function translateDom(root) {
  document.documentElement.lang = LANG;
  if (LANG !== "ru") return;
  for (const el of root.querySelectorAll("[data-i18n-html]")) {
    const html = RU_HTML[el.dataset.i18nHtml];
    if (html) el.innerHTML = html;
  }
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const core = n.nodeValue.trim();
    if (core && RU[core]) n.nodeValue = n.nodeValue.replace(core, RU[core]);
  }
  for (const el of root.querySelectorAll("[title], [placeholder]")) {
    for (const a of ["title", "placeholder"]) {
      const v = el.getAttribute(a);
      if (v && RU[v]) el.setAttribute(a, RU[v]);
    }
  }
}

function setLang(lang) {
  try {
    localStorage.setItem("devapp.lang", lang);
  } catch {}
  location.reload();
}

translateDom(document.body);
