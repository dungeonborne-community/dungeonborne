// dbnative.dll — нативный помощник listen-сервера: «каждый игрок со своими данными, как на DS».
//
// Родная UBattleDsUtility::InitPlayerGameData на listen ставит игроку MyOpenId = LOCAL_GAME_MODE_OPENID и берёт
// данные из таблицы менеджера (mgr+0x80) по этому ключу; запись туда кладёт только ответ на battle_fetch при
// StartLocalBattle — одна на бой. Данные — C++ protobuf (не UStruct), из Lua их не создать.
// Помощник перед InitPlayerGameData каждого игрока:
//   1) берёт у backend DBattleUserData этого игрока (GET /ds/battle_user_data?open_id=..);
//   2) создаёт родной объект DBattleUserData, разбирает в него байты родным парсером protobuf;
//   3) родной AddBattleUserData(mgr, msg) под ключом LOCAL_GAME_MODE_OPENID — запись заменяется свежей (state 1),
//      и родная InitPlayerGameData делает полную инициализацию уже с данными этого игрока;
//      по желанию — ещё раз под настоящим OpenId (realkey=1);
// после InitPlayerGameData — родной SetOpenId(PS, настоящий OpenId).
// Код игры не патчится: только вызовы её функций и данные. Адреса — из полного дампа (Ghidra,
// docs/ghidra_startlocalbattle); перед вызовом сверяются первые байты функции, при несовпадении — отказ.
//
// Lua (UE4SS) не экспортирует API Lua, поэтому вызов такой: Lua пишет req.txt рядом с DLL, вызывает dbn_call
// (lua_CFunction из package.loadlib, стек Lua не трогает) и читает rsp.txt. Журнал — dbnative.log там же.
// 03.11: несколько серверов из одной установки — у экземпляра с -port=N (N != 7777) файлы req-N.txt / rsp-N.txt
// (то же правило в Lua: RuServer, RuClient), иначе запросы разных процессов затирали бы друг друга.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <winhttp.h>
#include <timeapi.h>
#include <psapi.h>
#include <tlhelp32.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

#pragma comment(lib, "winhttp.lib")
#pragma comment(lib, "winmm.lib")
#pragma comment(lib, "psapi.lib")

#define IMAGE_BASE 0x140000000ull
// RVA = адрес из дампа − IMAGE_BASE; база exe фиксированная (DYNAMIC_BASE нет), но берём GetModuleHandle(NULL)
#define RVA_MSG_CTOR      (0x1424a1930ull - IMAGE_BASE) // DBattleUserData::DBattleUserData(this, Arena*)
#define RVA_MSG_VTABLE    (0x146d66dc8ull - IMAGE_BASE)
#define RVA_PARSE         (0x145f93780ull - IMAGE_BASE) // Clear + разбор: (const {ptr,len}*, Message*)
#define RVA_ADD_DATA      (0x142e9b2f0ull - IMAGE_BASE) // UBattleDsUtility::AddBattleUserData(mgr, const DBattleUserData*)
#define RVA_SET_OPEN_ID   (0x142fe79a0ull - IMAGE_BASE) // ALyraPlayerState::SetOpenId(PS, const FString*), только Authority
#define RVA_SET_TEAM      (0x142fe6fd0ull - IMAGE_BASE) // ALyraPlayerState::SetGenericTeamId(this = PS+0x3b8, const FGenericTeamId*)
#define PS_TEAM_IFACE     0x3b8 // ILyraTeamAgentInterface в ALyraPlayerState (vtable +0x10 — SetGenericTeamId, +0x18 — Get)
#define PS_MY_TEAM_ID     0x879
#define RVA_SERIALIZE     (0x145f41b60ull - IMAGE_BASE) // MessageLite::SerializeToString(msg, std::string*)
#define RVA_DUMP_USER     (0x142ec68a0ull - IMAGE_BASE) // DumpUserDataToCache(PS, data, c3, c4, int *reason, c6): docs/ghidra_gameover
#define RVA_TYPE_NAME     (0x145f41250ull - IMAGE_BASE) // MessageLite::GetTypeName(msg, std::string* out)
// 03.11 (research/netclose, Ghidra по образу из дампа): UNetConnection::Close(FNetCloseResult&&) — по строке
// «UNetConnection::Close: » (единственная ссылка 0x14516f083). Driver +0x58, State +0x13c (1 USOCK_Closed,
// 2 Pending, 3 Open). FNetCloseResult — 0x30 байт: Result +0 (ENetCloseResult, 0xe = Unknown — так закрывает
// старый Close() без причины: пакет причины не шлётся), UEnum* +0x10, FString контекст +0x18, ChainedResult +0x28.
// Close шлёт закрытие канала 0 (клиент отключается), ставит State = Closed; UNetDriver::TickDispatch потом сам
// делает CleanUp (контроллер, Logout) — как при любом родном разрыве.
#define RVA_CONN_CLOSE    (0x14516efa0ull - IMAGE_BASE)
#define RVA_ENUM_CLOSE    (0x14466b3b0ull - IMAGE_BASE) // StaticEnum<ENetCloseResult>()
#define CONN_DRIVER       0x58
#define CONN_STATE        0x13c
#define NET_CLOSE_UNKNOWN 0xe
#define MSG_SIZE          0x80
#define MSG_DS_PLAYER_KEY 0x60  // DBattleUserData: указатель на DBattlePlayerDsKey (тег 101)
#define DSKEY_USER_KEY    0x10  // DBattlePlayerDsKey: ArenaStringPtr ds_user_key (тег 1)
// 02.11 (docs/ghidra_gameover): UBattleDsUtility+0x80 — TMap<FString OpenId, TSharedPtr<данные игрока>>,
// элемент 0x28: FString ключ +0, объект +0x10. В данных игрока статус +0x78 (3 — итог снят), причина +0x7c.
#define MGR_USER_MAP      0x80
#define USER_MAP_ELEM     0x28
#define DATA_STATUS       0x78
#define DATA_REASON       0x7c

static const char *LOCAL_KEY = "LOCAL_GAME_MODE_OPENID";

typedef void *(__fastcall *MsgCtorFn)(void *self, void *arena);
typedef char (__fastcall *ParseFn)(const void *piece, void *msg);
typedef uint64_t (__fastcall *AddDataFn)(void *mgr, const void *msg);
typedef void *(__fastcall *SetOpenIdFn)(void *ps, const void *fstring);
typedef void (__fastcall *SerializeFn)(const void *msg, void *stdstring);
typedef void (__fastcall *DumpUserFn)(void *ps, void *data, char c3, char c4, int *reason, char c6);
typedef void (__fastcall *SetTeamFn)(void *teamIface, const uint8_t *teamId);
typedef void *(__fastcall *TypeNameFn)(const void *msg, void *stdstring);
typedef void (__fastcall *ConnCloseFn)(void *conn, void *closeResult);
typedef void *(__fastcall *StaticEnumFn)(void);

// std::string MSVC: SSO 16 байт, size @+0x10, cap @+0x18 (пустая: cap = 15)
struct StdString { union { char buf[16]; char *ptr; } u; uint64_t size, cap; };
static const char *StdData(const struct StdString *s) { return s->cap > 15 ? s->u.ptr : s->u.buf; }

struct Prologue { uint64_t rva; unsigned char bytes[8]; const char *name; };
static const struct Prologue PROLOGUES[] = {
    { RVA_MSG_CTOR,    { 0x48, 0x89, 0x51, 0x08, 0x48, 0x8d, 0x05, 0x8d }, "DBattleUserData ctor" },
    { RVA_PARSE,       { 0x48, 0x89, 0x5c, 0x24, 0x18, 0x48, 0x89, 0x74 }, "protobuf parse" },
    { RVA_ADD_DATA,    { 0x48, 0x89, 0x5c, 0x24, 0x08, 0x55, 0x56, 0x57 }, "AddBattleUserData" },
    { RVA_SET_OPEN_ID, { 0x80, 0xb9, 0x50, 0x01, 0x00, 0x00, 0x03, 0x75 }, "SetOpenId" },
    { RVA_SERIALIZE,   { 0x48, 0x83, 0x7a, 0x18, 0x10, 0x48, 0x8b, 0xc2 }, "SerializeToString" },
    { RVA_TYPE_NAME,   { 0x48, 0x89, 0x5c, 0x24, 0x08, 0x57, 0x48, 0x83 }, "GetTypeName" },
    { RVA_SET_TEAM,    { 0x48, 0x89, 0x5c, 0x24, 0x10, 0x56, 0x57, 0x41 }, "SetGenericTeamId" },
};

