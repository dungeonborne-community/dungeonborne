"""Summary of luac -l -l: per function, source lines and references (field names/strings/numbers). python lsum.py file.dis.txt"""
import re
import sys

t = open(sys.argv[1], encoding='utf-8', errors='replace').read()
HEAD = re.compile(r'(?:main|function) <.*:(\d+),(\d+)>')
CONST = re.compile(r';\s*(?:_ENV\s+)?("(?:[^"\\]|\\.)*"|-?\d+(?:\.\d+)?)')
for b in re.split(r'(?=^(?:main|function) <)', t, flags=re.M):
    m = HEAD.match(b)
    if not m:
        continue
    body = b.split('\nconstants')[0]
    seen = []
    for k in CONST.findall(body):
        if k not in seen:
            seen.append(k)
    print('[%s-%s] %s' % (m.group(1), m.group(2), ' '.join(seen)[:1500]))
