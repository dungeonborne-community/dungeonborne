"""Loot generator for arena PVP chests (custom games) for loot/loot.toml.

Run from the backups root:  python -I tools/pvp_chests.py   then  cargo xtask loot build  and  cargo xtask balance build

What it does (FIX_LIST R1-loot, 05.11):
- chests BP_TreasureChest_PVP_{Cloth,Leather,Plate,Ring,Staff,Sword,CrossBow}_C (loot_source_map 165–171) — each
  with its own equipment type, purple only (UeSource_ItemType.rarity = EPIC), regular equipment only (id 18xxxxxxx),
  without gold and potions;
- PVP chest grid 9×10 = 90 cells (LOCAL analysis: tools/research/chest/PVP_CHEST_CAPACITY.md), item sizes —
  UeSource_ItemType.position_cfg (row_size along x, column_size along y);
- the slot composition and order (ORDERS) are chosen so that "first free place" placement fills all 90 cells both
  when walking by rows and by columns — the generator checks this with a simulation and fails if not;
- evenly by kind: each kind has its own pool, slots at 100 %;
- weapons (ROTATE): variants of one kind take turns (A, B, A, B …) — each variant has its own pool of one
  item, the k-th appearance of a kind in ORDERS takes variant k mod N. Other kinds — a random pick from the kind's pool.

Overwrites everything in loot/loot.toml from the MARK line to the end of the file (the PVP chest section is always last).
"""
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, W  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)
import json
import re
import sys
from pathlib import Path

TEMPLATE = W("loot") / "loot.toml"
BAL = W("balance")
MARK = "\n# ---- 05.11: PVP-сундуки арен"
W, H = 9, 10

# slot order by kind (found by brute force, scratchpad chest/; change only with the simulation check below)
ORDERS = {
    "armor": ["GLOVE", "HELMET", "CHEST", "SHOE", "CHEST", "TROUSERS", "CHEST", "GLOVE", "CHEST", "SHOE", "HELMET",
              "HELMET", "HELMET", "GLOVE", "TROUSERS", "TROUSERS", "SHOE", "GLOVE", "SHOE"],
    "ring": ["RING", "NECKLACE"] * 45,
    "staff": ["STAFF", "MACE", "MACE", "MACE", "MACE", "STAFF", "MACE", "RELIC", "MACE", "MACE", "STAFF", "MACE",
              "MACE", "MACE", "MACE", "STAFF", "MACE", "MACE", "RELIC", "MACE", "RELIC", "RELIC"],
    "sword": ["LONG_SWORD", "SHIELD", "SHIELD", "SWORD", "SWORD", "DAGGER", "LONG_SWORD", "LONG_SWORD", "SHIELD",
              "SWORD", "SWORD", "DAGGER", "DAGGER", "SHIELD", "SWORD", "DAGGER", "SHIELD", "SHIELD", "SWORD", "DAGGER",
              "DAGGER"],
    "crossbow": ["HEAVY_CROSSBOW"] * 15,
}
# kinds whose variants take turns (user's request 21:5x: weapons — 2 variants of each)
ROTATE = {"HEAVY_CROSSBOW", "MACE", "STAFF", "RELIC", "SHIELD", "DAGGER", "SWORD", "LONG_SWORD"}

CHESTS = [  # key, BP, source id, armor subtype prefix, plan, title (the generated loot.toml text stays Russian)
    ("cloth", "BP_TreasureChest_PVP_Cloth_C", 165, "CLOTH_", "armor", "ткань"),
    ("leather", "BP_TreasureChest_PVP_Leather_C", 167, "LEATHER_", "armor", "кожа"),
    ("plate", "BP_TreasureChest_PVP_Plate_C", 168, "PLATE_", "armor", "латы"),
    ("ring", "BP_TreasureChest_PVP_Ring_C", 169, "", "ring", "кольца и амулеты"),
    ("staff", "BP_TreasureChest_PVP_Staff_C", 170, "", "staff", "посохи, реликвии, булавы"),
    ("sword", "BP_TreasureChest_PVP_Sword_C", 171, "", "sword", "мечи, щиты, кинжалы"),
    ("crossbow", "BP_TreasureChest_PVP_CrossBow_C", 166, "", "crossbow", "арбалеты"),
]
RU = {"HELMET": "шлем", "GLOVE": "перчатки", "SHOE": "сапоги", "CHEST": "нагрудник", "TROUSERS": "штаны",
      "RING": "кольцо", "NECKLACE": "амулет", "STAFF": "посох", "RELIC": "реликвия", "MACE": "булава",
      "LONG_SWORD": "длинный меч", "SHIELD": "щит", "SWORD": "меч", "DAGGER": "кинжал", "HEAVY_CROSSBOW": "арбалет"}


def rows(name):
    d = json.loads((BAL / name).read_text(encoding="utf-8"))
    return d["rows"] if isinstance(d, dict) and "rows" in d else d


