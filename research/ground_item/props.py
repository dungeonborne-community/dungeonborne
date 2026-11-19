"""Asset export properties (CDO and fragments): python props.py <asset.json>"""
import json, sys

d = json.load(open(sys.argv[1], encoding='utf-8'))
ex = d['Exports']; imp = d['Imports']


def ref(i):
    try:
        i = int(i)
        if i > 0: return ex[i - 1]['ObjectName']
        if i < 0: return imp[-i - 1]['ObjectName']
    except Exception:
        return '?%s' % i
    return 'null'


def val(p):
    t = p.get('$type', '').split(',')[0].split('.')[-1]
    v = p.get('Value')
    if t == 'ObjectPropertyData':
        return ref(v)
    if t == 'SoftObjectPropertyData' and isinstance(v, dict):
        ap = v.get('AssetPath') or {}
        return '%s.%s' % (ap.get('PackageName'), ap.get('AssetName'))
    if isinstance(v, list):
        out = []
        for c in v:
            if isinstance(c, dict) and '$type' in c:
                out.append('%s=%s' % (c.get('Name'), val(c)))
            else:
                out.append(str(c))
        return '{' + ', '.join(out) + '}'
    if isinstance(v, dict):
        return json.dumps(v, ensure_ascii=False)[:300]
    return str(v)


sp = d.get('SoftObjectPathList') or []
if sp:
    print('SoftObjectPathList:')
    for s in sp:
        ap = s.get('AssetPath') or {}
        print('   ', ap.get('PackageName'), ap.get('AssetName'))
print('Imports with paths:', [i['ObjectName'] for i in imp if i.get('ClassName') in ('Package',)][:60])
for e in ex:
    data = e.get('Data') or []
    print('=== %s  (class %s, super %s)' % (e['ObjectName'], ref(e.get('ClassIndex')), ref(e.get('SuperStruct', e.get('SuperIndex')))))
    for p in data:
        if isinstance(p, dict):
            print('    %s = %s' % (p.get('Name'), val(p)[:400]))
