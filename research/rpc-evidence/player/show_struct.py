"""Prints struct/enum definitions from src/generated/tgf.rs by Rust name: python -I show_struct.py Name1 Name2 ..."""
import re, sys, pathlib
src = pathlib.Path(__file__).resolve().parents[3] / "src" / "generated" / "tgf.rs"
text = src.read_text(encoding="utf-8")
lines = text.splitlines()
for name in sys.argv[1:]:
    pat = re.compile(r"^\s*pub (struct|enum) " + re.escape(name) + r"\b")
    for i, l in enumerate(lines):
        if pat.match(l):
            depth = 0; out = []
            for j in range(i, len(lines)):
                s = lines[j]
                out.append(s)
                depth += s.count("{") - s.count("}")
                if depth <= 0 and "{" in "".join(out):
                    break
            print("\n".join(x for x in out if "#[prost" not in x and x.strip()))
            print()
            break
    else:
        print(f"// {name}: not found\n")