def fits(sizes, order):
    """'First free place' placement: how many cells are occupied. order: 'yx' — by rows, 'xy' — by columns."""
    g = [[False] * W for _ in range(H)]
    cells = [(x, y) for y in range(H) for x in range(W)] if order == "yx" else [(x, y) for x in range(W) for y in range(H)]
    for rx, cy in sizes:
        for x, y in cells:
            if x + rx <= W and y + cy <= H and not any(g[y + j][x + i] for i in range(rx) for j in range(cy)):
                for i in range(rx):
                    for j in range(cy):
                        g[y + j][x + i] = True
                break
    return sum(map(sum, g))


def main():
    rarity = {r["type_id"]: r.get("rarity", "").replace("EN_ITEM_RARITY_", "") for r in rows("UeSource_ItemType.json")}
    size = {r["type_id"]: (r["position_cfg"]["row_size"], r["position_cfg"]["column_size"])
            for r in rows("UeSource_ItemType.json") if r.get("position_cfg")}
    sub = {r["equip_id"]: r.get("equip_subtype", "").replace("EN_GEAR_EQUIP_SUBTYPE_", "") for r in rows("UeSource_Equipment.json")}
    known = {r["type_id"] for r in rows("item_type.json")}
    gear = {r["item_id"] for r in rows("gear_template.json")}
    s = TEMPLATE.read_text(encoding="utf-8")
    names = {int(m.group(1)): m.group(2) for m in re.finditer(r"\{ id = (\d+), w = \d+[^}]*\},\s*#\s*([^\n]*)", s)}
    if MARK in s:
        s = s[: s.index(MARK)]

    def variants(kind):
        ids = sorted(i for i, t in sub.items() if t == kind and rarity.get(i) == "EPIC" and i in known and i in gear
                     and str(i).startswith("18"))
        if not ids:
            sys.exit(f"no purple items of kind {kind}")
        return ids

    groups, pools = [], []
    for key, src, sid, pref, plan, title in CHESTS:
        order = [pref + t if plan == "armor" else t for t in ORDERS[plan]]
        kinds = list(dict.fromkeys(order))
        ids_of = {k: variants(k) for k in kinds}
        # check: sizes of all variants of a kind are the same, the placement fills 90 cells
        for k, ids in ids_of.items():
            if len({size[i] for i in ids}) != 1:
                sys.exit(f"{k}: variants have different sizes {[size[i] for i in ids]}")
        sizes = [size[ids_of[k][0]] for k in order]
        a, b = fits(sizes, "yx"), fits(sizes, "xy")
        if min(a, b) != W * H:
            sys.exit(f"{key}: the placement fills {a}/{b} cells of {W * H} — fix ORDERS")
        slots, seen = [], {}
        for k in order:
            label = RU[k.replace(pref, "") if pref else k]
            if k in ROTATE:
                n = seen.get(k, 0)
                seen[k] = n + 1
                i = ids_of[k][n % len(ids_of[k])]
                slots.append(f'  {{ pool = "pvp_{key}_{k.lower()}_{i}", chance = 100 }},  # {label}: {names.get(i, i)}\n')
            else:
                slots.append(f'  {{ pool = "pvp_{key}_{k.lower()}", chance = 100 }},  # {label}\n')
        for k in kinds:
            label = RU[k.replace(pref, "") if pref else k]
            if k in ROTATE:
                for i in ids_of[k]:
                    pools.append(f'[pools.pvp_{key}_{k.lower()}_{i}]\nname = "PVP {title}: {label} {i}"\nitems = [\n'
                                 f'  {{ id = {i}, w = 100 }},  # {names.get(i, k)}\n]\n')
            else:
                pools.append(f'[pools.pvp_{key}_{k.lower()}]\nname = "PVP {title}: {label}, фиолетовые"\nitems = [\n'
                             + "".join(f"  {{ id = {i}, w = 100 }},  # {names.get(i, k)}\n" for i in ids_of[k]) + "]\n")
        count = ", ".join(f'{RU[k.replace(pref, "") if pref else k]} {order.count(k)}'
                          + (f" (вариантов {len(ids_of[k])}, по очереди)" if k in ROTATE else "") for k in kinds)
        groups.append(f'[groups.pvp_{key}]\nname = "PVP-сундук: {title} — {src}"\nloot_types = [0, 1, 2, 3, 4, 5]\n'
                      f'sources = [\n  "{src}",  # {sid}\n]\n# {len(order)} предм., 90 клеток: {count}\nslots = [\n'
                      + "".join(slots) + "]\n")
        print(f"{key}: {len(order)} items — {count}")
    head = (MARK + " (пользовательские игры) — СГЕНЕРИРОВАНО tools/pvp_chests.py, правьте генератор.\n"
            "# Фиолетовая экипировка своего типа, 90 клеток (9×10), равномерно по видам, оружие — варианты по очереди.\n")
    TEMPLATE.write_text(s.rstrip("\n") + "\n" + head + "\n" + "\n".join(groups) + "\n" + "\n".join(pools), encoding="utf-8")
    print(f"written: {TEMPLATE}")


if __name__ == "__main__":
    main()
