"""Send a native raid result to the backend manually (the same thing dbnative does): POST /ds/player_result.

    python -I tools/player_result.py <gameover.bin> --open-id 76561198000000903 --reason 2
    python -I tools/player_result.py <gameover.bin> --open-id … --reason 3          # exit through a portal

reason — EnDSPlayerActorStatusType: 1 SELF_QUIT, 2 DEAD, 3 ESCAPE, 4 ALIVE; status is always 3 (GAMEOVER).
The backend applies the result only if it previously gave the player match data (GET /ds/battle_user_data) and the result has not been applied yet.
"""
import argparse
import json
import sys
import urllib.parse
import urllib.request


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("bin")
    ap.add_argument("--open-id", required=True)
    ap.add_argument("--reason", type=int, required=True)
    ap.add_argument("--status", type=int, default=3)
    ap.add_argument("--base", default="http://127.0.0.1:9091")
    a = ap.parse_args()
    body = open(a.bin, "rb").read()
    q = urllib.parse.urlencode({"open_id": a.open_id, "status": a.status, "reason": a.reason})
    req = urllib.request.Request(a.base + "/ds/player_result?" + q, data=body, method="POST",
                                 headers={"Content-Type": "application/x-protobuf"})
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            print(r.status, json.dumps(json.loads(r.read()), ensure_ascii=False))
    except urllib.error.HTTPError as e:
        print(e.code, e.read().decode("utf-8", "replace"))
        sys.exit(1)


if __name__ == "__main__":
    main()
