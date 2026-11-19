"""All equipment on the auction (playtest 06.11): adds missing items to balance/trade_commodity.json for both markets.

Run from the backups root:  python -I tools/auction_all.py   then  cargo xtask balance build

- Re-running duplicates nothing: items already listed are skipped.
- Items without UeSource_ItemType.rarity are not added: they have no data for the client.
- Row settings (tax, limits, duration) — from an existing row of the same rarity on the same market, for ROTTED and NORMAL — from UNCOMMON.
- Price — the average price of the rarity: existing items have their own price per slot, so the reference one is taken from PRICE.
- id: as for existing ones, item_id * 10; relics 220… do not fit into int32 that way, for them id = item_id.
"""
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, W  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)
import json

PRICE = {
    'EN_ITEM_RARITY_ROTTED': 200,
    'EN_ITEM_RARITY_NORMAL': 400,
    'EN_ITEM_RARITY_UNCOMMON': 900,
    'EN_ITEM_RARITY_ENHANCED': 2700,
    'EN_ITEM_RARITY_EPIC': 9000,
    'EN_ITEM_RARITY_EXCEPTIONAL': 40000,
    'EN_ITEM_RARITY_LEGENDARY': 200000,
    'EN_ITEM_RARITY_UNIQUE': 1000000,
}
TEMPLATE_RARITY = {'EN_ITEM_RARITY_ROTTED': 'EN_ITEM_RARITY_UNCOMMON', 'EN_ITEM_RARITY_NORMAL': 'EN_ITEM_RARITY_UNCOMMON'}
MARKETS = (1, 2)
INT32_MAX = 2147483647


def load(name):
    return json.load(open(str(W(f'balance/{name}.json')), encoding='utf-8'))


def rows(name):
    return load(name)['rows']


doc = load('trade_commodity')
table = doc['rows']
rarity = {r['type_id']: r.get('rarity') for r in rows('UeSource_ItemType')}
gear = [r['type_id'] for r in rows('item_type') if r.get('item_major_type') == 'EN_ITEM_MAJOR_TYPE_EQUIPMENT']
listed = {(r['market_id'], r['item_id']) for r in table}
ids = {(r['market_id'], r['id']) for r in table}

templates = {}
for r in table:
    templates.setdefault((r['market_id'], rarity.get(r['item_id'])), r)

added, skipped = 0, 0
for item in sorted(gear):
    rar = rarity.get(item)
    if not rar or rar not in PRICE:
        skipped += 1
        continue
    for market in MARKETS:
        if (market, item) in listed:
            continue
        tpl = templates[(market, TEMPLATE_RARITY.get(rar, rar))]
        row_id = item * 10 if item * 10 <= INT32_MAX else item
        assert (market, row_id) not in ids, (market, row_id)
        row = dict(tpl)
        row.update(id=row_id, item_id=item, price_def=PRICE[rar], is_on_shelf=True)
        table.append(row)
        ids.add((market, row_id))
        added += 1

table.sort(key=lambda r: (r['market_id'] != 2, r['id']))
doc['header']['count'] = len(table)
for source in doc['header'].get('data_source', []):
    source['count'] = len(table)
with open(str(W('balance/trade_commodity.json')), 'w', encoding='utf-8', newline='\n') as f:
    json.dump(doc, f, ensure_ascii=False, indent=2)
    f.write('\n')
print(f'rows added {added}, items without rarity skipped {skipped}, total rows {len(table)}')
