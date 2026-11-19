"""Replaces text in the markup from a dictionary: elements with data-t="key" (text only inside) and attributes data-t-attr="attribute:key".

swap(html, base) -> (html_base, other): the markup gets the text from base (the main language — English), and what was
in the markup is returned as the dictionary other (key -> text, whitespace collapsed, as site.js sees it). A key without text in base
stays as it was and goes into missing.
"""
import html as _html
import re

TEXT = re.compile(r'(<([a-zA-Z0-9]+)\b[^>]*?\sdata-t="([^"]+)"[^>]*>)([^<]*)(</\2>)')
ATTR_TAG = re.compile(r'<[a-zA-Z0-9]+\b[^>]*\sdata-t-attr="([^"]+)"[^>]*>')


def norm(s):
    return " ".join(_html.unescape(s).split())


def esc_text(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def esc_attr(s):
    return esc_text(s).replace('"', "&quot;")


def swap(markup, base):
    other, missing = {}, []

    def text(m):
        start, _tag, key, content, end = m.groups()
        other[key] = norm(content)
        if key not in base:
            missing.append(key)
            return m.group(0)
        return start + esc_text(base[key]) + end

    markup = TEXT.sub(text, markup)

    def attr_tag(m):
        tag = m.group(0)
        attr, key = m.group(1).split(":", 1)
        am = re.search(r'\s%s="([^"]*)"' % re.escape(attr), tag)
        if not am:
            return tag
        other[key] = norm(am.group(1))
        if key not in base:
            missing.append(key)
            return tag
        return tag[: am.start(1)] + esc_attr(base[key]) + tag[am.end(1):]

    markup = ATTR_TAG.sub(attr_tag, markup)
    return markup, other, missing