static char g_dir[MAX_PATH];
static FILE *g_log;
// Идентификатор боя для backend (&session= в battle_user_data и player_result): один процесс сервера — один рейд,
// поэтому «<pid>-<время старта процесса>»; задаётся в DllMain
static char g_session[64] = "unknown";
// Суффикс файлов обмена: "-<порт>" при -port=N (N != 7777), иначе пусто; задаётся в DllMain
static char g_tag[16] = "";

// 07.11: журнал пишут игровой поток (dbn_call) и сторож выхода (свой поток) — под блокировкой
static CRITICAL_SECTION g_logLock;

static void Log(const char *fmt, ...)
{
    EnterCriticalSection(&g_logLock);
    if (!g_log) {
        char p[MAX_PATH];
        snprintf(p, sizeof p, "%sdbnative.log", g_dir);
        g_log = fopen(p, "a");
        if (!g_log) { LeaveCriticalSection(&g_logLock); return; }
    }
    SYSTEMTIME t;
    GetLocalTime(&t);
    fprintf(g_log, "%02d:%02d:%02d%s | ", t.wHour, t.wMinute, t.wSecond, g_tag);
    va_list ap;
    va_start(ap, fmt);
    vfprintf(g_log, fmt, ap);
    va_end(ap);
    fputc('\n', g_log);
    fflush(g_log);
    LeaveCriticalSection(&g_logLock);
}

static uint8_t *Base(void) { return (uint8_t *)GetModuleHandleW(NULL); }

static int CheckPrologues(char *err, size_t errsz)
{
    for (size_t i = 0; i < sizeof PROLOGUES / sizeof PROLOGUES[0]; i++) {
        const uint8_t *p = Base() + PROLOGUES[i].rva;
        if (memcmp(p, PROLOGUES[i].bytes, 8) != 0) {
            snprintf(err, errsz, "не та сборка: %s @ %p = %02x%02x%02x%02x%02x%02x%02x%02x", PROLOGUES[i].name, p,
                     p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7]);
            return 0;
        }
    }
    return 1;
}

// --- req.txt: строки key=value ---
static char g_req[4096];
static const char *ReqGet(const char *key, char *out, size_t outsz)
{
    size_t kl = strlen(key);
    for (const char *line = g_req; *line;) {
        const char *end = strchr(line, '\n');
        size_t len = end ? (size_t)(end - line) : strlen(line);
        if (len > kl && line[kl] == '=' && strncmp(line, key, kl) == 0) {
            size_t vl = len - kl - 1;
            while (vl && (line[kl + 1 + vl - 1] == '\r' || line[kl + 1 + vl - 1] == ' ')) vl--;
            if (vl >= outsz) vl = outsz - 1;
            memcpy(out, line + kl + 1, vl);
            out[vl] = 0;
            return out;
        }
        if (!end) break;
        line = end + 1;
    }
    out[0] = 0;
    return NULL;
}

static uint64_t ParseAddr(const char *s) { return _strtoui64(s, NULL, 0); }

// --- HTTP (WinHTTP, синхронно; backend на этой же машине — миллисекунды) ---
// body != NULL — POST с этим телом (Content-Type: application/x-protobuf, json — application/json). *status — код
// ответа (0 — сеть).
// Ответ любого кода возвращается (для 200 при ошибке HttpGet ниже — NULL).
static uint8_t *HttpRequest(const char *url, const void *body, size_t bodyLen, DWORD *statusOut, size_t *outLen,
                            char *err, size_t errsz, int json);

static uint8_t *HttpGet(const char *url, size_t *outLen, char *err, size_t errsz)
{
    DWORD status = 0;
    uint8_t *buf = HttpRequest(url, NULL, 0, &status, outLen, err, errsz, 0);
    if (buf && status != 200) {
        snprintf(err, errsz, "HTTP %lu: %.*s", status, (int)(*outLen > 200 ? 200 : *outLen), (char *)buf);
        free(buf);
        return NULL;
    }
    return buf;
}

static uint8_t *HttpRequest(const char *url, const void *body, size_t bodyLen, DWORD *statusOut, size_t *outLen,
                            char *err, size_t errsz, int json)
{
    wchar_t wurl[1024];
    MultiByteToWideChar(CP_UTF8, 0, url, -1, wurl, 1024);
    URL_COMPONENTS uc = { sizeof uc };
    wchar_t host[256], path[768];
    uc.lpszHostName = host; uc.dwHostNameLength = 256;
    uc.lpszUrlPath = path; uc.dwUrlPathLength = 768;
    wchar_t extra[512]; uc.lpszExtraInfo = extra; uc.dwExtraInfoLength = 512;
    if (!WinHttpCrackUrl(wurl, 0, 0, &uc)) { snprintf(err, errsz, "плохой url %s", url); return NULL; }
    wchar_t full[1280];
    _snwprintf(full, 1280, L"%s%s", path, extra);
    full[1279] = 0;

    uint8_t *buf = NULL;
    size_t len = 0, cap = 0;
    HINTERNET s = WinHttpOpen(L"dbnative", WINHTTP_ACCESS_TYPE_NO_PROXY, NULL, NULL, 0);
    HINTERNET c = s ? WinHttpConnect(s, host, uc.nPort, 0) : NULL;
    HINTERNET r = c ? WinHttpOpenRequest(c, body ? L"POST" : L"GET", full, NULL, NULL, NULL,
                                         uc.nScheme == INTERNET_SCHEME_HTTPS ? WINHTTP_FLAG_SECURE : 0) : NULL;
    *statusOut = 0;
    if (!r) { snprintf(err, errsz, "WinHttp open %lu", GetLastError()); goto done; }
    WinHttpSetTimeouts(r, 3000, 3000, 3000, 5000);
    const wchar_t *hdr = !body ? WINHTTP_NO_ADDITIONAL_HEADERS
        : json ? L"Content-Type: application/json\r\n" : L"Content-Type: application/x-protobuf\r\n";
    if (!WinHttpSendRequest(r, hdr, body ? (DWORD)-1L : 0, (LPVOID)body, (DWORD)bodyLen, (DWORD)bodyLen, 0)
        || !WinHttpReceiveResponse(r, NULL)) {
        snprintf(err, errsz, "запрос %s: ошибка %lu", url, GetLastError());
        goto done;
    }
    DWORD status = 0, sz = sizeof status;
    WinHttpQueryHeaders(r, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, NULL, &status, &sz, NULL);
    *statusOut = status;
    for (;;) {
        DWORD avail = 0, got = 0;
        if (!WinHttpQueryDataAvailable(r, &avail) || avail == 0) break;
        if (len + avail + 1 > cap) { cap = (len + avail + 1) * 2; buf = realloc(buf, cap); }
        if (!WinHttpReadData(r, buf + len, avail, &got)) break;
        len += got;
    }
    if (!buf) buf = calloc(1, 1); // ответ без тела — пустая строка, а не ошибка
    else buf[len] = 0;
done:
    if (r) WinHttpCloseHandle(r);
    if (c) WinHttpCloseHandle(c);
    if (s) WinHttpCloseHandle(s);
    *outLen = len;
    return buf;
}

// --- protobuf: дописать к сообщению ds_player_key{ds_user_key=key} — при разборе поле-сообщение сливается,
//     строка — последняя побеждает, т.е. ключ заменяется ---
static size_t PutVarint(uint8_t *p, uint64_t v)
{
    size_t n = 0;
    while (v >= 0x80) { p[n++] = (uint8_t)(v | 0x80); v >>= 7; }
    p[n++] = (uint8_t)v;
    return n;
}

