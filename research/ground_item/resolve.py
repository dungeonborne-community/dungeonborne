"""Summary: which mesh an item on the ground uses (PickupMesh, including inheritance) vs. its icon and its look on the hero.
python resolve.py idj > pickup_table.tsv"""
import json, os, sys

D = sys.argv[1]
info = {}


def load(path):
    d = json.load(open(path, encoding='utf-8'))
    ex = d['Exports']; imp = d['Imports']

    def ref(i):
        i = int(i)
        if i > 0: return ex[i - 1]['ObjectName']
        if i < 0: return imp[-i - 1]['ObjectName']
        return None

    def soft(v):
        if isinstance(v, dict):
            ap = v.get('AssetPath') or {}
            return ap.get('PackageName')
        return None

    r = {'file': os.path.basename(path)}
    for e in ex:
        n = e['ObjectName']
        if n.endswith('_C') and not n.startswith('Default__') and 'SuperStruct' in e:
            r['cls'] = n
            r['super'] = ref(e['SuperStruct'])
    for e in ex:
        n = e['ObjectName']; data = e.get('Data') or []
        if n.startswith('Default__'):
            for p in data:
                if p.get('Name') == 'ItemDefId': r['id'] = p.get('Value')
        elif n.startswith('InventoryFragment_PickupMesh'):
            for p in data:
                if p.get('Name') == 'StaticMesh':
                    v = p.get('Value')
                    if isinstance(v, list):
                        for c in v:
                            if isinstance(c, dict) and 'Value' in c: r['mesh'] = soft(c['Value'])
                    else:
                        r['mesh'] = soft(v)
                if p.get('Name') == 'CollectableClass': r['coll'] = ref(p.get('Value'))
        elif n.startswith('InventoryFragment_InventoryIcon'):
            for p in data:
                if p.get('Name') == 'Brush':
                    for c in p.get('Value') or []:
                        if isinstance(c, dict) and c.get('Name') == 'ResourceObject': r['icon'] = ref(c.get('Value'))
                if p.get('Name') == 'DisplayQuality': r['q'] = str(p.get('Value')).replace('TgfEquipQuality', '')
        elif n.startswith('InventoryFragment_EquippableItem'):
            for p in data:
                if p.get('Name') == 'EquipmentDefinition': r['aid'] = ref(p.get('Value'))
    return r


for f in os.listdir(D):
    if not f.endswith('.json') or os.path.getsize(os.path.join(D, f)) == 0: continue
    try:
        r = load(os.path.join(D, f))
    except Exception as e:
        print('ERR', f, e, file=sys.stderr); continue
    if 'cls' in r: info[r['cls']] = r


def eff(c, k):
    seen = 0
    while c in info and seen < 20:
        if info[c].get(k): return info[c][k], c
        c = info[c].get('super'); seen += 1
    return None, None


print('\t'.join(['id', 'class', 'quality', 'pickup_mesh', 'mesh_from', 'collectable', 'icon', 'aid']))
for c, r in sorted(info.items(), key=lambda x: str(x[1].get('id'))):
    if not r.get('id'): continue
    m, mf = eff(c, 'mesh'); col, _ = eff(c, 'coll'); ic, _ = eff(c, 'icon'); q, _ = eff(c, 'q'); a, _ = eff(c, 'aid')
    print('\t'.join(str(x) for x in [r['id'], c, q or '', (m or '').split('/')[-1], 'own' if mf == c else (mf or '-'), col or '', ic or '', a or '']))
