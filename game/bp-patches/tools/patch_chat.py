"""ChatFix (02.11): show the world chat channel in the chat panel. ChatChannelConfigDataTable, row Chat_Channel_World:
bShowInPanel false -> true. UAssetGUI reads the table as a RawExport, so the edit is on raw bytes of export 0:
- the row's unversioned property header `80 0f 20 02` -> `00 0f 02` (bShowInPanel moves from the zero mask to the
  serialized properties);
- the value byte `01` goes right after SendInterval (float 1.2 = `9a 99 99 3f`).
The export keeps its size (one byte out of the header, one byte in). Every step checks what it expects.
py -3 -I patch_chat.py <in.json> <out.json>"""
import base64, json, sys

src, dst = sys.argv[1:3]
d = json.load(open(src, encoding="utf-8-sig"))
exp = d["Exports"][0]
data = bytearray(base64.b64decode(exp["Data"]))
size = len(data)

name = data.find(b"Chat_Channel_World")
if name < 0 or data.find(b"Chat_Channel_World", name + 1) >= 0:
    sys.exit("expected exactly one Chat_Channel_World row")
head = data.rfind(bytes.fromhex("800f2002"), 0, name)
if head < 0 or name - head > 40:
    sys.exit("the row header 80 0f 20 02 is not where expected - already patched or another game build?")
interval = data.find(bytes.fromhex("9a99993f00000000"), name)
if interval < 0 or interval - name > 40:
    sys.exit("SendInterval (1.2) after the row name is not where expected")

data[interval + 4:interval + 4] = b"\x01"      # bShowInPanel = true, right after SendInterval
data[head:head + 4] = bytes.fromhex("000f02")  # header: bShowInPanel is serialized now
if len(data) != size:
    sys.exit(f"export size changed: {size} -> {len(data)}")
exp["Data"] = base64.b64encode(bytes(data)).decode()
json.dump(d, open(dst, "w", encoding="utf-8"), indent=2)
print("ok: Chat_Channel_World bShowInPanel -> true")