static uint8_t *WithKey(const uint8_t *data, size_t len, const char *key, size_t *outLen)
{
    size_t kl = strlen(key);
    uint8_t inner[300];
    size_t il = 0;
    inner[il++] = 0x0a; // ds_user_key: тег 1, LEN
    il += PutVarint(inner + il, kl);
    memcpy(inner + il, key, kl);
    il += kl;
    uint8_t *out = malloc(len + il + 16);
    memcpy(out, data, len);
    size_t n = len;
    n += PutVarint(out + n, (101u << 3) | 2); // ds_player_key: тег 101, LEN
    n += PutVarint(out + n, il);
    memcpy(out + n, inner, il);
    n += il;
    *outLen = n;
    return out;
}

// ArenaStringPtr → std::string (MSVC: SSO 16 байт, size @+0x10, cap @+0x18)
static int ReadStdString(uint64_t tagged, char *out, size_t outsz)
{
    const uint8_t *s = (const uint8_t *)(tagged & ~3ull);
    if (!s) return 0;
    uint64_t size = *(const uint64_t *)(s + 0x10), cap = *(const uint64_t *)(s + 0x18);
    const char *data = cap > 15 ? *(const char *const *)s : (const char *)s;
    if (size >= outsz) size = outsz - 1;
    memcpy(out, data, size);
    out[size] = 0;
    return 1;
}

// Родной объект DBattleUserData из байтов; ключ записи проверяется по разобранному сообщению.
// Объект не освобождается: AddBattleUserData копирует его к себе (CopyFrom), а вызывать деструктор по неподтверждённому
// слоту vtable опаснее, чем оставить несколько КБ на игрока за бой.
static void *BuildMsg(const uint8_t *bytes, size_t len, const char *key, char *err, size_t errsz)
{
    uint8_t *b = Base();
    void *msg = _aligned_malloc(0x200, 16);
    memset(msg, 0, 0x200);
    ((MsgCtorFn)(b + RVA_MSG_CTOR))(msg, NULL);
    if (*(uint8_t **)msg != b + RVA_MSG_VTABLE) {
        snprintf(err, errsz, "конструктор: vtable %p, ждали %p", *(void **)msg, b + RVA_MSG_VTABLE);
        return NULL;
    }
    size_t full = 0;
    uint8_t *data = WithKey(bytes, len, key, &full);
    struct { const uint8_t *ptr; size_t len; } piece = { data, full };
    ((ParseFn)(b + RVA_PARSE))(&piece, msg);
    free(data); // строки protobuf копирует к себе (aliasing выключен)
    const uint8_t *dsKey = *(const uint8_t *const *)((uint8_t *)msg + MSG_DS_PLAYER_KEY);
    char got[300] = "";
    if (!dsKey || !ReadStdString(*(const uint64_t *)(dsKey + DSKEY_USER_KEY), got, sizeof got) || strcmp(got, key) != 0) {
        snprintf(err, errsz, "разбор не удался: ds_user_key = '%s', ждали '%s'", got, key);
        return NULL;
    }
    return msg;
}

// --- адрес подключения игрока: UNetConnection::RequestURL (FString, в рефлексии нет) — «/Game/...?OpenId=..&token=..»,
//     с ним клиент вошёл (по нему родной PreLogin 0x142fa17f0 берёт OpenId и token). Смещение ищем: первое FString
//     объекта соединения, которое начинается с '/' и содержит '?'. Возвращает смещение или 0. ---
static int ReadFStringW(const uint8_t *p, wchar_t *out, size_t cnt)
{
    const wchar_t *d = *(const wchar_t *const *)p;
    int32_t num = *(const int32_t *)(p + 8), max = *(const int32_t *)(p + 12);
    if (!d || ((uintptr_t)d & 1) || (uintptr_t)d < 0x10000 || (uintptr_t)d > 0x7fffffffffffull) return 0;
    if (num <= 1 || num > max || max > 4096 || d[num - 1] != 0) return 0;
    size_t n = (size_t)num - 1 < cnt - 1 ? (size_t)num - 1 : cnt - 1;
    memcpy(out, d, n * sizeof(wchar_t));
    out[n] = 0;
    return 1;
}

static int ConnRequestUrl(const uint8_t *conn, char *out, size_t outsz)
{
    static wchar_t w[4096];
    for (size_t o = 0x30; o < 0x4000; o += 8) {
        int ok = 0;
        __try { ok = ReadFStringW(conn + o, w, 4096); } __except (EXCEPTION_EXECUTE_HANDLER) { ok = 0; }
        if (ok && w[0] == L'/' && wcschr(w, L'?')) {
            WideCharToMultiByte(CP_UTF8, 0, w, -1, out, (int)outsz, NULL, NULL);
            return (int)o;
        }
    }
    out[0] = 0;
    return 0;
}

// Опция адреса «?Key=Value» (ключ без учёта регистра); нет — пустая строка
static void UrlOpt(const char *url, const char *key, char *out, size_t outsz)
{
    size_t kl = strlen(key);
    out[0] = 0;
    for (const char *q = strchr(url, '?'); q; q = strchr(q + 1, '?')) {
        if (_strnicmp(q + 1, key, kl) == 0 && q[1 + kl] == '=') {
            const char *v = q + 2 + kl;
            size_t n = strcspn(v, "?#& ");
            if (n >= outsz) n = outsz - 1;
            memcpy(out, v, n);
            out[n] = 0;
            return;
        }
    }
}

// Закрыть соединение игрока родным UNetConnection::Close (выгон, 03.11). conn= — UNetConnection (PC.Player удалённого
// игрока; ULocalPlayer мод сюда не передаёт). Своя проверка пролога — не в общем списке: при расхождении не должен
// ломаться swap. Звать из игрового потока.
static const unsigned char CLOSE_PROLOGUE[8] = { 0x40, 0x55, 0x53, 0x56, 0x57, 0x41, 0x54, 0x41 };
static const unsigned char ENUM_PROLOGUE[8] = { 0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x05, 0x45 };

static int OpCloseConn(char *rsp, size_t rspsz)
{
    char connS[64];
    ReqGet("conn", connS, sizeof connS);
    uint8_t *conn = (uint8_t *)ParseAddr(connS);
    if (!conn) { snprintf(rsp, rspsz, "ok=0\nerr=нужен conn\n"); return 0; }
    const uint8_t *pc = Base() + RVA_CONN_CLOSE, *pe = Base() + RVA_ENUM_CLOSE;
    if (memcmp(pc, CLOSE_PROLOGUE, 8) != 0 || memcmp(pe, ENUM_PROLOGUE, 8) != 0) {
        snprintf(rsp, rspsz, "ok=0\nerr=не та сборка: Close/StaticEnum\n");
        return 0;
    }
    void *driver = NULL;
    int32_t state = -1;
    __try {
        driver = *(void **)(conn + CONN_DRIVER);
        state = *(const int32_t *)(conn + CONN_STATE);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        snprintf(rsp, rspsz, "ok=0\nerr=conn не читается\n");
        return 0;
    }
    if (!driver || state < 1 || state > 3) {
        snprintf(rsp, rspsz, "ok=0\nerr=не соединение: driver=%p state=%d\n", driver, state);
        return 0;
    }
    if (state == 1) { snprintf(rsp, rspsz, "ok=1\nalready=1\n"); return 1; }
    // FNetCloseResult по значению на стеке, как временный объект в родном Close(ENetCloseResult): Close его забирает
    // (move) — после вызова поля обнулены, освобождать нечего
    __declspec(align(16)) uint8_t result[0x40];
    memset(result, 0, sizeof result);
    *(uint32_t *)result = NET_CLOSE_UNKNOWN;
    *(void **)(result + 0x10) = ((StaticEnumFn)(Base() + RVA_ENUM_CLOSE))();
    ((ConnCloseFn)(Base() + RVA_CONN_CLOSE))(conn, result);
    int32_t after = *(const int32_t *)(conn + CONN_STATE);
    snprintf(rsp, rspsz, "ok=%d\nstate_before=%d\nstate_after=%d\n", after == 1, state, after);
    return after == 1;
}

