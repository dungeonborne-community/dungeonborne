"""GM commands via the backend admin API (src/admin_api.rs, 127.0.0.1:9092) — while the client has neither a GM field nor a general chat.

    python -I tools/gm.py players                    # who is online
    python -I tools/gm.py "item 100066 15"           # to the only player online
    python -I tools/gm.py "gold 1000" --open-id 76561198000000902
    python -I tools/gm.py help
    python -I tools/gm.py "host status"              # game server: phase, map, last command
    python -I tools/gm.py "host portal"              # a blue portal in front of each player (or "host portal <open_id>")
    python -I tools/gm.py "host exits"               # all map exits: spawn + activation
    python -I tools/gm.py "host kill 76561198000000903"  # kill the player's pawn (native death, death result test)
    python -I tools/gm.py "host leave all"           # send players to the lobby (ClientTravel)

With [auth] require = "relay" the admin's session token is needed: --token or env DBL_ADMIN_TOKEN (not printed to the log).
"""
import argparse
import json
import os
import sys
import urllib.request


def call(base, method, path, body=None, token=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(base + path, data=data, method=method, headers={"Content-Type": "application/json"})
    if token:
        req.add_header("Authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            return r.status, json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read() or b"null")


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", help='"players" or a full GM command, e.g. "item 100066 15"')
    ap.add_argument("--open-id")
    ap.add_argument("--base", default=os.environ.get("DBL_ADMIN", "http://127.0.0.1:9092"))
    ap.add_argument("--token", default=os.environ.get("DBL_ADMIN_TOKEN"))
    a = ap.parse_args()
    if a.cmd == "players":
        st, rsp = call(a.base, "GET", "/admin/v1/players", token=a.token)
        if st != 200:
            sys.exit("{}: {}".format(st, rsp))
        for p in rsp:
            print("{open_id}  user_id={user_id}  character={nick}".format(**p))
        print("online:", len(rsp))
        return
    # commands to the game server: "host status", "host portal [open_id|all]", "host exits", "host probe"
    if a.cmd.startswith("host"):
        host_cmd = a.cmd[4:].strip()
        if host_cmd in ("", "status"):
            st, rsp = call(a.base, "GET", "/admin/v1/host/status", token=a.token)
        else:
            st, rsp = call(a.base, "POST", "/admin/v1/host/command", {"cmd": host_cmd}, a.token)
        if st != 200:
            sys.exit("{}: {}".format(st, rsp))
        print(json.dumps(rsp, ensure_ascii=False, indent=2))
        return
    body = {"cmd": a.cmd}
    if a.open_id:
        body["open_id"] = a.open_id
    st, rsp = call(a.base, "POST", "/admin/v1/gm", body, a.token)
    if st != 200:
        sys.exit("{}: {}".format(st, rsp))
    print("player {} ({})".format(rsp["open_id"], "online" if rsp["online"] else "offline — will see it on login"))
    if rsp["result_code"] != 0:
        sys.exit("error {}: {}".format(rsp["result_code"], rsp["result_message"]))
    print("\n".join(rsp["result_list"]) or "ok")


if __name__ == "__main__":
    main()
