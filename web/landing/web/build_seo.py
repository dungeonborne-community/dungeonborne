"""Search engines and social networks: python build_seo.py — after edits to the "Help" text, community links, the site address and
deploying a new launcher version (update_release.py).

Writes:
- into the <head> of index.html between <!-- seo:start --> and <!-- seo:end -->: og:site_name, locales, the social card and
  JSON-LD markup (what this site is, the launcher as a free Windows app, the "Help" questions and answers). When SITE_URL is set —
  also canonical, og:url, og:image (img/og.jpg, 1200×630), hreflang (en / ru via ?lang=ru);
- into the footer — the community link addresses from LINKS (an empty address — placeholder href="#");
- robots.txt (and sitemap.xml when SITE_URL is set).

What JSON-LD is: a piece of data in <script type="application/ld+json"> that Google and Yandex read. It is not visible and not
executed (CSP does not get in the way), but it helps a search engine understand what is on the page: for example, show "free, Windows"
for the launcher in the results, or expandable answers to questions.
"""
import html
import json
import re
from pathlib import Path

SITE = Path(__file__).resolve().parent

# site address with a trailing slash; empty — while there is no domain (13.11: dungeonborne-community.com chosen)
SITE_URL = "https://dungeonborne-community.com/"
NAME = "Dungeonborne Community"  # as the server in Discord
# community links (footer and sameAs in JSON-LD); an empty string — placeholder
LINKS = {
    "discord": "",
    "youtube": "",
    "telegram": "",
    "vk": "",
    "reddit": "",
}


def strip(s):
    return " ".join(html.unescape(re.sub(r"<[^>]+>", " ", s)).split())


def main():
    page = SITE / "index.html"
    h = page.read_text(encoding="utf-8")

    title = strip(re.search(r"<title>(.*?)</title>", h, re.S).group(1))
    desc = html.unescape(re.search(r'<meta name="description" content="([^"]*)"', h).group(1))
    version = re.search(r'<span data-release="version">([^<]+)</span>', h).group(1)
    size_mb = int(re.search(r'data-release="size" data-bytes="(\d+)"', h).group(1)) / 1048576

    # the "Help" questions and answers (the main markup language is English)
    help_part = h[h.index('<section id="help"'):]
    help_part = help_part[: help_part.index("</section>")]
    faq = []
    for d in re.finditer(r"<details\b[^>]*>(.*?)</details>", help_part, re.S):
        body = d.group(1)
        q = strip(re.search(r"<summary>(.*?)</summary>", body, re.S).group(1))
        a = strip(body.split("</summary>", 1)[1])
        if q and a:
            faq.append({"@type": "Question", "name": q, "acceptedAnswer": {"@type": "Answer", "text": a}})

    same = [u for u in LINKS.values() if u]
    app = {
        "@type": "SoftwareApplication",
        "name": f"{NAME} Launcher",
        "applicationCategory": "GameApplication",
        "operatingSystem": "Windows 10, Windows 11",
        "softwareVersion": version,
        "fileSize": f"{size_mb:.1f} MB",
        "offers": {"@type": "Offer", "price": "0", "priceCurrency": "USD"},
        "description": desc,
    }
    site = {"@type": "WebSite", "name": NAME, "description": desc, "inLanguage": ["en", "ru"]}
    org = {"@type": "Organization", "name": NAME}
    if same:
        org["sameAs"] = same
    if SITE_URL:
        site["url"] = SITE_URL
        org["url"] = SITE_URL
        app["url"] = SITE_URL
        app["downloadUrl"] = SITE_URL + "downloads/DungeonborneLauncher.exe"
    graph = [site, org, app]
    if faq:
        graph.append({"@type": "FAQPage", "mainEntity": faq})
    ld = json.dumps({"@context": "https://schema.org", "@graph": graph}, ensure_ascii=False, indent=1).replace("</", "<\\/")

    tags = [
        f'<meta property="og:site_name" content="{html.escape(NAME)}">',
        '<meta property="og:locale" content="en_US">',
        '<meta property="og:locale:alternate" content="ru_RU">',
        '<meta name="twitter:card" content="summary_large_image">',
        f'<meta name="twitter:title" content="{html.escape(title)}">',
    ]
    if SITE_URL:
        tags += [
            f'<link rel="canonical" href="{SITE_URL}">',
            f'<link rel="alternate" hreflang="en" href="{SITE_URL}">',
            f'<link rel="alternate" hreflang="ru" href="{SITE_URL}?lang=ru">',
            f'<link rel="alternate" hreflang="x-default" href="{SITE_URL}">',
            f'<meta property="og:url" content="{SITE_URL}">',
            f'<meta property="og:image" content="{SITE_URL}img/og.jpg">',
            '<meta property="og:image:width" content="1200">',
            '<meta property="og:image:height" content="630">',
        ]
    block = ("  <!-- seo:start — built by build_seo.py, do not edit by hand -->\n  " + "\n  ".join(tags)
             + f'\n  <script type="application/ld+json">\n{ld}\n  </script>\n  <!-- seo:end -->\n')
    if "<!-- seo:start" in h:
        a, b = h.index("  <!-- seo:start"), h.index("<!-- seo:end -->\n") + len("<!-- seo:end -->\n")
        h = h[:a] + block + h[b:]
    else:
        anchor = '  <link rel="icon"'
        h = h.replace(anchor, block + anchor, 1)

    # community links in the footer
    def link(m):
        url = LINKS.get(m.group(2), "")
        attrs = f'href="{html.escape(url)}" target="_blank" rel="noopener"' if url else 'href="#"'
        return f'<a class="{m.group(1)}" {attrs} data-link="{m.group(2)}">'
    h = re.sub(r'<a class="([^"]+)" href="[^"]*"(?: target="_blank" rel="noopener")? data-link="([a-z]+)">', link, h)
    page.write_text(h, encoding="utf-8", newline="\n")

    robots = "User-agent: *\nAllow: /\nDisallow: /downloads/\n"
    if SITE_URL:
        robots += f"\nSitemap: {SITE_URL}sitemap.xml\n"
        (SITE / "sitemap.xml").write_text(
            '<?xml version="1.0" encoding="UTF-8"?>\n'
            '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">\n'
            f"  <url>\n    <loc>{SITE_URL}</loc>\n"
            f'    <xhtml:link rel="alternate" hreflang="en" href="{SITE_URL}"/>\n'
            f'    <xhtml:link rel="alternate" hreflang="ru" href="{SITE_URL}?lang=ru"/>\n'
            "  </url>\n</urlset>\n", encoding="utf-8", newline="\n")
    (SITE / "robots.txt").write_text(robots, encoding="utf-8", newline="\n")
    print(f"seo: faq {len(faq)}, links {len(same)}/{len(LINKS)}, site url {'set' if SITE_URL else 'empty'}")


if __name__ == "__main__":
    main()