static int OpConnUrl(char *rsp, size_t rspsz)
{
    char connS[64], url[2048], open[128], token[256];
    ReqGet("conn", connS, sizeof connS);
    const uint8_t *conn = (const uint8_t *)ParseAddr(connS);
    if (!conn) { snprintf(rsp, rspsz, "ok=0\nerr=нужен conn\n"); return 0; }
    int off = ConnRequestUrl(conn, url, sizeof url);
    if (!off) { snprintf(rsp, rspsz, "ok=0\nerr=RequestURL не найден\n"); return 0; }
    UrlOpt(url, "OpenId", open, sizeof open);
    UrlOpt(url, "token", token, sizeof token);
    snprintf(rsp, rspsz, "ok=1\noff=0x%x\nurl=%.900s\nopen_id=%s\ntoken=%s\n", off, url, open, token);
    return 1;
}

// --- protobuf: чтение varint и поиск поля верхнего уровня ---
static int GetVarint(const uint8_t *p, const uint8_t *end, uint64_t *v, const uint8_t **next)
{
    uint64_t r = 0;
    for (int s = 0; p < end && s < 64; s += 7) {
        uint8_t b = *p++;
        r |= (uint64_t)(b & 0x7f) << s;
        if (!(b & 0x80)) { *v = r; *next = p; return 1; }
    }
    return 0;
}

// Поле field: wire 0 — значение в *val; wire 2 — начало и длина в *sub/*sublen. 1 — найдено.
static int ProtoField(const uint8_t *p, size_t len, uint32_t field, uint64_t *val, const uint8_t **sub, size_t *sublen)
{
    const uint8_t *end = p + len;
    while (p < end) {
        uint64_t key, v;
        if (!GetVarint(p, end, &key, &p)) return 0;
        uint32_t f = (uint32_t)(key >> 3), w = (uint32_t)(key & 7);
        if (w == 0) {
            if (!GetVarint(p, end, &v, &p)) return 0;
            if (f == field) { *val = v; return 1; }
        } else if (w == 2) {
            if (!GetVarint(p, end, &v, &p) || v > (uint64_t)(end - p)) return 0;
            if (f == field) { *sub = p; *sublen = (size_t)v; return 1; }
            p += v;
        } else if (w == 1) p += 8;
        else if (w == 5) p += 4;
        else return 0;
    }
    return 0;
}

// DBattleUserData.user_parameter (2) -> DBattleUserParameter.team_id (1, int64); 0 — нет
static int64_t TeamIdOf(const uint8_t *data, size_t len)
{
    const uint8_t *up; size_t uplen; uint64_t v = 0;
    if (!ProtoField(data, len, 2, &v, &up, &uplen)) return 0;
    const uint8_t *dummy; size_t dl;
    if (!ProtoField(up, uplen, 1, &v, &dummy, &dl)) return 0;
    return (int64_t)v;
}

// Команда игрока родным ALyraPlayerState::SetGenericTeamId (только authority): кроме MyTeamID обновляет состав
// команд в ULyraTeamSubsystem и рассылает смену команды — простая запись свойства этого не делает
static int OpSetTeam(char *rsp, size_t rspsz)
{
    char psS[64], teamS[16], err[512] = "";
    ReqGet("ps", psS, sizeof psS);
    ReqGet("team", teamS, sizeof teamS);
    uint8_t *ps = (uint8_t *)ParseAddr(psS);
    int team = atoi(teamS);
    if (!ps || !teamS[0] || team < 0 || team > 254) { snprintf(rsp, rspsz, "ok=0\nerr=нужны ps, team 0..254\n"); return 0; }
    if (!CheckPrologues(err, sizeof err)) { snprintf(rsp, rspsz, "ok=0\nerr=%s\n", err); return 0; }
    uint8_t before = ps[PS_MY_TEAM_ID], id = (uint8_t)team;
    ((SetTeamFn)(Base() + RVA_SET_TEAM))(ps + PS_TEAM_IFACE, &id);
    snprintf(rsp, rspsz, "ok=1\nbefore=%u\nafter=%u\n", before, ps[PS_MY_TEAM_ID]);
    return 1;
}

// team_id игрока до инициализации (ChoosePlayerStart — раньше InitPlayerGameData): лёгкий GET /ds/team_of?open_id=
// -> {"team_id": N} (база — из url данных боя, до «/ds/»). Не /ds/battle_user_data: у повторной выдачи побочные
// эффекты (комплект deadmoney, новая сессия выдачи raid_entry, 403/409 в строгом режиме).
static int OpTeamOf(char *rsp, size_t rspsz)
{
    char open[128], url[512], err[512] = "", full[800];
    ReqGet("open_id", open, sizeof open);
    ReqGet("url", url, sizeof url);
    const char *ds = strstr(url, "/ds/");
    if (!open[0] || !ds) { snprintf(rsp, rspsz, "ok=0\nerr=нужны open_id и url с /ds/\n"); return 0; }
    snprintf(full, sizeof full, "%.*s/ds/team_of?open_id=%s", (int)(ds - url), url, open);
    DWORD status = 0;
    size_t len = 0;
    uint8_t *body = HttpRequest(full, NULL, 0, &status, &len, err, sizeof err, 0);
    if (!body) { snprintf(rsp, rspsz, "ok=0\nerr=%s\n", err); return 0; }
    const char *t = status == 200 ? strstr((char *)body, "\"team_id\"") : NULL;
    const char *colon = t ? strchr(t, ':') : NULL;
    if (!colon) {
        snprintf(rsp, rspsz, "ok=0\nerr=HTTP %lu: %.200s\n", status, (char *)body);
        free(body);
        return 0;
    }
    long long team = _strtoi64(colon + 1, NULL, 10);
    free(body);
    snprintf(rsp, rspsz, "ok=1\nteam_id=%lld\n", team);
    return 1;
}

static int OpSwap(char *rsp, size_t rspsz)
{
    char mgrS[64], open[128], url[512], realkey[8], err[512] = "";
    ReqGet("mgr", mgrS, sizeof mgrS);
    ReqGet("open_id", open, sizeof open);
    ReqGet("url", url, sizeof url);
    ReqGet("realkey", realkey, sizeof realkey);
    void *mgr = (void *)ParseAddr(mgrS);
    if (!mgr || !open[0] || !url[0]) { snprintf(rsp, rspsz, "ok=0\nerr=нужны mgr, open_id, url\n"); return 0; }
    if (!CheckPrologues(err, sizeof err)) { snprintf(rsp, rspsz, "ok=0\nerr=%s\n", err); return 0; }

    // conn= (UNetConnection игрока) — token из его адреса подключения уходит в backend (&token=), сверяет backend
    char connS[64], curl[2048] = "", token[256] = "";
    if (ReqGet("conn", connS, sizeof connS) && ParseAddr(connS) && ConnRequestUrl((const uint8_t *)ParseAddr(connS), curl, sizeof curl))
        UrlOpt(curl, "token", token, sizeof token);
    char full[1000];
    snprintf(full, sizeof full, "%s%s&session=%s%s%s", url, open, g_session, token[0] ? "&token=" : "", token);
    // 03.11: tries=N (1..5), pause_ms — повтор только при ошибке сети (нет ответа HTTP: 12029, 12002, таймаут);
    // ответ backend с кодом (403, 409, …) не повторяем. Вызов синхронный в игровом потоке — повторы держат сервер.
    char triesS[16] = "", pauseS[16] = "";
    ReqGet("tries", triesS, sizeof triesS);
    ReqGet("pause_ms", pauseS, sizeof pauseS);
    int tries = atoi(triesS), pause = atoi(pauseS);
    if (tries < 1) tries = 1;
    if (tries > 5) tries = 5;
    if (pause < 0 || pause > 3000) pause = 500;
    size_t len = 0;
    uint8_t *bytes = NULL;
    int attempt = 0;
    while (++attempt <= tries) {
        err[0] = 0;
        bytes = HttpGet(full, &len, err, sizeof err);
        if (bytes || strncmp(err, "HTTP ", 5) == 0 || attempt == tries) break;
        Sleep((DWORD)pause);
    }
    if (!bytes) { snprintf(rsp, rspsz, "ok=0\nerr=%s\nattempts=%d\n", err, attempt); return 0; }

    const char *keys[2] = { LOCAL_KEY, open };
    int nkeys = realkey[0] == '1' ? 2 : 1;
    uint64_t rc[2] = { 0, 0 };
    for (int i = 0; i < nkeys; i++) {
        void *msg = BuildMsg(bytes, len, keys[i], err, sizeof err);
        if (!msg) { free(bytes); snprintf(rsp, rspsz, "ok=0\nerr=%s\n", err); return 0; }
        rc[i] = ((AddDataFn)(Base() + RVA_ADD_DATA))(mgr, msg);
        if ((uint32_t)rc[i] != 0) {
            free(bytes);
            snprintf(rsp, rspsz, "ok=0\nerr=AddBattleUserData(%s) = 0x%llx\n", keys[i], (unsigned long long)(uint32_t)rc[i]);
            return 0;
        }
    }
    int64_t teamId = TeamIdOf(bytes, len);
    free(bytes);
    snprintf(rsp, rspsz, "ok=1\nbytes=%zu\nkeys=%d\nteam_id=%lld\nattempts=%d\n", len, nkeys, (long long)teamId, attempt);
    return 1;
}

