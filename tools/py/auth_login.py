"""Manual check of login via a provider (Discord or any other) — the way the launcher does it, without the launcher.

    python -I tools/auth_login.py [--web http://127.0.0.1:8090] [--provider discord]

Prints a code and a link: open it in a browser, log in, compare the code on the "Done" page. The script waits for the result and
prints the account. The session token is not printed.
"""
import argparse
import json
import sys
import time
import urllib.request


def call(method, url, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, method=method, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.status, json.loads(r.read() or b"{}")
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read() or b"{}")


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("--web", default="http://127.0.0.1:8090")
    ap.add_argument("--provider")
    a = ap.parse_args()
    st, m = call("GET", a.web + "/.well-known/dbserver.json")
    if st != 200:
        sys.exit("manifest: HTTP {}".format(st))
    api = m["auth"]["api"]
    print("server providers:", m["auth"]["providers"])
    st, s = call("POST", api + "/login/start", {"provider": a.provider} if a.provider else {})
    if st != 200:
        sys.exit("login/start: {} {}".format(st, s))
    print("\ncode:   ", s["user_code"])
    print("link:   ", s["browser_url"], "\n")
    deadline = time.time() + s["expires_in_s"]
    while time.time() < deadline:
        time.sleep(s["poll_interval_ms"] / 1000)
        st, p = call("GET", api + "/login/poll?login_id=" + s["login_id"])
        if st != 200:
            sys.exit("poll: {} {}".format(st, p))
        if p["status"] == "pending":
            continue
        if p["status"] == "denied":
            link = p.get("link")
            sys.exit("rejected: {} — {}{}".format(p.get("reason"), p.get("message"), "\nlink: " + link if link else ""))
        print("logged in, token received ({} characters), expires {}".format(len(p["session_token"]), p["expires_at"]))
        print(json.dumps(p["account"], ensure_ascii=False, indent=2))
        return
    sys.exit("login timed out")


if __name__ == "__main__":
    main()
