"""A mini disassembler of UE5.1 Kismet bytecode (disk format) that counts in-memory offsets.
Memory: an object/property pointer is 8 bytes, FScriptName 12; disk: FPackageIndex 4, FName 8, FFieldPath 4+8n+4."""
import struct

OBJ_MEM, PROP_MEM, NAME_MEM = 8, 8, 12

SIMPLE = {0x0B: "Nothing", 0x15: "EndParmValue", 0x16: "EndFunctionParms", 0x17: "Self", 0x25: "IntZero", 0x26: "IntOne",
          0x27: "True", 0x28: "False", 0x2A: "NoObject", 0x2D: "NoInterface", 0x30: "EndStructConst", 0x32: "EndArray",
          0x3A: "EndSet", 0x3C: "EndMap", 0x3E: "EndSetConst", 0x40: "EndMapConst", 0x4D: "PopExecutionFlow",
          0x50: "Breakpoint", 0x53: "EndOfScript", 0x5A: "WireTracepoint", 0x5E: "Tracepoint", 0x66: "EndArrayConst"}


class Reader:
    def __init__(self, data, names):
        self.d, self.p, self.mem, self.names = data, 0, 0, names

    def u8(self):
        v = self.d[self.p]; self.p += 1; self.mem += 1; return v

    def i32(self):
        v = struct.unpack_from("<i", self.d, self.p)[0]; self.p += 4; self.mem += 4; return v

    def raw(self, n):
        v = self.d[self.p:self.p + n]; self.p += n; self.mem += n; return v

    def obj(self):
        v = struct.unpack_from("<i", self.d, self.p)[0]; self.p += 4; self.mem += OBJ_MEM; return v

    def name(self):
        i, n = struct.unpack_from("<ii", self.d, self.p); self.p += 8; self.mem += NAME_MEM
        return self.names[i] + (f"_{n-1}" if n else "")

    def prop(self):
        cnt = struct.unpack_from("<i", self.d, self.p)[0]; self.p += 4
        path = []
        for _ in range(cnt):
            i, n = struct.unpack_from("<ii", self.d, self.p); self.p += 8
            path.append(self.names[i] + (f"_{n-1}" if n else ""))
        owner = struct.unpack_from("<i", self.d, self.p)[0]; self.p += 4
        self.mem += PROP_MEM
        return ("/".join(path), owner)