// 08.11 (docs/LEYLINE_SEALS.md, разбор LOCAL): родной UBattleUtility::Loot (0x142ed1bf0) ищет строку лута по ключу
// (источник, UBattleDsUtility+0x1d4, LootType). +0x1d0 (map_id) и +0x1d4 (loot_map_id = map_detail+0x60) ставит только
// InitGameMap (0x142e9c1e0); на listen её не зовём, поля 0 — и любой бой брал лут карты 0 (таблицы Classic/Hard и
// печати лей-линий не работали). lootmap: ds=<адрес UBattleDsUtility> map=<map_id> loot=<loot_map_id> — ставит оба
// поля; без map/loot — только читает. Сборку exe сверяет CheckPrologues (смещения — для неё).
#define DSU_MAP_ID      0x1d0
#define DSU_LOOT_MAP_ID 0x1d4
#define MAP_ID_MAX      999999

static int OpLootMap(char *rsp, size_t rspsz)
{
    char dsS[64], mapS[16], lootS[16], err[512] = "";
    ReqGet("ds", dsS, sizeof dsS);
    ReqGet("map", mapS, sizeof mapS);
    ReqGet("loot", lootS, sizeof lootS);
    uint8_t *ds = (uint8_t *)ParseAddr(dsS);
    if (!ds) { snprintf(rsp, rspsz, "ok=0\nerr=нужен ds\n"); return 0; }
    if (!CheckPrologues(err, sizeof err)) { snprintf(rsp, rspsz, "ok=0\nerr=%s\n", err); return 0; }
    MEMORY_BASIC_INFORMATION mbi;
    int32_t *f = (int32_t *)(ds + DSU_MAP_ID);
    if (!VirtualQuery(f, &mbi, sizeof mbi) || mbi.State != MEM_COMMIT ||
        !(mbi.Protect & (PAGE_READWRITE | PAGE_EXECUTE_READWRITE)) || (uint8_t *)(f + 2) > (uint8_t *)mbi.BaseAddress + mbi.RegionSize) {
        snprintf(rsp, rspsz, "ok=0\nerr=адрес %p не для записи\n", (void *)f);
        return 0;
    }
    int32_t beforeMap = f[0], beforeLoot = f[1];
    if (beforeMap < 0 || beforeMap > MAP_ID_MAX || beforeLoot < 0 || beforeLoot > MAP_ID_MAX) {
        snprintf(rsp, rspsz, "ok=0\nerr=странные значения map=%d loot=%d — не тот объект?\n", beforeMap, beforeLoot);
        return 0;
    }
    if (!mapS[0] || !lootS[0]) { snprintf(rsp, rspsz, "ok=1\nmap=%d\nloot=%d\n", beforeMap, beforeLoot); return 1; }
    long map = strtol(mapS, NULL, 10), loot = strtol(lootS, NULL, 10);
    if (map < 0 || map > MAP_ID_MAX || loot < 0 || loot > MAP_ID_MAX) {
        snprintf(rsp, rspsz, "ok=0\nerr=map, loot 0..%d\n", MAP_ID_MAX);
        return 0;
    }
    f[0] = (int32_t)map;
    f[1] = (int32_t)loot;
    snprintf(rsp, rspsz, "ok=1\nbefore_map=%d\nbefore_loot=%d\nmap=%d\nloot=%d\n", beforeMap, beforeLoot, f[0], f[1]);
    return 1;
}

static int OpSetOpenId(char *rsp, size_t rspsz)
{
    char psS[64], open[128], err[512] = "";
    ReqGet("ps", psS, sizeof psS);
    ReqGet("open_id", open, sizeof open);
    void *ps = (void *)ParseAddr(psS);
    if (!ps || !open[0]) { snprintf(rsp, rspsz, "ok=0\nerr=нужны ps, open_id\n"); return 0; }
    if (!CheckPrologues(err, sizeof err)) { snprintf(rsp, rspsz, "ok=0\nerr=%s\n", err); return 0; }
    wchar_t w[128];
    int n = MultiByteToWideChar(CP_UTF8, 0, open, -1, w, 128); // с завершающим нулём, как FString
    struct { wchar_t *data; int32_t num, max; } fs = { w, n, n };
    ((SetOpenIdFn)(Base() + RVA_SET_OPEN_ID))(ps, &fs); // копирует строку к себе
    snprintf(rsp, rspsz, "ok=1\n");
    return 1;
}

// Проба итога игрока (только чтение): записи карты данных игроков UBattleDsUtility — ключ, статус, причина, тип
// сообщения. dump=<ключ> — ещё и сериализовать запись с этим ключом родным SerializeToString в
// gameover-<ключ>-<статус>-<время>.bin рядом с DLL (разбор: protoc --decode_raw или типы backend).
static void Sanitize(char *s)
{
    for (; *s; s++) if (!((*s >= '0' && *s <= '9') || (*s >= 'a' && *s <= 'z') || (*s >= 'A' && *s <= 'Z'))) *s = '_';
}

