import sys
img = open(sys.argv[1],'rb').read(); B=0x140000000
for a in sys.argv[2:]:
    o=int(a,16)-B; b=img[o:o+200]
    w = b[1:2]==b'\0'
    if w: s=b.decode('utf-16-le',errors='replace').split('\0')[0]
    else: s=b.split(b'\0')[0].decode(errors='replace')
    print(a, 'W' if w else 'A', repr(s))
