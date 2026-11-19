"""Comparison of RPC transport and headers: PVE SoloLobby (reference, sniffer) versus our gateway.

Reads rpc_dump/<TAG>_head.jsonl (the atframework.CSMsgHead header of each frame, main.rs dump_rpc_head) and
rpc_dump/<TAG>_tgcp.jsonl (TGCP frames, main.rs dump_tgcp). Both are written with RPC_DUMP=1 (our gateway) and in PVE_SNIFF mode.

    python -I tools/compare_heads.py                 # PVE versus OUR, latest sessions in full
    python -I tools/compare_heads.py --since 05:30   # only frames from this time on (local)

What it shows:
  * per frame kind (request C2S / response S2C / push S2C): which header fields are present, constants, example values;
  * links: echo of the response client_sequence with the request, monotonicity of server_sequence, what is in session_sequence/sequence_ack;
  * TGCP: which cmd in which direction, header bytes that are constant/changing, the reply to ping 0x6001.
"""
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, W  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)
import argparse
import collections
import datetime
import json
import os

RPC_DUMP = str(W("rpc_dump"))


def load(name, since):
    path = os.path.join(RPC_DUMP, name)
    if not os.path.exists(path):
        return []
    rows = []
    for line in open(path, encoding="utf-8"):
        try:
            r = json.loads(line)
        except ValueError:
            continue
        if since and datetime.datetime.fromtimestamp(r["ts_ms"] / 1000).strftime("%H:%M:%S") < since:
            continue
        rows.append(r)
    return rows


def kind(r):
    h = r.get("head") or {}
    if r["dir"] == "C2S":
        return "request"
    return "push" if "rpc_stream" in h or not h.get("client_sequence") else "response"


def flat(o, p=""):
    if isinstance(o, dict):
        for k, v in o.items():
            yield from flat(v, f"{p}.{k}" if p else k)
    else:
        yield p, o


def heads_report(tag, rows):
    print(f"\n=== {tag}: RPC headers, {len(rows)} frames ===")
    if not rows:
        print("  (no data — run with RPC_DUMP=1 / PVE_SNIFF)")
        return
    groups = collections.defaultdict(list)
    for r in rows:
        groups[kind(r)].append(r)
    for k in ("request", "response", "push"):
        g = groups.get(k, [])
        if not g:
            continue
        vals = collections.defaultdict(list)
        for r in g:
            for f, v in flat(r.get("head") or {}):
                vals[f].append(v)
        print(f"  [{k}] {len(g)} pcs")
        for f in sorted(vals):
            vs = vals[f]
            uniq = list(dict.fromkeys(json.dumps(v, ensure_ascii=False) for v in vs))
            share = f"{len(vs)}/{len(g)}"
            if len(uniq) == 1:
                print(f"    {f:34} {share:>9}  always {uniq[0][:60]}")
            else:
                print(f"    {f:34} {share:>9}  {len(uniq)} values, e.g. {', '.join(u[:24] for u in uniq[:4])}")
    # links: response ↔ the last request with the same client_sequence
    pending = {}
    echo_ok = echo_bad = 0
    last_srv = None
    mono = True
    for r in rows:
        h = r.get("head") or {}
        if kind(r) == "request":
            pending[h.get("client_sequence")] = h
            continue
        s = h.get("server_sequence")
        if s is not None and last_srv is not None and s <= last_srv:
            mono = False
        last_srv = s if s is not None else last_srv
        if kind(r) == "response":
            if h.get("client_sequence") in pending:
                echo_ok += 1
            else:
                echo_bad += 1
    print(f"  response.client_sequence = number of the pending request: yes {echo_ok}, no {echo_bad}")
    print(f"  server_sequence strictly increases over S2C: {'yes' if mono else 'NO'}")


def tgcp_report(tag, rows):
    print(f"\n=== {tag}: TGCP frames, {len(rows)} ===")
    if not rows:
        print("  (no data)")
        return
    by = collections.defaultdict(list)
    for r in rows:
        by[(r["dir"], r["cmd"])].append(r)
    for (d, cmd), g in sorted(by.items()):
        hdrs = [bytes.fromhex(r["hdr"]) for r in g]
        n = min(len(h) for h in hdrs)
        const = "".join(
            f"{hdrs[0][i]:02x}" if all(h[i] == hdrs[0][i] for h in hdrs) else ".." for i in range(n)
        )
        print(f"  {d:12} {cmd}  {len(g):5} pcs  header {const}")
        raws = list(dict.fromkeys(r.get("raw", "") for r in g if r.get("raw")))
        for raw in raws[:2]:
            print(f"      raw {raw[:120]}{'…' if len(raw) > 120 else ''}")
    # reply to ping
    pings = [r for r in rows if r["dir"] == "C2S" and r["cmd"] == "0x6001"]
    pongs = [r for r in rows if r["dir"] == "S2C" and r["cmd"] == "0x6002"]
    print(f"  pings C2S 0x6001: {len(pings)}, replies S2C 0x6002: {len(pongs)}")
    # S2C number in [12..16] (LE) of 0x4013
    seqs = [int.from_bytes(bytes.fromhex(r["hdr"])[12:16], "little") for r in rows if r["dir"] == "S2C" and r["cmd"] == "0x4013"]
    if seqs:
        print(f"  S2C 0x4013 [12..16]: first {seqs[:8]}")
    seqs = [int.from_bytes(bytes.fromhex(r["hdr"])[12:16], "little") for r in rows if r["dir"] == "C2S" and r["cmd"] == "0x4013"]
    if seqs:
        print(f"  C2S 0x4013 [12..16]: first {seqs[:8]}")


def main():
    import sys
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("--since", help="HH:MM[:SS], local time")
    ap.add_argument("--tags", default="PVE,OUR")
    a = ap.parse_args()
    for tag in a.tags.split(","):
        heads_report(tag, load(f"{tag}_head.jsonl", a.since))
        tgcp_report(tag, load(f"{tag}_tgcp.jsonl", a.since))


if __name__ == "__main__":
    main()
