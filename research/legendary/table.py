import json,csv,sys,collections
B='<repo>/'
J=lambda f: json.load(open(B+'ExcelConfigjson/'+f+'.json',encoding='utf-8'))
ST='<data>/new_class/research/csv/Dungeonborne/Content/AdditionalData/StringTable/'
def strings(n):
    return {r['Key']:r['SourceString'] for r in csv.DictReader(open(ST+n,encoding='utf-8'))}
aff=strings('AffixStringTable.csv'); itm=strings('ItemStringTable.csv')
it=J('UeSource_ItemType')
at={x['item_id']:x for x in J('affix_template')}
season={x['item_id']:x for x in json.load(open(B+'seasons/playtest/balance/affix_template.json',encoding='utf-8'))['rows']}
lr={x['affix_alias_id']:x for x in J('affix_legendary_rule')}
bd={x['ability_set_id']:x for x in J('battle_ability_set_description')}
ab={x['ue_source_id']:x['ue_path'].split('.')[-1].rstrip("'") for x in J('UeSource_AbilitySet')}
gt={x['item_id'] for x in J('gear_template')}
eq={x.get('item_id',x.get('ue_source_id')):x for x in J('UeSource_Equipment')}
def info(t):
    if not t: return None
    d=t['data']; leg=[r['affix_alias_id'] for r in d.get('legendary_rule',[])]
    gs=[]
    for r in d.get('fixed_rule',[])+d.get('random_rule',[]):
        if r.get('gear_set') or r.get('gear_set_id') or 'gear_set' in json.dumps(r): gs.append(r['affix_type'][21:])
    return leg,gs
rows=[]
for x in it:
    if x['rarity'] not in ('EN_ITEM_RARITY_LEGENDARY','EN_ITEM_RARITY_UNIQUE') or 'EQUIP' not in x['item_type']: continue
    i=x['type_id']; b=info(at.get(i)); s=info(season.get(i))
    leg = b[0] if b else []
    out=[]
    for a in leg:
        r=lr.get(a); asid=r['offset']['ability_set_id'] if r else None
        out.append(dict(alias=a, rule=bool(r), asid=asid, desc=bd.get(asid,{}).get('description') if asid else None,
             text=aff.get('affix_legendary_%s'%asid) if asid else None, ability=ab.get(asid)))
    rows.append(dict(id=i, rar=x['rarity'][15:], kind=x['item_type'][17:], bp=x['ue_path'].split('/')[-1].split('.')[0],
        name=itm.get('Item_Name_%d'%i), in_at=b is not None, in_gt=i in gt, gearset=b[1] if b else None,
        season_diff=(s!=b), leg=out))
json.dump(rows,open(sys.argv[1],'w',encoding='utf-8'),ensure_ascii=False,indent=1)
for r in rows:
    l=r['leg']
    print(r['id'],r['rar'][:4],r['kind'][:6],r['bp'][:28].ljust(28),(r['name'] or '-')[:10].ljust(10),'AT' if r['in_at'] else '--','GT' if r['in_gt'] else '--',
          'set:'+','.join(r['gearset']) if r['gearset'] else '', 'SEASONDIFF' if r['season_diff'] else '',
          ' | '.join('%s rule=%s as=%s abil=%s txt=%s'%(e['alias'],int(e['rule']),e['asid'],(e['ability'] or 'NONE')[:30],(e['text'] or 'NONE')[:20]) for e in l) or 'NO_LEGENDARY_RULE')
