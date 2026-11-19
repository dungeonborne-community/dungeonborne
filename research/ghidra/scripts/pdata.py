import sys, struct, bisect
img = open(sys.argv[1], 'rb').read(); B = 0x140000000
pe = struct.unpack_from('<I', img, 0x3c)[0]
optsz = struct.unpack_from('<H', img, pe+20)[0]
dd = pe+24+112
erva, esz = struct.unpack_from('<II', img, dd+3*8)
ents = [struct.unpack_from('<III', img, erva+i*12) for i in range(esz//12)]
starts = [e[0] for e in ents]
print('pdata entries', len(ents))
for a in sys.argv[2:]:
    r = int(a, 16) - B
    i = bisect.bisect_right(starts, r) - 1
    s, e, u = ents[i]
    # follow chained unwind info to the primary function
    while True:
        flags = img[u] >> 3
        if not (flags & 4): break
        cnt = img[u+2]; off = u + 4 + ((cnt+1)&~1)*2
        s, e, u = struct.unpack_from('<III', img, off)
    print(f'{a}: func {B+s:#x}..{B+e:#x}' if s <= r < e or True else '')