static int OpGoState(char *rsp, size_t rspsz)
{
    char mgrS[64], dump[128], err[512] = "";
    ReqGet("mgr", mgrS, sizeof mgrS);
    ReqGet("dump", dump, sizeof dump);
    const uint8_t *mgr = (const uint8_t *)ParseAddr(mgrS);
    if (!mgr) { snprintf(rsp, rspsz, "ok=0\nerr=нужен mgr\n"); return 0; }
    if (!CheckPrologues(err, sizeof err)) { snprintf(rsp, rspsz, "ok=0\nerr=%s\n", err); return 0; }

    // TSet: TSparseArray {data, num, max; TBitArray флагов: inline 4 dword @+0x10, вторичный указатель @+0x20,
    // NumBits @+0x28}
    const uint8_t *set = mgr + MGR_USER_MAP;
    const uint8_t *elems = *(const uint8_t *const *)set;
    int32_t num = *(const int32_t *)(set + 8), nbits = *(const int32_t *)(set + 0x28);
    const uint32_t *flags = *(const uint32_t *const *)(set + 0x20);
    if (!flags) flags = (const uint32_t *)(set + 0x10);
    size_t off = (size_t)snprintf(rsp, rspsz, "ok=1\nnum=%d\n", num);
    for (int32_t i = 0; elems && i < num && i < nbits && off + 300 < rspsz; i++) {
        if (!((flags[i >> 5] >> (i & 31)) & 1)) continue;
        const uint8_t *e = elems + (size_t)i * USER_MAP_ELEM;
        char key[128] = "?", type[128] = "?";
        int32_t st = -1, rs = -1;
        uint64_t blen = 0;
        const uint8_t *data = NULL;
        __try {
            const wchar_t *kd = *(const wchar_t *const *)e;
            if (kd && *(const int32_t *)(e + 8) > 0) WideCharToMultiByte(CP_UTF8, 0, kd, -1, key, sizeof key, NULL, NULL);
            data = *(const uint8_t *const *)(e + 0x10);
            if (data) {
                st = *(const int32_t *)(data + DATA_STATUS);
                rs = *(const int32_t *)(data + DATA_REASON);
                struct StdString t = { { { 0 } }, 0, 15 };
                ((TypeNameFn)(Base() + RVA_TYPE_NAME))(data, &t);
                snprintf(type, sizeof type, "%.*s", (int)(t.size < 120 ? t.size : 120), StdData(&t));
                if (dump[0] && strcmp(dump, key) == 0) {
                    struct StdString s = { { { 0 } }, 0, 15 };
                    ((SerializeFn)(Base() + RVA_SERIALIZE))(data, &s);
                    blen = s.size;
                    char safe[128], p[MAX_PATH];
                    snprintf(safe, sizeof safe, "%s", key);
                    Sanitize(safe);
                    SYSTEMTIME tm;
                    GetLocalTime(&tm);
                    snprintf(p, sizeof p, "%sgameover-%s-%d-%02d%02d%02d.bin", g_dir, safe, st, tm.wHour, tm.wMinute, tm.wSecond);
                    FILE *f = fopen(p, "wb");
                    if (f) { fwrite(StdData(&s), 1, (size_t)s.size, f); fclose(f); }
                    // буфер строки выделен аллокатором игры — не освобождаем (несколько КБ на итог)
                }
            }
        } __except (EXCEPTION_EXECUTE_HANDLER) {
            snprintf(type, sizeof type, "исключение 0x%08lx", GetExceptionCode());
        }
        off += (size_t)snprintf(rsp + off, rspsz - off, "e%d=%s|%d|%d|%s|%p|%llu\n", i, key, st, rs, type, (void *)data,
                                (unsigned long long)blen);
    }
    return 1;
}

// Итог игрока в backend (контракт docs/DS_PIPELINE.md «Нативный итог рейда»): запись карты с ключом key (настоящий
// OpenId, нужна realkey в native_data.txt) в статусе 3 сериализуется родным SerializeToString и уходит
// POST <url>?open_id=<key>&status=<статус>&reason=<причина>, тело — protobuf. Копия — gameover-<key>-post-*.bin.
// Ответ: ok=1 http=<код> body=<ответ backend>; ok=0 — запись не найдена/не в статусе 3/сеть (повторить можно).
// 06.11 переход на этаж: родной DumpUserDataToCache снимает с живого героя надетое и сумки в запись (то же, что при выходе
// порталом, но c3 = 1 — предметы «можно на этаж» остаются, docs/ghidra_gameover/README.md), reason = NULL — статус записи
// не трогает (игрок не помечается вышедшим). Запись -> POST url (backend отдаст её
// как deploy_preview на следующем этаже). Вызывать только из игрового потока.
static int OpGoFloor(char *rsp, size_t rspsz)
{
    char mgrS[64], psS[64], key[128], url[512], err[512] = "";
    ReqGet("mgr", mgrS, sizeof mgrS);
    ReqGet("ps", psS, sizeof psS);
    ReqGet("key", key, sizeof key);
    ReqGet("url", url, sizeof url);
    const uint8_t *mgr = (const uint8_t *)ParseAddr(mgrS);
    void *ps = (void *)ParseAddr(psS);
    if (!mgr || !ps || !key[0] || !url[0]) { snprintf(rsp, rspsz, "ok=0\nerr=нужны mgr, ps, key, url\n"); return 0; }
    if (!CheckPrologues(err, sizeof err)) { snprintf(rsp, rspsz, "ok=0\nerr=%s\n", err); return 0; }

    const uint8_t *set = mgr + MGR_USER_MAP;
    const uint8_t *elems = *(const uint8_t *const *)set;
    int32_t num = *(const int32_t *)(set + 8), nbits = *(const int32_t *)(set + 0x28);
    const uint32_t *flags = *(const uint32_t *const *)(set + 0x20);
    if (!flags) flags = (const uint32_t *)(set + 0x10);
    uint8_t *data = NULL;
    for (int32_t i = 0; elems && i < num && i < nbits && !data; i++) {
        if (!((flags[i >> 5] >> (i & 31)) & 1)) continue;
        const uint8_t *e = elems + (size_t)i * USER_MAP_ELEM;
        const wchar_t *kd = *(const wchar_t *const *)e;
        char k[128] = "";
        if (kd && *(const int32_t *)(e + 8) > 0) WideCharToMultiByte(CP_UTF8, 0, kd, -1, k, sizeof k, NULL, NULL);
        if (strcmp(k, key) == 0) data = *(uint8_t *const *)(e + 0x10);
    }
    if (!data) { snprintf(rsp, rspsz, "ok=0\nerr=нет записи '%s'\n", key); return 0; }
    int32_t st = *(const int32_t *)(data + DATA_STATUS);
    if (st == 3) { snprintf(rsp, rspsz, "ok=0\nerr=статус 3: итог уже снят\n"); return 0; }

    // 10.11: c3 = 1 — снятие «на этаж»: каждый предмет проверяется 0x142f96be0 (бит 1 флагов типа — запрет переноса
    // между этажами) вместо 0x142f96b20 (бит 0 — запрет выноса из рейда), запись о выносе не делается. Камни
    // воскрешения 100014/100046 (bring_out CAN_NOT_BUT_ALLOW_CROSS_LAYER = 1) при c3 = 0 терялись на втором этаже.
    ((DumpUserFn)(Base() + RVA_DUMP_USER))(ps, data, 1, 1, NULL, 0);
    int32_t after = *(const int32_t *)(data + DATA_STATUS);

    struct StdString s = { { { 0 } }, 0, 15 };
    ((SerializeFn)(Base() + RVA_SERIALIZE))(data, &s); // буфер — аллокатор игры, не освобождаем (как OpGoReport)
    char full[800];
    snprintf(full, sizeof full, "%s%sopen_id=%s&session=%s", url, strchr(url, '?') ? "&" : "?", key, g_session);
    DWORD status = 0;
    size_t len = 0;
    uint8_t *body = HttpRequest(full, StdData(&s), (size_t)s.size, &status, &len, err, sizeof err, 0);
    if (!body) { snprintf(rsp, rspsz, "ok=0\nerr=%s\nbytes=%llu\n", err, (unsigned long long)s.size); return 0; }
    for (size_t i = 0; i < len; i++) if (body[i] == '\n' || body[i] == '\r') body[i] = ' ';
    snprintf(rsp, rspsz, "ok=1\nhttp=%lu\nbytes=%llu\nstatus_before=%d\nstatus_after=%d\nbody=%.400s\n", status,
             (unsigned long long)s.size, st, after, (char *)body);
    free(body);
    return 1;
}

