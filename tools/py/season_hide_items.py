"""Take items that do not work off the market (season playtest, user's decision 11.11).

Builds seasons/<season>/balance/trade_commodity.json from the base balance/trade_commodity.json: for the items below
is_on_shelf = false — system sellers (auction, Buyout) do not list them, players cannot list them.
- legendary and unique gear without its own effect: no affix_template.data.legendary_rule (weapons, all
  legendary armor, necklaces and rings without properties);
- the cryomancer's ice staffs of all rarities (181050002…181055002);
- 12.11 (user's decision): grey and rotted (EN_ITEM_RARITY_NORMAL, ROTTED) — the market minimum is green, as in
  the original table (there was no grey there; tools/auction_all.py added it on 06.11). The client is refused with -4205
  EN_ERR_TRADE_PRODUCT_NOT_ON_SHELF (trade_check_product).
Ice staffs are removed from loot in the template seasons/playtest/loot/loot.toml, items without an effect are not in loot.
13.11 (user's decision "Yes, x0.5–x4 everywhere"): for all rows of both markets the listing price limits are 50–400% of
price_def (price_lower_limit = 50, price_upper_limit = 400) and is_calculate_sp = false — the standard price = price_def,
without the deal average (backend daf8dc2).

    python -I -X utf8 tools/season_hide_items.py [--season playtest]
Then `cargo xtask balance build --season=<season>`; the backend removes already listed system lots of such items
on the next pass (trade_seed_system_lots).
"""
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, W  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)
import argparse
import json
from pathlib import Path

ICE_STAFFS = {181050002, 181051002, 181052002, 181053002, 181054002, 181055002}
HIGH = {"EN_ITEM_RARITY_LEGENDARY", "EN_ITEM_RARITY_UNIQUE"}
LOW = {"EN_ITEM_RARITY_ROTTED", "EN_ITEM_RARITY_NORMAL"}
PRICE_LOWER, PRICE_UPPER = 50, 400   # % of price_def, both markets (13.11)


def load(p):
    return json.loads(Path(p).read_text(encoding="utf-8"))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--season", default="playtest")
    a = ap.parse_args()
    out = W("seasons") / a.season / "balance"
    item_type = {r["type_id"]: r for r in load(str(W("balance/item_type.json")))["rows"]}
    ue = {r["type_id"]: r for r in load(str(W("balance/UeSource_ItemType.json")))["rows"]}
    tpl_path = out / "affix_template.json" if (out / "affix_template.json").exists() else W("balance/affix_template.json")
    tpl = {r["item_id"]: r for r in load(tpl_path)["rows"]}

    def rarity(t):
        u = item_type.get(t, {}).get("ue_source_type_id") or t
        return ue.get(u, {}).get("rarity", "")

    def broken(t):
        if t in ICE_STAFFS or rarity(t) in LOW:
            return True
        if not item_type.get(t, {}).get("item_minor_type", "").startswith("EN_ITEM_MINOR_TYPE_EQUIPMENT"):
            return False
        return rarity(t) in HIGH and not tpl.get(t, {}).get("data", {}).get("legendary_rule")

    tc = load(str(W("balance/trade_commodity.json")))
    hidden = set()
    for r in tc["rows"]:
        if broken(r["item_id"]) and r.get("is_on_shelf"):
            r["is_on_shelf"] = False
            hidden.add(r["item_id"])
        r["price_lower_limit"], r["price_upper_limit"] = PRICE_LOWER, PRICE_UPPER
        r.pop("is_calculate_sp", None)   # false (false is not written in the base)
    tc["header"]["count"] = len(tc["rows"])
    tc["header"]["description"] = (tc["header"].get("description", "") + f" Season {a.season}: broken legendary/unique gear, ice staffs, grey and rotted off shelf; price limits {PRICE_LOWER}-{PRICE_UPPER}% of price_def, no SP.").strip()
    (out / "trade_commodity.json").write_text(json.dumps(tc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    low = {t for t in hidden if rarity(t) in LOW}
    print(f"taken off the shelf: {len(hidden)} items (ice staffs {len(hidden & ICE_STAFFS)}, grey and rotted {len(low)}) -> {out / 'trade_commodity.json'}")


if __name__ == "__main__":
    main()
