# research — research notes and scripts

One folder per question: what we found, how to verify it, analysis scripts. Heavy research data (exports, BP,
dumps) lives in `home\re\research`, FModel exports in `home\dumps\fmodel`, not in git.

The client exe is analysed in **Ghidra** (+ JDK 21), an external program a person installs; the path is `config/local.toml
[tools] ghidra`, whether it is present: `cargo ws doctor`. Decrypting paks with the key and analysing the INTL login: only when the owner says so.

| Folder | Topic |
|---|---|
| `rpc-evidence/` | protocol records per RPC (`tools/py/rpc_evidence.py`) |
| `battleroom`, `hallleave`, `NETMODE_MAP.md` | match rooms, leaving the hall, net modes |
| `loot`, `ground_item`, `legendary`, `merchant_prices` | loot, items on the ground, legendaries, merchant prices |
| `castle_spawn` | spawn points in the Castle |
| `clotho` | the Clotho trial |
| `mithril_classes`, `new_abilities`, `new_equipment`, `talents` | new classes, skills, equipment, talents |
| `modkit`, `ping_widget` | modding kit, ping widget |
| `ghidra` | client native code from the dump image: steps and scripts (`ghidra/README.md`) |

A research conclusion that became a decision moves to `docs/` or to the `agents/memory` memory; drafts and evidence
stay here.