static int OpGoReport(char *rsp, size_t rspsz)
{
    char mgrS[64], key[128], url[512], err[512] = "";
    ReqGet("mgr", mgrS, sizeof mgrS);
    ReqGet("key", key, sizeof key);
    ReqGet("url", url, sizeof url);
    const uint8_t *mgr = (const uint8_t *)ParseAddr(mgrS);
    if (!mgr || !key[0] || !url[0]) { snprintf(rsp, rspsz, "ok=0\nerr=нужны mgr, key, url\n"); return 0; }
    if (!CheckPrologues(err, sizeof err)) { snprintf(rsp, rspsz, "ok=0\nerr=%s\n", err); return 0; }

    const uint8_t *set = mgr + MGR_USER_MAP;
    const uint8_t *elems = *(const uint8_t *const *)set;
    int32_t num = *(const int32_t *)(set + 8), nbits = *(const int32_t *)(set + 0x28);
    const uint32_t *flags = *(const uint32_t *const *)(set + 0x20);
    if (!flags) flags = (const uint32_t *)(set + 0x10);
    const uint8_t *data = NULL;
    for (int32_t i = 0; elems && i < num && i < nbits && !data; i++) {
        if (!((flags[i >> 5] >> (i & 31)) & 1)) continue;
        const uint8_t *e = elems + (size_t)i * USER_MAP_ELEM;
        const wchar_t *kd = *(const wchar_t *const *)e;
        char k[128] = "";
        if (kd && *(const int32_t *)(e + 8) > 0) WideCharToMultiByte(CP_UTF8, 0, kd, -1, k, sizeof k, NULL, NULL);
        if (strcmp(k, key) == 0) data = *(const uint8_t *const *)(e + 0x10);
    }
    if (!data) { snprintf(rsp, rspsz, "ok=0\nerr=нет записи '%s' (realkey в native_data.txt?)\n", key); return 0; }
    int32_t st = *(const int32_t *)(data + DATA_STATUS), rs = *(const int32_t *)(data + DATA_REASON);
    if (st != 3) { snprintf(rsp, rspsz, "ok=0\nerr=статус %d, ждали 3\n", st); return 0; }

    struct StdString s = { { { 0 } }, 0, 15 };
    ((SerializeFn)(Base() + RVA_SERIALIZE))(data, &s); // буфер — аллокатор игры, не освобождаем
    char safe[128], p[MAX_PATH], full[800];
    snprintf(safe, sizeof safe, "%s", key);
    Sanitize(safe);
    SYSTEMTIME tm;
    GetLocalTime(&tm);
    snprintf(p, sizeof p, "%sgameover-%s-post-%02d%02d%02d.bin", g_dir, safe, tm.wHour, tm.wMinute, tm.wSecond);
    FILE *f = fopen(p, "wb");
    if (f) { fwrite(StdData(&s), 1, (size_t)s.size, f); fclose(f); }

    snprintf(full, sizeof full, "%s%sopen_id=%s&status=%d&reason=%d&session=%s", url, strchr(url, '?') ? "&" : "?", key,
             st, rs, g_session);
    DWORD status = 0;
    size_t len = 0;
    uint8_t *body = HttpRequest(full, StdData(&s), (size_t)s.size, &status, &len, err, sizeof err, 0);
    if (!body) { snprintf(rsp, rspsz, "ok=0\nerr=%s\nbytes=%llu\n", err, (unsigned long long)s.size); return 0; }
    for (size_t i = 0; i < len; i++) if (body[i] == '\n' || body[i] == '\r') body[i] = ' ';
    snprintf(rsp, rspsz, "ok=1\nhttp=%lu\nbytes=%llu\nstatus=%d\nreason=%d\nbody=%.600s\n", status,
             (unsigned long long)s.size, st, rs, (char *)body);
    free(body);
    return 1;
}

#ifndef PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION
#define PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION 0x4
#endif

// Тикрейт headless-сервера = частота кадров; кадр ограничивается сном (t.MaxFPS). Windows 11 у фонового процесса
// без видимого окна включает EcoQoS (урезает частоту ядра) и не выполняет timeBeginPeriod — сон растягивается до
// ~15.6 мс, и сервер проседает до ~64 кадров и ниже. Здесь: EcoQoS выключен, разрешение таймера соблюдается для
// этого процесса, таймер 1 мс; priority=above — класс приоритета выше обычного.
static int OpPerf(char *rsp, size_t rspsz)
{
    char prio[16];
    ReqGet("priority", prio, sizeof prio);
    PROCESS_POWER_THROTTLING_STATE st = { 0 };
    st.Version = PROCESS_POWER_THROTTLING_CURRENT_VERSION;
    st.ControlMask = PROCESS_POWER_THROTTLING_EXECUTION_SPEED | PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION;
    st.StateMask = 0; // ни одно из ограничений не включено
    BOOL okThrottle = SetProcessInformation(GetCurrentProcess(), ProcessPowerThrottling, &st, sizeof st);
    DWORD errThrottle = okThrottle ? 0 : GetLastError();
    MMRESULT okTimer = timeBeginPeriod(1);
    BOOL okPrio = TRUE;
    if (strcmp(prio, "above") == 0) okPrio = SetPriorityClass(GetCurrentProcess(), ABOVE_NORMAL_PRIORITY_CLASS);
    // affinity=<маска> (0x… или десятичная) — логические процессоры этого процесса; 03.11, несколько серверов на ПК
    char aff[32];
    ReqGet("affinity", aff, sizeof aff);
    char affOut[96] = "";
    if (aff[0]) {
        DWORD_PTR mask = (DWORD_PTR)_strtoui64(aff, NULL, 0);
        BOOL okAff = mask && SetProcessAffinityMask(GetCurrentProcess(), mask);
        DWORD errAff = okAff ? 0 : GetLastError();
        snprintf(affOut, sizeof affOut, "affinity=0x%llx ok=%d err=%lu\n", (unsigned long long)mask, okAff, errAff);
    }
    DWORD_PTR procMask = 0, sysMask = 0;
    GetProcessAffinityMask(GetCurrentProcess(), &procMask, &sysMask);
    snprintf(rsp, rspsz, "ok=%d\nthrottle_off=%d err=%lu\ntimer_1ms=%d\npriority=%s(%d) class=0x%lx\n%smask=0x%llx\n",
             okThrottle && okTimer == TIMERR_NOERROR, okThrottle, errThrottle, okTimer == TIMERR_NOERROR,
             prio[0] ? prio : "normal", okPrio, GetPriorityClass(GetCurrentProcess()), affOut,
             (unsigned long long)procMask);
    return 1;
}

// Новый рейд в том же процессе (03.11, сброс сервера между матчами): session = <pid>-r<epoch>. Backend считает рейд
// завершённым по (игрок, session) — без смены сессии игрок прошлого матча получал бы 409 raid_finished.
static int OpNewSession(char *rsp, size_t rspsz)
{
    char epoch[32];
    ReqGet("epoch", epoch, sizeof epoch);
    if (!epoch[0]) { snprintf(rsp, rspsz, "ok=0\nerr=нужен epoch\n"); return 0; }
    snprintf(g_session, sizeof g_session, "%lu-r%s", GetCurrentProcessId(), epoch);
    Log("новая session %s", g_session);
    snprintf(rsp, rspsz, "ok=1\nsession=%s\n", g_session);
    return 1;
}

// JSON-запрос к backend (03.11, heartbeat слота POST /ds/slot): url, body — тело одной строкой (нет — GET).
// Ответ: ok=1 http=<код> body=<ответ, переводы строк заменены пробелами>; ok=0 err= — сеть.
static int OpHttp(char *rsp, size_t rspsz)
{
    char url[512], body[2048], err[512] = "";
    ReqGet("url", url, sizeof url);
    ReqGet("body", body, sizeof body);
    if (!url[0]) { snprintf(rsp, rspsz, "ok=0\nerr=нужен url\n"); return 0; }
    DWORD status = 0;
    size_t len = 0;
    uint8_t *out = HttpRequest(url, body[0] ? body : NULL, strlen(body), &status, &len, err, sizeof err, 1);
    if (!out) { snprintf(rsp, rspsz, "ok=0\nerr=%s\n", err); return 0; }
    for (size_t i = 0; i < len; i++) if (out[i] == '\n' || out[i] == '\r') out[i] = ' ';
    snprintf(rsp, rspsz, "ok=1\nhttp=%lu\nbody=%.1500s\n", status, (char *)out);
    free(out);
    return 1;
}

