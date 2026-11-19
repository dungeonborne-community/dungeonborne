"""Class showcase from the game tables and strings: description, strengths and weaknesses, weapons, Q/E skills, talents.

    python build_classes.py

Reads `ExcelConfigjson/role_type.json`, `role_skill.json`, `skill_num_exchange.json` and `names/strings*.tsv` (backups folder).
Writes the markup into index.html between <!-- classes:start --> and <!-- classes:end --> in English (the site's main language) and
the Russian texts into classes_ru.js (window.I18N_RU). Do not edit either by hand: text fixes go into FIX below.
The scenes are built in Russian (as before), at the end i18n_swap.swap puts in English and returns Russian as a dictionary.
"""
import html
import json
import re
from pathlib import Path

from i18n_swap import swap

SITE = Path(__file__).resolve().parent
REPO = SITE.parents[2]  # workspace root
NAMES = REPO / "content" / "names"
CFG = REPO / "core" / "backend" / "ExcelConfigjson"

# Order on the site: by race, as in character selection. file — name of the image and video files, crest — the crest in the sprite.
CLASSES = [
    # career,          file,           crest,       skill prefix, talent prefix, race key, race colour
    ("FIGHTER",        "fighter",      "k-fighter", 107, "Fighter",      "race.h", "text-accent-text"),
    ("ROGUE",          "rogue",        "k-rogue",   103, "Rogue",        "race.h", "text-accent-text"),
    ("PRIEST",         "priest",       "k-priest",  105, "Priest",       "race.h", "text-accent-text"),
    ("WIZARD",         "pyromancer",   "k-pyro",    109, "Wizard",       "race.h", "text-accent-text"),
    ("SWORDMASTER",    "swordmaster",  "k-sword",   101, "SwordMaster",  "race.e", "text-ok"),
    ("DRUID_PANTHER",  "druid",        "k-druid",   106, "DruidPanther", "race.e", "text-ok"),
    ("DEATHKNIGHT",    "death-knight", "k-dk",      108, "DeathKnight",  "race.u", "text-arcane"),
    ("LICH",           "cryomancer",   "k-cryo",    110, "Lich",         "race.u", "text-arcane"),
]
GROUPS = [("race.h", "race.hp", "Люди", "text-accent-text"), ("race.e", "race.ep", "Эльфы", "text-ok"), ("race.u", "race.up", "Нежить", "text-arcane")]
RACE_RU = {"race.h": "Человек", "race.e": "Эльф", "race.u": "Нежить"}
# English texts of the shared showcase keys (their Russian ones are in i18n.js)
SHARED_EN = {
    "cls.cons": "Weaknesses", "cls.label": "Classes", "cls.panther": "in panther form", "cls.pause": "Pause",
    "cls.play": "Watch", "cls.pros": "Strengths", "cls.talent": "talent", "cls.talents": "Talents",
    "cls.weapons": "Weapons", "race.e": "Elf", "race.ep": "Elves", "race.h": "Human", "race.hp": "Humans",
    "race.u": "Undead", "race.up": "Undead",
}
# hidden in the table but needed: the druid's Shadow Assault — E in panther form
EXTRA_SKILLS = {106004: "E"}
PANTHER = {106004}

# Fixes to the game strings (typos, words run together, cut-off phrases). Key — (language, string key).
FIX = {
    ("ru", "Initial_Skill_Description_103003"): "Выпускает 6 кинжалов в нескольких врагов впереди, снимает с себя все отрицательные эффекты и замедляет врагов на 70% на 3 сек.",
    ("ru", "Initial_Skill_Description_107006"): "Активирует силу рассечения на 15 секунд. Когда следующая атака оружием наносит незаблокированный активный урон, цель получает рассечение: начальный урон зависит от её текущего здоровья, а пока рассечение действует, она получает урон за пройденное расстояние. Рассечение длится 6 секунд на игроках и 10 секунд на монстрах и накладывает «Тяжёлую рану» 30%.",
    ("ru", "Talent_Fighter_Desc5"): "Враги под действием навыка Q замедляются на 20%.",
    ("ru", "Talent_DruidPanther_Desc2"): "Навык E может накопить на 1 заряд больше.",
    ("ru", "Talent_Lich_Desc4"): "Урон первой фазы «Ледяного шторма» и активный урон «Морозного проклятья» увеличены на 50%.",
    ("ru", "Initial_Skill_Description_110004"): "Заряжается до 2 стадий. Стадия 1: ледяная стрела наносит активный урон, замедляет и накладывает «Проклятие обморожения» — пассивный урон каждую секунду, который исцеляет криоманта. Стадия 2: сразу накладывает «Проклятие обморожения» с дополнительным активным уроном и замедлением, а после короткой задержки распространяет его на ближайшие цели. На одной цели — не больше 1 стака. (Требуется посох или сфера.)",
}


