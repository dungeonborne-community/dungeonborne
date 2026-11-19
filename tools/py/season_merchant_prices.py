"""The merchant buys equipment back for 50% of the standard Buyout Trades price (season playtest, user's decision 13.11).

Builds seasons/<season>/balance/item_type.json from the base balance/item_type.json: system_repurchase_price
(what the merchant pays; the client shows the same number in the tooltip) = price_def // 2 of the market 1 (Buyout Trades) row
of the seasonal trade_commodity, for those with price_def > 0. The list with the previous prices — research/merchant_prices/merchant_prices_50.json.
- relics 220xxxxxx — 0 (the backend sells crafted ones to the merchant for 0 at any price, the tooltip must not lie);
- items of starter sets (role_default_equipment) that have 0 in the base — 0, as in the original: the set is given on
  character creation and again on login if nothing is equipped — otherwise 800–1700 gold per set out of nothing;
- other items (not equipment, no row on market 1) — as in the base.

    python -I -X utf8 tools/season_merchant_prices.py [--season playtest]
Then `cargo xtask balance build --season=<season>`.
"""
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, W  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)
import argparse
import json
from pathlib import Path

MARKET_BUYOUT = 1


def load(p):
    return json.loads(Path(p).read_text(encoding="utf-8"))


def is_relic(t):
    return 220000000 <= t < 230000000


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--season", default="playtest")
    a = ap.parse_args()
    out = W("seasons") / a.season / "balance"
    tc_path = out / "trade_commodity.json" if (out / "trade_commodity.json").exists() else W("balance/trade_commodity.json")
    price = {}
    for r in load(tc_path)["rows"]:
        if r["market_id"] == MARKET_BUYOUT and r.get("price_def", 0) > 0:
            price[r["item_id"]] = r["price_def"]
    doc = load(str(W("balance/item_type.json")))
    starter = {r["item_id"] for r in load(str(W("balance/role_default_equipment.json")))["rows"]}
    changed = relics = kept = 0
    for r in doc["rows"]:
        t = r["type_id"]
        if t not in price or r.get("item_major_type") != "EN_ITEM_MAJOR_TYPE_EQUIPMENT":
            continue
        if t in starter and not r.get("system_repurchase_price"):
            kept += 1
            continue
        new = 0 if is_relic(t) else price[t] // 2
        relics += is_relic(t)
        if r.get("system_repurchase_price", 0) != new:
            if new:
                r["system_repurchase_price"] = new
            else:
                r.pop("system_repurchase_price", None)
            changed += 1
    doc["header"]["count"] = len(doc["rows"])
    doc["header"]["description"] = (doc["header"].get("description", "") + f" Season {a.season}: merchant pays 50% of Buyout price_def.").strip()
    (out / "item_type.json").write_text(json.dumps(doc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"merchant price changed for {changed} types (of {len(price)} on market 1, relics with 0: {relics}, starter with 0: {kept}) ->{out / 'item_type.json'}")


if __name__ == "__main__":
    main()
