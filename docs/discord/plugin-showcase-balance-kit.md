<!-- Post in the ✨・plugin-showcase forum. Heading — the "Title" field, tag — Tool. Message 1 — post body
     (attachments: balance-tool-cover.webp first — gallery cover, then the zip). Message 2 — first reply in the thread.
     The original post is bilingual (RU + EN); this copy gives the English text only. -->

## Title (≤100)
🛠 Balance Tool — edit game balance in Excel or with an LLM

## Tag
Tool

## Attachments
1. balance-tool-cover.webp (cover)
2. balance_kit/out/dungeonborne-balance-20261104-542ba6f5.zip (15.6 MB)

## Message 1 (post body)
**Author:** <author>

**Balance Tool** — a framework tool: change game balance without our code, in Excel or by asking ChatGPT/Claude. You get a patch file; the server owner reviews it and applies it with one command.

**Editable:** class stats per level · item stats & affixes · legendaries · vendor prices · durability
**What you see:** 🟨 yellow — editable · 🟧 orange — already differs from the original game (original in the cell note) · "Differences" sheet — all changes as a list
**Kit version:** `20261104-542ba6f5` (`dbbalance-kit/1`) · **Download:** zip below 👇

🧩 Server owners: `cargo xtask balance kit [--season=<id>]` — your own kit for your server or season.

## Message 2 (first reply in the thread)
**🤖 With an LLM**
1. Unzip, upload `llm/PROMPT.md` + the needed `llm/<table>.md` to the chat (the large `affix_*` ones one at a time)
2. Ask: "the knight is weak at levels 1–10", "potions 20% cheaper"
3. Save the JSON from the answer to `patch.json` · check: `python tools/check.py patch.json`

**📊 In Excel**
1. `balance.xlsx` → change only yellow cells, do not add or delete rows
2. `pip install openpyxl`, then `python tools/make_patch.py balance.xlsx --author <nickname> --comment "why"`

**📤 Submit:** post `patch-*.json` as a reply in this thread with one sentence on "why".
⚠️ Skill damage and loot are not in the kit. A patch is tied to the kit version — after an update, download the new zip.