// Замер ресурсов своего процесса (03.11): память (рабочий набор, личная), процессорное время с начала процесса
// (мс, ядро + пользователь — долю CPU Lua считает по приросту), число потоков и дескрипторов
static int OpStats(char *rsp, size_t rspsz)
{
    PROCESS_MEMORY_COUNTERS_EX mc = { 0 };
    mc.cb = sizeof mc;
    GetProcessMemoryInfo(GetCurrentProcess(), (PROCESS_MEMORY_COUNTERS *)&mc, sizeof mc);
    FILETIME created, exited, kernel, user;
    unsigned long long cpuMs = 0;
    if (GetProcessTimes(GetCurrentProcess(), &created, &exited, &kernel, &user)) {
        ULARGE_INTEGER k, u;
        k.LowPart = kernel.dwLowDateTime; k.HighPart = kernel.dwHighDateTime;
        u.LowPart = user.dwLowDateTime; u.HighPart = user.dwHighDateTime;
        cpuMs = (k.QuadPart + u.QuadPart) / 10000;
    }
    DWORD pid = GetCurrentProcessId(), threads = 0, handles = 0;
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    if (snap != INVALID_HANDLE_VALUE) {
        THREADENTRY32 te = { 0 };
        te.dwSize = sizeof te;
        for (BOOL more = Thread32First(snap, &te); more; more = Thread32Next(snap, &te))
            if (te.th32OwnerProcessID == pid) threads++;
        CloseHandle(snap);
    }
    GetProcessHandleCount(GetCurrentProcess(), &handles);
    snprintf(rsp, rspsz, "ok=1\nws_mb=%llu\npeak_ws_mb=%llu\nprivate_mb=%llu\ncpu_ms=%llu\nnow_ms=%llu\nthreads=%lu\nhandles=%lu\n",
             (unsigned long long)(mc.WorkingSetSize >> 20), (unsigned long long)(mc.PeakWorkingSetSize >> 20),
             (unsigned long long)(mc.PrivateUsage >> 20), cpuMs, (unsigned long long)GetTickCount64(), threads, handles);
    return 1;
}

// Сторож выхода (07.11, docs/CRASH_LONG_PLAY.md): после консольного quit мод взводит его на sec секунд; если процесс
// ещё жив — завершаем сами. Раньше это делал os.exit из служебного потока Lua (LoopAsync), но Lua в двух потоках
// роняет UE4SS; снаружи зависший процесс под ACE убить не удаётся (30.10). Поток без Lua и без вызовов игры.
static volatile LONG g_quitArmed;

static DWORD WINAPI QuitWatchThread(LPVOID param)
{
    DWORD ms = (DWORD)(uintptr_t)param;
    Sleep(ms);
    Log("quitwatch: процесс жив через %lu с после quit — TerminateProcess", ms / 1000);
    TerminateProcess(GetCurrentProcess(), 0);
    return 0;
}

static int OpQuitWatch(char *rsp, size_t rspsz)
{
    char v[16];
    ReqGet("sec", v, sizeof v);
    int sec = atoi(v);
    if (sec < 1 || sec > 600) { snprintf(rsp, rspsz, "ok=0\nerr=sec 1..600\n"); return 0; }
    if (InterlockedCompareExchange(&g_quitArmed, 1, 0) != 0) { snprintf(rsp, rspsz, "ok=1\nalready=1\n"); return 1; }
    HANDLE t = CreateThread(NULL, 0, QuitWatchThread, (LPVOID)(uintptr_t)(sec * 1000), 0, NULL);
    if (!t) { InterlockedExchange(&g_quitArmed, 0); snprintf(rsp, rspsz, "ok=0\nerr=CreateThread %lu\n", GetLastError()); return 0; }
    CloseHandle(t);
    snprintf(rsp, rspsz, "ok=1\nsec=%d\n", sec);
    return 1;
}

static int RunOp(char *rsp, size_t rspsz)
{
    char op[32];
    ReqGet("op", op, sizeof op);
    if (strcmp(op, "swap") == 0) return OpSwap(rsp, rspsz);
    if (strcmp(op, "setopenid") == 0) return OpSetOpenId(rsp, rspsz);
    if (strcmp(op, "perf") == 0) return OpPerf(rsp, rspsz);
    if (strcmp(op, "stats") == 0) return OpStats(rsp, rspsz);
    if (strcmp(op, "http") == 0) return OpHttp(rsp, rspsz);
    if (strcmp(op, "newsession") == 0) return OpNewSession(rsp, rspsz);
    if (strcmp(op, "session") == 0) { snprintf(rsp, rspsz, "ok=1\nsession=%s\n", g_session); return 1; }
    if (strcmp(op, "gostate") == 0) return OpGoState(rsp, rspsz);
    if (strcmp(op, "goreport") == 0) return OpGoReport(rsp, rspsz);
    if (strcmp(op, "gofloor") == 0) return OpGoFloor(rsp, rspsz);
    if (strcmp(op, "connurl") == 0) return OpConnUrl(rsp, rspsz);
    if (strcmp(op, "closeconn") == 0) return OpCloseConn(rsp, rspsz);
    if (strcmp(op, "setteam") == 0) return OpSetTeam(rsp, rspsz);
    if (strcmp(op, "teamof") == 0) return OpTeamOf(rsp, rspsz);
    if (strcmp(op, "quitwatch") == 0) return OpQuitWatch(rsp, rspsz);
    if (strcmp(op, "lootmap") == 0) return OpLootMap(rsp, rspsz);
    if (strcmp(op, "check") == 0) {
        char err[512] = "";
        if (CheckPrologues(err, sizeof err)) snprintf(rsp, rspsz, "ok=1\n");
        else snprintf(rsp, rspsz, "ok=0\nerr=%s\n", err);
        return 1;
    }
    snprintf(rsp, rspsz, "ok=0\nerr=неизвестная операция '%s'\n", op);
    return 0;
}

__declspec(dllexport) int dbn_call(void *L)
{
    (void)L;
    char p[MAX_PATH], rsp[4096] = "";
    snprintf(p, sizeof p, "%sreq%s.txt", g_dir, g_tag);
    FILE *f = fopen(p, "rb");
    size_t n = f ? fread(g_req, 1, sizeof g_req - 1, f) : 0;
    if (f) fclose(f);
    g_req[n] = 0;
    remove(p);

    __try {
        RunOp(rsp, sizeof rsp);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        snprintf(rsp, sizeof rsp, "ok=0\nerr=исключение 0x%08lx\n", GetExceptionCode());
    }
    char oneLine[4096];
    snprintf(oneLine, sizeof oneLine, "%s", rsp);
    for (char *c = oneLine; *c; c++) if (*c == '\n') *c = ' ';
    char op[32];
    ReqGet("op", op, sizeof op);
    static char lastGo[4096]; // gostate зовётся раз в секунду — в журнал только изменения
    if (strcmp(op, "gostate") != 0 || strcmp(lastGo, oneLine) != 0) Log("%s -> %s", op[0] ? op : "?", oneLine);
    if (strcmp(op, "gostate") == 0) snprintf(lastGo, sizeof lastGo, "%s", oneLine);

    snprintf(p, sizeof p, "%srsp%s.txt", g_dir, g_tag);
    f = fopen(p, "wb");
    if (f) { fputs(rsp, f); fclose(f); }
    return 0; // lua_CFunction: результатов на стеке нет
}

BOOL WINAPI DllMain(HINSTANCE h, DWORD reason, LPVOID r)
{
    (void)r;
    if (reason == DLL_PROCESS_ATTACH) {
        InitializeCriticalSection(&g_logLock);
        GetModuleFileNameA(h, g_dir, MAX_PATH);
        char *slash = strrchr(g_dir, '\\');
        if (slash) slash[1] = 0;
        // -port=N в командной строке (без учёта регистра) -> суффикс "-N", кроме порта по умолчанию 7777
        char cmd[4096];
        snprintf(cmd, sizeof cmd, "%s", GetCommandLineA());
        _strlwr(cmd);
        const char *pp = strstr(cmd, "-port=");
        int port = pp ? atoi(pp + 6) : 0;
        if (port > 0 && port != 7777) snprintf(g_tag, sizeof g_tag, "-%d", port);
        FILETIME created, exited, kernel, user;
        SYSTEMTIME st;
        if (GetProcessTimes(GetCurrentProcess(), &created, &exited, &kernel, &user) && FileTimeToSystemTime(&created, &st))
            snprintf(g_session, sizeof g_session, "%lu-%04d%02d%02d%02d%02d%02d", GetCurrentProcessId(), st.wYear, st.wMonth,
                     st.wDay, st.wHour, st.wMinute, st.wSecond);
        Log("загружен, exe base %p, session %s", Base(), g_session);
    }
    return TRUE;
}
