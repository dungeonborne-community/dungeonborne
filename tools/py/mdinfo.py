"""Game crash minidump (UEMinidump.dmp, *.dmp): exception, registers, module+offset of the address, whether rip has code.

    python -I tools/py/mdinfo.py "<Crashes folder>/*/UEMinidump.dmp" [--probe 0x142f60246 ...]
--probe — show memory bytes at the addresses (if they are in the dump). Reads only the dump file: it does not touch the live memory
of the process (docs/RESEARCH.md). The module map and cutting out the exe image for Ghidra — research/ghidra/scripts.
"""
import struct, sys, glob, os

REGS = ["Rax","Rcx","Rdx","Rbx","Rsp","Rbp","Rsi","Rdi","R8","R9","R10","R11","R12","R13","R14","R15","Rip"]

def rd(f, off, n):
    f.seek(off); return f.read(n)

def u32(b, o=0): return struct.unpack_from("<I", b, o)[0]
def u64(b, o=0): return struct.unpack_from("<Q", b, o)[0]

def mdstr(f, rva):
    n = u32(rd(f, rva, 4)); return rd(f, rva + 4, n).decode("utf-16-le", "replace")

def analyze(path, probe):
    f = open(path, "rb")
    hdr = rd(f, 0, 32)
    nstreams, dir_rva = u32(hdr, 8), u32(hdr, 12)
    streams = {}
    for i in range(nstreams):
        e = rd(f, dir_rva + i * 12, 12)
        streams.setdefault(u32(e), []).append((u32(e, 4), u32(e, 8)))
    print("==", path, "size", os.path.getsize(path))
    # modules
    mods = []
    if 4 in streams:
        sz, rva = streams[4][0]; n = u32(rd(f, rva, 4))
        for i in range(n):
            m = rd(f, rva + 4 + i * 108, 108)
            base, size, name = u64(m), u32(m, 8), mdstr(f, u32(m, 20))
            mods.append((base, size, name))
    def modof(a):
        for b, s, n in mods:
            if b <= a < b + s: return "%s+0x%x" % (os.path.basename(n), a - b)
        return "?"
    # memory ranges
    ranges = []
    if 5 in streams:
        sz, rva = streams[5][0]; n = u32(rd(f, rva, 4))
        for i in range(n):
            d = rd(f, rva + 4 + i * 16, 16)
            ranges.append((u64(d), u32(d, 8), u32(d, 12)))
    if 9 in streams:
        sz, rva = streams[9][0]; d = rd(f, rva, 16); n, base_rva = u64(d), u64(d, 8)
        cur = base_rva
        for i in range(n):
            e = rd(f, rva + 16 + i * 16, 16); start, size = u64(e), u64(e, 8)
            ranges.append((start, size, cur)); cur += size
    def readmem(a, n):
        for s, sz, r in ranges:
            if s <= a and a + n <= s + sz: return rd(f, r + (a - s), n)
        return None
    print("memory ranges:", len(ranges), "total bytes", sum(r[1] for r in ranges))
    # exception
    if 6 in streams:
        sz, rva = streams[6][0]; e = rd(f, rva, sz)
        tid = u32(e); code = u32(e, 8); addr = u64(e, 8 + 16)
        nparams = u32(e, 8 + 24); params = [u64(e, 8 + 32 + 8 * k) for k in range(nparams)]
        ctx_size, ctx_rva = u32(e, 8 + 152), u32(e, 8 + 156)
        print("exception tid=%d code=0x%08x addr=0x%x (%s) params=%s" % (tid, code, addr, modof(addr), [hex(p) for p in params]))
        ctx = rd(f, ctx_rva, ctx_size)
        regs = {nm: u64(ctx, 0x78 + 8 * k) for k, nm in enumerate(REGS)}
        print("  " + " ".join("%s=%x" % (k, v) for k, v in regs.items()))
        rip = regs["Rip"]
        code_bytes = readmem(rip - 0x80, 0x100) or readmem(rip, 0x10)
        print("  code at rip in dump:", "YES %d bytes" % len(code_bytes) if code_bytes else "NO")
        if code_bytes: print("  ", code_bytes.hex())
    for p in probe:
        b = readmem(p, 0x92)
        print("  probe 0x%x (%s): %s" % (p, modof(p), b.hex() if b else "not in dump"))
    print("  main module:", [(hex(b), hex(s), os.path.basename(n)) for b, s, n in mods[:2]])

if __name__ == "__main__":
    import argparse
    ap = argparse.ArgumentParser(description="minidump: exception, registers, module+offset")
    ap.add_argument("dumps", nargs="+", help="dump files or glob patterns")
    ap.add_argument("--probe", nargs="*", default=[], help="addresses (hex) to dump bytes from, if present")
    a = ap.parse_args()
    probes = [int(x, 16) for x in a.probe]
    for p in a.dumps:
        for path in sorted(glob.glob(p)):
            try: analyze(path, probes)
            except Exception as ex: print("ERR", path, ex)
