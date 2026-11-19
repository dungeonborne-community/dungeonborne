"""Site icon as the "D" tile of the splash screen (.boot-mark): the dark "Torch" tile with a frame, a gold "D" with a glow.
The letter is an outline from the site font (Dungeonborne Serif Latin Bold), so as not to depend on system fonts.

    python site/og/favicon.py      → site/web/img/favicon.svg
    node site/og/render.mjs icon   → site/web/img/apple-touch-icon.png (180×180, from favicon.svg)
"""
from pathlib import Path

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

WEB = Path(__file__).resolve().parent.parent / "web"
font = TTFont(WEB / "fonts" / "DungeonborneSerifLatin-Bold.woff2")
gs = font.getGlyphSet()
name = font.getBestCmap()[ord("D")]

bp = BoundsPen(gs)
gs[name].draw(bp)
x0, y0, x1, y1 = bp.bounds
# the letter in a 64×64 tile: height 38, centred (the font's y axis points up, SVG's — down)
size, box = 38, 64
k = size / (y1 - y0)
dx = (box - (x1 - x0) * k) / 2 - x0 * k
dy = (box + (y1 - y0) * k) / 2 + y0 * k
sp = SVGPathPen(gs)
gs[name].draw(TransformPen(sp, (k, 0, 0, -k, dx, dy)))
path = sp.getCommands()

svg = f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
<defs>
<filter id="g" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="2.2" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge></filter>
<linearGradient id="t" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2d261e"/><stop offset="1" stop-color="#1f1a14"/></linearGradient>
</defs>
<rect x="2" y="2" width="60" height="60" rx="9" fill="url(#t)" stroke="#6b5638" stroke-width="2.5"/>
<path filter="url(#g)" fill="#e0a84a" d="{path}"/>
</svg>
"""
(WEB / "img" / "favicon.svg").write_text(svg, encoding="utf-8", newline="\n")
print("favicon.svg", len(svg), "bytes")
