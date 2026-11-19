# Extracting embedded FileDescriptorProto from the exe: we look for the proto file name, around it — the serialized descriptor.
import sys, re
from google.protobuf import descriptor_pb2

d = open(sys.argv[1], "rb").read()
want = sys.argv[2:]  # substrings of .proto names


def varint(b, i):
    r = s = 0
    while True:
        x = b[i]; i += 1
        r |= (x & 0x7F) << s; s += 7
        if x < 0x80: return r, i


def to_text(fd):
    out = [f'// {fd.name}  package {fd.package}']
    for dep in fd.dependency: out.append(f'import "{dep}";')
    T = {1: "double", 2: "float", 3: "int64", 4: "uint64", 5: "int32", 8: "bool", 9: "string", 12: "bytes", 13: "uint32", 17: "sint32", 18: "sint64"}
    def msg(m, ind=""):
        out.append(f"{ind}message {m.name} {{")
        for n in m.nested_type: msg(n, ind + "  ")
        for e in m.enum_type:
            out.append(f"{ind}  enum {e.name} {{ " + " ".join(f"{v.name}={v.number};" for v in e.value) + " }")
        for f in m.field:
            lab = "repeated " if f.label == 3 else ""
            t = f.type_name or T.get(f.type, str(f.type))
            out.append(f"{ind}  {lab}{t} {f.name} = {f.number};")
        out.append(f"{ind}}}")
    for e in fd.enum_type:
        out.append(f"enum {e.name} {{ " + " ".join(f"{v.name}={v.number};" for v in e.value) + " }")
    for m in fd.message_type: msg(m)
    for s in fd.service:
        out.append(f"service {s.name} {{")
        for mm in s.method: out.append(f"  rpc {mm.name}({mm.input_type}) returns ({mm.output_type});")
        out.append("}")
    return "\n".join(out)


seen = set()
for m in re.finditer(rb"\x0a([\x05-\x7f])([a-zA-Z0-9_/\.]+\.proto)", d):
    name = m.group(2).decode()
    if len(m.group(2)) != m.group(1)[0] or not any(w in name for w in want) or name in seen:
        continue
    start = m.start()
    # length unknown: try to parse, extending the end until the first success with the maximum number of messages
    best = None
    for end in range(start + 50, min(len(d), start + 400000), 1):
        pass
    # simpler: the descriptor usually lies whole; parse greedily field by field
    i = start; ok_end = start
    try:
        while i < len(d):
            key, j = varint(d, i)
            fn, wt = key >> 3, key & 7
            if fn == 0 or fn > 20 or wt not in (0, 2): break
            if wt == 0:
                _, j = varint(d, j)
            else:
                ln, j = varint(d, j)
                if ln > 500000: break
                j += ln
            i = j; ok_end = i
    except IndexError:
        pass
    fd = descriptor_pb2.FileDescriptorProto()
    try:
        fd.ParseFromString(d[start:ok_end])
    except Exception as ex:
        print("// fail", name, ex); continue
    seen.add(name)
    print(to_text(fd)); print()