def load_strings(name):
    out = {}
    for line in (NAMES / name).open(encoding="utf-8"):
        p = line.rstrip("\n").split("\t", 2)
        if len(p) == 3:
            out.setdefault(p[1], p[2])
    return out


STR = {"ru": load_strings("strings.tsv"), "en": load_strings("strings_en.tsv")}
NUM = {r["id"]: r.get("value") for r in json.loads((CFG / "skill_num_exchange.json").read_text(encoding="utf-8")) if "value" in r}


def text(lang, key):
    if (lang, key) in FIX:
        return FIX[(lang, key)]
    s = STR[lang].get(key, "")
    s = re.sub(r"</?[A-Za-z]*\s*/?>", "", s)  # <Base>, <Yellow>, < />
    s = re.sub(r"\{\d\|(\w+)\}", lambda m: NUM.get(m.group(1), m.group(0)), s)
    s = s.replace("「", "").replace("」", "")
    s = re.sub(r"\[?\{Skill1\}\]?", "Q", s)
    s = re.sub(r"\[?\{Skill2\}\]?", "E", s)
    s = re.sub(r"\[([^\]]+)\]", r"«\1»" if lang == "ru" else r"\1", s)
    if lang == "ru":
        s = s.replace("всостояние", "в состоянии")
        s = re.sub(r"([а-яё])(\d)", r"\1 \2", s)
        s = re.sub(r"(\d%?)([а-яё])", r"\1 \2", s)
        s = re.sub(r"%([а-яё])", r"% \1", s)
    s = re.sub(r"([.!?])([А-ЯЁA-Z])", r"\1 \2", s)
    s = re.sub(r"\s+", " ", s).strip()
    assert "{" not in s, (lang, key, s)
    return s


def career_rows():
    rows = json.loads((CFG / "role_type.json").read_text(encoding="utf-8"))
    by = {}
    for r in rows:
        m = r["meta"]
        if m.get("force") == "EN_FORCE_TYPE_ADVENTURER" and r["environment"] == "EN_ENVIRONMENT_TYPE_ALL_AVAILABLE":
            by[m["career"].removeprefix("EN_ROLE_CAREER_")] = r
    return by


def skills_of(prefix):
    rows = json.loads((CFG / "role_skill.json").read_text(encoding="utf-8"))
    out = []
    for r in rows:
        sid = r["skill_id"]
        if sid // 1000 != prefix:
            continue
        if r.get("is_display") or sid in EXTRA_SKILLS:
            out.append((sid, r["skill_position"][-1]))
    return out


def talents_of(prefix):
    n = 1
    out = []
    while f"Talent_{prefix}_Name{n}" in STR["en"]:
        out.append(n)
        n += 1
    return out


def esc(s):
    return html.escape(s, quote=False)


def split_list(lang, key):
    """«++ Разгром|+ Владение» → [(2, 'Разгром'), (1, 'Владение')]."""
    out = []
    for part in text(lang, key).split("|"):
        part = part.strip()
        m = re.match(r"^([+\-]+)\s*(.*)$", part)
        level, body = (len(m.group(1)), m.group(2)) if m else (1, part)
        out.append((level, body.strip()))
    return out


