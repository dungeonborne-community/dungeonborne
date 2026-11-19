"""End-to-end check of login and the relay without the game (src/auth, docs/LAUNCHER_SPEC.md §4) — what the launcher will do.

Start the backend with a test config (provider dev, require = relay, separate DB):
    $env:SERVER_TOML='probe/auth_test/server.toml'; $env:DB_PATH='probe/auth_test/db'; cargo run
Then:
    python -I tools/auth_e2e.py [--web http://127.0.0.1:8090]

Steps: manifest → login/start → dev page → callback → poll (token) → me → direct HTTP game login rejected →
via the relay (TLS, certificate pin from the manifest) the HTTP game login passes under the account's open_id → someone else's token
rejected → refresh (the old token is dead) → logout.
"""
import argparse
import hashlib
import json
import socket
import ssl
import sys
import urllib.parse
import urllib.request

OK = FAIL = 0


def check(cond, what):
    global OK, FAIL
    print(("  OK   " if cond else "  FAIL ") + what)
    if cond:
        OK += 1
    else:
        FAIL += 1


def http(method, url, body=None, token=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, method=method)
    if data is not None:
        req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.status, r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode("utf-8", "replace")


GAME_LOGIN_BODY = json.dumps({"channel_info": {"steamid": "11111111111111111", "user_name": "spoof"}, "channelid": 21})


def game_login_request(host):
    return (
        "POST /v2/auth/login?seq=7 HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\n"
        "Content-Length: {}\r\nConnection: close\r\n\r\n{}".format(host, len(GAME_LOGIN_BODY), GAME_LOGIN_BODY)
    ).encode()


def read_all(sock):
    out = b""
    while True:
        try:
            chunk = sock.recv(65536)
        except (socket.timeout, ssl.SSLError):
            break
        if not chunk:
            break
        out += chunk
    return out


def relay_open(addr, port, pin, token, target_port):
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE  # verified by the pin below, as in the launcher (relay.cert_sha256)
    raw = socket.create_connection((addr, port), timeout=10)
    s = ctx.wrap_socket(raw)
    der = s.getpeercert(binary_form=True)
    pin_ok = hashlib.sha256(der).hexdigest() == pin
    s.sendall((json.dumps({"v": 1, "token": token, "port": target_port, "client": "auth_e2e"}) + "\n").encode())
    line = b""
    while not line.endswith(b"\n"):
        c = s.recv(1)
        if not c:
            break
        line += c
    return s, pin_ok, json.loads(line or b"{}")


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("--web", default="http://127.0.0.1:8090")
    ap.add_argument("--name", default="Tester")
    a = ap.parse_args()

    print("1. manifest")
    st, body = http("GET", a.web + "/.well-known/dbserver.json")
    m = json.loads(body) if st == 200 else {}
    check(m.get("schema") == "dbserver/1", "schema dbserver/1")
    check("dev" in m.get("auth", {}).get("providers", []), "auth.providers contains dev")
    relay = m.get("relay") or {}
    check(bool(relay.get("cert_sha256")), "relay.cert_sha256 (pin) is present")
    api = m.get("auth", {}).get("api", a.web + "/api/launcher/v1")

    print("2. login via the dev provider")
    st, body = http("POST", api + "/login/start", {})
    s = json.loads(body)
    check(st == 200 and s.get("login_id") and s.get("user_code"), "login/start: login_id and user_code")
    st, body = http("GET", api + "/login/poll?login_id=" + s["login_id"])
    check(json.loads(body).get("status") == "pending", "poll before login: pending")
    st, page = http("GET", s["browser_url"])
    check(st == 200 and "/auth/dev/callback" in page, "dev page with the form")
    cb = a.web + "/auth/dev/callback?" + urllib.parse.urlencode({"state": s["login_id"], "name": a.name})
    st, page = http("GET", cb)
    check(st == 200 and s["user_code"] in page, "callback: \"Done\" page with the same user_code")
    st, body = http("GET", api + "/login/poll?login_id=" + s["login_id"])
    p = json.loads(body)
    check(p.get("status") == "ok" and len(p.get("session_token", "")) == 64, "poll: ok and token")
    token, acc = p.get("session_token"), p.get("account", {})
    check(acc.get("open_id", "").startswith("765611979"), "open_id issued by the server: " + acc.get("open_id", ""))
    st, body = http("GET", api + "/login/poll?login_id=" + s["login_id"])
    check(st == 404, "repeated poll: token already handed out (404)")
    st, body = http("GET", api + "/me", token=token)
    check(st == 200 and json.loads(body).get("open_id") == acc.get("open_id"), "me via Bearer")

    print("3. direct HTTP game login (without the relay) rejected")
    try:
        c = socket.create_connection(("127.0.0.1", 80), timeout=5)
        c.sendall(game_login_request("127.0.0.1"))
        resp = read_all(c).decode("utf-8", "replace")
        check('"ret":1' in resp.replace(" ", ""), "ret 1 (login via launcher required)")
    except OSError as e:
        check(False, "port 80: " + str(e))

    print("4. HTTP game login via the relay")
    sk, pin_ok, rsp = relay_open(relay.get("address", "127.0.0.1"), relay.get("port", 7443), relay.get("cert_sha256"), token, 80)
    check(pin_ok, "the relay certificate matched the manifest pin")
    check(rsp.get("ok") is True, "preface accepted")
    sk.sendall(game_login_request("sg.intlgame.com"))
    resp = read_all(sk).decode("utf-8", "replace")
    sk.close()
    check(acc.get("open_id", "?") in resp and "11111111111111111" not in resp, "game login under the account's open_id, the claimed SteamID is ignored")

    print("5. relay rejections")
    sk, _, rsp = relay_open(relay.get("address", "127.0.0.1"), relay.get("port", 7443), relay.get("cert_sha256"), "0" * 64, 80)
    sk.close()
    check(rsp.get("error") == "expired", "someone else's token → expired")
    sk, _, rsp = relay_open(relay.get("address", "127.0.0.1"), relay.get("port", 7443), relay.get("cert_sha256"), token, 22)
    sk.close()
    check(rsp.get("error") == "bad_port", "port 22 → bad_port")

    print("6. refresh and logout")
    st, body = http("POST", api + "/session/refresh", token=token)
    new = json.loads(body).get("session_token") if st == 200 else None
    check(bool(new) and new != token, "refresh: new token")
    st, _ = http("GET", api + "/me", token=token)
    check(st == 401, "old token after refresh — 401")
    http("POST", api + "/logout", token=new)
    st, _ = http("GET", api + "/me", token=new)
    check(st == 401, "after logout — 401")

    print("\nresult: OK {}, FAIL {}".format(OK, FAIL))
    sys.exit(1 if FAIL else 0)


if __name__ == "__main__":
    main()
