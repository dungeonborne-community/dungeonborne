import sys, struct, re
img = open(sys.argv[1], 'rb').read(); B = 0x140000000
def va(o): return B+o
def find_all(b):
    i = img.find(b); r = []
    while i != -1: r.append(i); i = img.find(b, i+1)
    return r
for s in sys.argv[2:]:
    for enc, b in (('a', s.encode()+b'\0'), ('w', s.encode('utf-16-le')+b'\0\0')):
        for o in find_all(b):
            t = va(o); print(f'== {s} [{enc}] at {t:#x}')
            # absolute pointers (UFunction name tables)
            for p in find_all(struct.pack('<Q', t)):
                nxt = struct.unpack_from('<Q', img, p+8)[0]
                print(f'   ptr at {va(p):#x}  next qword {nxt:#x}')
            # rip-relative lea/mov in code section (first .std 0x1000..0x67bf640)
            code = img[0x1000:0x67bf640]
            for m in re.finditer(rb'[\x48\x4c][\x8d\x8b][\x05\x0d\x15\x1d\x25\x2d\x35\x3d]', code):
                i = m.start()
                d = struct.unpack_from('<i', code, i+3)[0]
                if 0x1000+i+7+d == o:
                    print(f'   code ref at {va(0x1000+i):#x}')
