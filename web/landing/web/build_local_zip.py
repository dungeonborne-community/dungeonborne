"""Site archive for viewing without a server: unpack it and open index.html with a double click.

python build_local_zip.py  ->  ../dungeonborne-site-local.zip

Takes only what the page needs (no launcher exe, build scripts or README). Chrome does not load fonts
from a file:// page (its origin is null), so in the archive copy the fonts are embedded in the CSS as data: URLs;
the site files are not changed. Clips and images from video/ and img/ are included if they exist.
"""
import base64
import io
import pathlib
import re
import zipfile

WEB = pathlib.Path(__file__).resolve().parent
OUT = WEB.parent / "dungeonborne-site-local.zip"
ROOT = "dungeonborne-site/"
FILES = ["index.html", "styles.css", "fonts.css", "site.js", "atmos.js", "i18n.js", "classes_ru.js", "news.js"]
MEDIA = ["img", "video"]  # all contents except README
SKIP = {"README.md"}


def inline_fonts(css: str) -> str:
    def repl(m):
        data = (WEB / m.group(2)).read_bytes()
        return f'url("data:font/woff2;base64,{base64.b64encode(data).decode()}")'

    return re.sub(r'url\((["\']?)(fonts/[^"\')]+\.woff2)\1\)', repl, css)


def main():
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for name in FILES:
            p = WEB / name
            if name.endswith(".css"):
                z.writestr(ROOT + name, inline_fonts(p.read_text(encoding="utf-8")))
            elif name == "index.html":  # preloading font files from file:// also hits origin null
                html = re.sub(r'[ \t]*<link rel="preload" href="fonts/[^"]+"[^>]*>\n', "", p.read_text(encoding="utf-8"))
                z.writestr(ROOT + name, html)
            else:
                z.write(p, ROOT + name)
        for d in MEDIA:
            for p in sorted((WEB / d).rglob("*")) if (WEB / d).is_dir() else []:
                if p.is_file() and p.name not in SKIP:
                    z.write(p, ROOT + p.relative_to(WEB).as_posix())
        for p in sorted((WEB / "fonts").glob("OFL-*.txt")):  # font licences travel with the fonts
            z.write(p, ROOT + "fonts/" + p.name)
    OUT.write_bytes(buf.getvalue())
    print(f"{OUT}  {OUT.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