def expr(r, out, depth=0):
    """reads one expression; out — a list of (mem_start, disk_start, depth, text)"""
    m0, d0 = r.mem, r.p
    op = r.u8()
    line = None

    def emit(t):
        out.append((m0, d0, depth, t))

    if op in SIMPLE:
        emit(SIMPLE[op]); return op
    if op in (0x00, 0x01, 0x02, 0x48, 0x6C, 0x33):
        emit(f"{ {0:'LocalVariable',1:'InstanceVariable',2:'DefaultVariable',0x48:'LocalOutVariable',0x6C:'ClassSparseDataVariable',0x33:'PropertyConst'}[op] } {r.prop()}"); return op
    if op == 0x04:
        emit("Return"); expr(r, out, depth + 1); return op
    if op == 0x06:
        emit(f"Jump {r.i32()}"); return op
    if op == 0x07:
        emit(f"JumpIfNot {r.i32()}"); expr(r, out, depth + 1); return op
    if op == 0x0F:
        emit(f"Let {r.prop()}"); expr(r, out, depth + 1); expr(r, out, depth + 1); return op
    if op in (0x14, 0x43, 0x44, 0x5C, 0x5F, 0x60, 0x62, 0x6B):
        emit({0x14: "LetBool", 0x43: "LetMulticastDelegate", 0x44: "LetDelegate", 0x5C: "AddMulticastDelegate", 0x5F: "LetObj",
              0x60: "LetWeakObjPtr", 0x62: "RemoveMulticastDelegate", 0x6B: "ArrayGetByRef"}[op])
        expr(r, out, depth + 1); expr(r, out, depth + 1); return op
    if op in (0x12, 0x19, 0x1A):
        emit({0x12: "ClassContext", 0x19: "Context", 0x1A: "Context_FailSilent"}[op])
        expr(r, out, depth + 1)
        skip = r.i32(); rv = r.prop()
        out.append((r.mem, r.p, depth + 1, f"(skip={skip} rvalue={rv})"))
        expr(r, out, depth + 1); return op
    if op in (0x1B, 0x45, 0x4B):
        n = r.name()
        if op == 0x4B:
            emit(f"InstanceDelegate {n}"); return op
        emit(f"{'VirtualFunction' if op == 0x1B else 'LocalVirtualFunction'} {n}")
        while expr(r, out, depth + 1) != 0x16:
            pass
        return op
    if op in (0x1C, 0x46, 0x68, 0x63):
        f = r.obj()
        emit(f"{ {0x1C:'FinalFunction',0x46:'LocalFinalFunction',0x68:'CallMath',0x63:'CallMulticastDelegate'}[op] } obj={f}")
        while expr(r, out, depth + 1) != 0x16:
            pass
        return op
    if op == 0x1D:
        emit(f"IntConst {r.i32()}"); return op
    if op == 0x1E:
        emit(f"FloatConst {struct.unpack('<f', r.raw(4))[0]}"); return op
    if op == 0x37:
        emit(f"DoubleConst {struct.unpack('<d', r.raw(8))[0]}"); return op
    if op in (0x35, 0x36):
        emit(f"Int64Const {struct.unpack('<q', r.raw(8))[0]}"); return op
    if op == 0x1F:
        s = bytearray()
        while True:
            c = r.raw(1)[0]
            if c == 0: break
            s.append(c)
        emit(f"StringConst {s.decode('latin1')!r}"); return op
    if op == 0x34:
        s = bytearray()
        while True:
            c = r.raw(2)
            if c == b"\0\0": break
            s += c
        emit(f"UnicodeStringConst {s.decode('utf-16-le')!r}"); return op
    if op == 0x20:
        emit(f"ObjectConst obj={r.obj()}"); return op
    if op == 0x21:
        emit(f"NameConst {r.name()}"); return op
    if op in (0x22, 0x23):
        emit(f"{'RotationConst' if op == 0x22 else 'VectorConst'} {struct.unpack('<3d', r.raw(24))}"); return op
    if op == 0x41:
        emit(f"Vector3fConst {struct.unpack('<3f', r.raw(12))}"); return op
    if op == 0x2B:
        r.raw(80); emit("TransformConst"); return op
    if op in (0x24, 0x2C):
        emit(f"ByteConst {r.u8()}"); return op
    if op == 0x29:
        t = r.u8(); emit(f"TextConst type={t}")
        if t == 1:
            for _ in range(3): expr(r, out, depth + 1)
        elif t in (2, 3):
            expr(r, out, depth + 1)
        elif t == 4:
            r.obj(); expr(r, out, depth + 1); expr(r, out, depth + 1)
        return op
    if op in (0x2E, 0x13, 0x52, 0x54, 0x55):
        emit(f"{ {0x2E:'DynamicCast',0x13:'MetaCast',0x52:'ObjToInterfaceCast',0x54:'CrossInterfaceCast',0x55:'InterfaceToObjCast'}[op] } obj={r.obj()}")
        expr(r, out, depth + 1); return op
    if op == 0x38:
        emit(f"PrimitiveCast {r.u8()}"); expr(r, out, depth + 1); return op
    if op == 0x2F:
        st = r.obj(); sz = r.i32(); emit(f"StructConst obj={st} size={sz}")
        while expr(r, out, depth + 1) != 0x30:
            pass
        return op
    if op == 0x31:
        emit("SetArray"); expr(r, out, depth + 1)
        while expr(r, out, depth + 1) != 0x32:
            pass
        return op
    if op == 0x65:
        p = r.prop(); n = r.i32(); emit(f"ArrayConst {p} n={n}")
        while expr(r, out, depth + 1) != 0x66:
            pass
        return op
    if op == 0x42:
        emit(f"StructMemberContext {r.prop()}"); expr(r, out, depth + 1); return op
    if op == 0x18:
        emit(f"Skip {r.i32()}"); expr(r, out, depth + 1); return op
    if op == 0x4C:
        emit(f"PushExecutionFlow {r.i32()}"); return op
    if op in (0x4E, 0x4F, 0x51, 0x5D, 0x67, 0x6D):
        emit({0x4E: "ComputedJump", 0x4F: "PopExecutionFlowIfNot", 0x51: "InterfaceContext", 0x5D: "ClearMulticastDelegate",
              0x67: "SoftObjectConst", 0x6D: "FieldPathConst"}[op]); expr(r, out, depth + 1); return op
    if op == 0x5B:
        emit(f"SkipOffsetConst {r.i32()}"); return op
    if op == 0x61:
        emit(f"BindDelegate {r.name()}"); expr(r, out, depth + 1); expr(r, out, depth + 1); return op
    if op == 0x64:
        emit(f"LetValueOnPersistentFrame {r.prop()}"); expr(r, out, depth + 1); return op
    if op == 0x69:
        n = struct.unpack_from("<H", r.d, r.p)[0]; r.p += 2; r.mem += 2
        end = r.i32(); emit(f"SwitchValue n={n} end={end}"); expr(r, out, depth + 1)
        for _ in range(n):
            expr(r, out, depth + 1); r.i32(); expr(r, out, depth + 1)
        expr(r, out, depth + 1); return op
    raise ValueError(f"unknown opcode 0x{op:02X} at disk {d0} (memory {m0})")


def disasm(script, names):
    r = Reader(script, names)
    out = []
    while r.p < len(script):
        expr(r, out)
    return out, r.mem
