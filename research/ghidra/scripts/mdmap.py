import sys, struct
p = sys.argv[1]
f = open(p, 'rb')
hdr = f.read(32)
sig, ver, nstreams, dirrva = struct.unpack_from('<IIII', hdr)
f.seek(dirrva); dd = f.read(12*nstreams)
streams = {}
for i in range(nstreams):
    t, sz, rva = struct.unpack_from('<III', dd, i*12)
    streams[t] = (sz, rva)
print('streams', sorted(streams))
def rd(rva, n): f.seek(rva); return f.read(n)
def mstr(rva):
    n = struct.unpack('<I', rd(rva,4))[0]; return rd(rva+4, n).decode('utf-16-le')
sz, rva = streams[4]  # ModuleList
cnt = struct.unpack('<I', rd(rva,4))[0]
blob = rd(rva+4, cnt*108)
mods = []
for i in range(cnt):
    base, size, _, _, namerva = struct.unpack_from('<QIIII', blob, i*108)
    name = mstr(namerva); mods.append((base, size, name))
for b, s, n in mods:
    if 'Dungeonborne' in n or 'UE4SS' in n: print(f'{b:#x} {s:#x} {n}')
sz, rva = streams[9]  # Memory64List
n, baserva = struct.unpack('<QQ', rd(rva,16))
descs = rd(rva+16, n*16)
exe = [m for m in mods if m[2].lower().endswith('dungeonborne.exe') or 'Win64-Shipping' in m[2] or m[2].lower().endswith('.exe')][0]
print('exe', hex(exe[0]), hex(exe[1]), exe[2])
off = baserva; cov = 0; ranges = []
for i in range(n):
    start, size = struct.unpack_from('<QQ', descs, i*16)
    if start < exe[0]+exe[1] and start+size > exe[0]:
        ranges.append((start, size, off)); cov += size
    off += size
print('ranges in exe', len(ranges), 'covered', hex(cov), 'of', hex(exe[1]))
import json
json.dump({'base': exe[0], 'size': exe[1], 'ranges': ranges}, open(sys.argv[2], 'w'))