def build():
    careers = career_rows()
    en = {}
    tabs, panels = {g[0]: [] for g in GROUPS}, []
    for career, f, crest, sp, tp, race_key, race_color in CLASSES:
        r = careers[career]
        k = f"cls.{f}"
        name = text("ru", r["str_name"])
        en[f"{k}.n"] = text("en", r["str_name"])
        en[f"{k}.d"] = text("en", r["str_desc"])
        tabs[race_key].append(
            f'<button type="button" role="tab" id="tab-{f}" aria-controls="cls-{f}" aria-selected="false" class="class-tab">'
            f'<span class="crest class-crest"><svg class="icon"><use href="#{crest}"/></svg></span>'
            f'<span class="leading-tight" data-t="{k}.n">{esc(name)}</span></button>')

        def li_list(key, kind):
            items = []
            for i, ((lvl, ru_b), (_, en_b)) in enumerate(zip(split_list("ru", r[key]), split_list("en", r[key]))):
                en[f"{k}.{kind}{i}"] = en_b
                strong = " text-fg" if lvl > 1 else ""
                items.append(f'<li class="{kind}{strong}" data-t="{k}.{kind}{i}">{esc(ru_b)}</li>')
            return "".join(items)

        weapons = []
        for i, (ru_w, en_w) in enumerate(zip(text("ru", r["str_equipments"]).split("|"), text("en", r["str_equipments"]).split("|"))):
            en[f"{k}.w{i}"] = en_w.strip()
            weapons.append(f'<li class="pill" data-t="{k}.w{i}">{esc(ru_w.strip())}</li>')

        # skills: tiles by slot and descriptions
        slots = {"Q": [], "E": []}
        details = []
        for sid, pos in skills_of(sp):
            sk = f"sk.{sid}"
            ru_n, ru_d = text("ru", f"Skill_Name_{sid}"), text("ru", f"Initial_Skill_Description_{sid}") or text("ru", f"Skill_Description_{sid}")
            en[f"{sk}.n"] = text("en", f"Skill_Name_{sid}")
            en[f"{sk}.d"] = text("en", f"Initial_Skill_Description_{sid}") or text("en", f"Skill_Description_{sid}")
            form = ' <span class="pill h-5 align-middle text-2xs text-fg-2" data-t="cls.panther">в облике пантеры</span>' if sid in PANTHER else ""
            slots[pos].append(
                f'<button type="button" class="skill-tile" aria-pressed="false" aria-controls="sk-{sid}" data-skill="{sid}">'
                f'<span class="slot skill-icon"><span class="skill-key">{pos}</span>'
                f'<img src="img/skills/{sid}.webp" width="128" height="128" alt="" loading="lazy" decoding="async"></span>'
                f'<span class="skill-name" data-t="{sk}.n">{esc(ru_n)}</span></button>')
            details.append(
                f'<div class="skill-detail" id="sk-{sid}" data-skill="{sid}">'
                f'<p class="flex flex-wrap items-center gap-2"><span class="skill-key-pill">{pos}</span>'
                f'<span class="font-display text-xl font-bold" data-t="{sk}.n">{esc(ru_n)}</span>{form}</p>'
                f'<p class="mt-2 text-copy text-fg-2" data-t="{sk}.d">{esc(ru_d)}</p></div>')
        slot_html = "".join(
            f'<div><p class="class-sub">{pos}</p><div class="mt-2 flex flex-wrap gap-2">{"".join(b)}</div></div>'
            for pos, b in slots.items() if b)

        # talents: tags on the right; the selected one's description goes where the skills' do (over the bottom of the video), the video does not change
        talents = []
        for n in talents_of(tp):
            tk = f"tl.{tp}.{n}"
            en[f"{tk}.n"] = text("en", f"Talent_{tp}_Name{n}")
            en[f"{tk}.d"] = text("en", f"Talent_{tp}_Desc{n}")
            ru_n = esc(text("ru", f"Talent_{tp}_Name{n}"))
            talents.append(
                f'<button type="button" class="talent-chip" aria-pressed="false" aria-controls="tl-{f}-{n}" data-skill="t{n}">'
                f'<span data-t="{tk}.n">{ru_n}</span></button>')
            details.append(
                f'<div class="skill-detail" id="tl-{f}-{n}" data-skill="t{n}">'
                f'<p class="flex flex-wrap items-center gap-2"><span class="skill-key-pill"><svg class="icon"><use href="#i-star"/></svg></span>'
                f'<span class="font-display text-xl font-bold" data-t="{tk}.n">{ru_n}</span>'
                f'<span class="pill h-5 align-middle text-2xs text-fg-2" data-t="cls.talent">талант</span></p>'
                f'<p class="mt-2 text-copy text-fg-2" data-t="{tk}.d">{esc(text("ru", f"Talent_{tp}_Desc{n}"))}</p></div>')

        # a one-screen scene: on the left the video (the skill description over its bottom) and the row of skills, on the right the class and talents
        panels.append(f'''        <article id="cls-{f}" role="tabpanel" aria-labelledby="tab-{f}" class="class-panel" data-video="video/classes/{f}">
          <div class="class-stage">
            <div class="min-w-0">
              <div class="group/media relative">
              <div class="card framed">
                <div class="slot aspect-video rounded-ovl" data-slot="video/classes/{f}.webm|.mp4 · video/skills/&lt;id&gt; · img/classes/{f}.webp · 1280×720">
                  <span class="crest"><svg class="icon"><use href="#{crest}"/></svg></span>
                  <img src="img/classes/{f}.webp" width="1280" height="720" alt="" loading="lazy" decoding="async">
                  <video class="class-video" muted loop playsinline preload="none" aria-hidden="true" tabindex="-1"></video>
                  <p class="video-caption" hidden></p>
                  <button type="button" class="video-toggle group" hidden><svg class="icon group-data-playing:hidden"><use href="#i-play"/></svg><svg class="icon hidden group-data-playing:block"><use href="#i-pause"/></svg><span class="group-data-playing:hidden" data-t="cls.play">Смотреть</span><span class="hidden group-data-playing:inline" data-t="cls.pause">Пауза</span></button>
                </div>
              </div>
              <div class="skill-details">{"".join(details)}</div>
              </div>
              <div class="class-skills">{slot_html}</div>
            </div>
            <div class="min-w-0">
              <p class="text-2xs font-semibold tracking-brand uppercase {race_color}" data-t="{race_key}">{RACE_RU[race_key]}</p>
              <h4 class="mt-1 font-display text-3xl font-bold lg:text-2xl" data-t="{k}.n">{esc(name)}</h4>
              <p class="mt-2 text-copy text-fg-2 lg:text-label" data-t="{k}.d">{esc(text("ru", r["str_desc"]))}</p>
              <div class="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-1 xl:grid-cols-2">
                <div><p class="class-sub text-ok" data-t="cls.pros">Сильные стороны</p><ul class="mt-1.5 space-y-0.5">{li_list("str_advantages", "pro")}</ul></div>
                <div><p class="class-sub text-critical" data-t="cls.cons">Слабые стороны</p><ul class="mt-1.5 space-y-0.5">{li_list("str_disadvantages", "con")}</ul></div>
              </div>
              <p class="class-sub mt-3" data-t="cls.weapons">Оружие</p>
              <ul class="mt-1.5 flex flex-wrap gap-1">{"".join(weapons)}</ul>
              <p class="class-sub mt-3"><span data-t="cls.talents">Таланты</span> · {len(talents)}</p>
              <div class="mt-1.5 flex flex-wrap gap-1">{"".join(talents)}</div>
            </div>
          </div>
        </article>''')

    groups = "\n".join(
        f'          <div class="class-group"><p class="class-group-label {color}" data-t="{gkey}">{gname}</p>'
        f'<div class="flex gap-1">{"".join(tabs[rkey])}</div></div>'
        for rkey, gkey, gname, color in GROUPS)
    block = (
        "      <!-- classes:start — built by build_classes.py from the game tables, do not edit by hand -->\n"
        '      <div class="reveal mt-5" data-classes>\n'
        "        <!-- picker: hidden without JS, and the scenes below are all shown in a row -->\n"
        '        <div role="tablist" aria-label="Классы" data-t-attr="aria-label:cls.label" class="class-picker" hidden>\n'
        f"{groups}\n        </div>\n"
        '        <div class="class-panels mt-5 space-y-16">\n' + "\n".join(panels) + "\n        </div>\n      </div>\n"
        "      <!-- classes:end -->\n")

    # the main language is English: Russian goes into the dictionary
    block, ru, missing = swap(block, {**en, **SHARED_EN})
    assert not missing, f"no English text: {missing}"

    page = SITE / "index.html"
    h = page.read_text(encoding="utf-8")
    start, end = h.index("      <!-- classes:start"), h.index("      <!-- classes:end -->\n") + len("      <!-- classes:end -->\n")
    page.write_bytes((h[:start] + block + h[end:]).encode("utf-8"))

    body = ",\n".join(f"  {json.dumps(k)}: {json.dumps(ru[k], ensure_ascii=False)}" for k in en if k in ru)
    (SITE / "classes_ru.js").write_text(
        "// Russian texts of the class showcase — built by build_classes.py from the game strings, do not edit by hand.\n"
        f"Object.assign((window.I18N_RU = window.I18N_RU || {{}}), {{\n{body},\n}});\n", encoding="utf-8", newline="\n")
    print(f"classes: {len(panels)}, russian strings: {sum(1 for k in en if k in ru)}")


if __name__ == "__main__":
    build()
