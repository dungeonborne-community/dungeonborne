import sys, json
dmp, rj, out = sys.argv[1:4]
r = json.load(open(rj)); base = r['base']
img = bytearray(r['size'])
f = open(dmp, 'rb')
for start, size, off in r['ranges']:
    lo = max(start, base); hi = min(start+size, base+r['size'])
    f.seek(off + (lo-start)); img[lo-base:hi-base] = f.read(hi-lo)
open(out, 'wb').write(img)
print('written', len(img))
