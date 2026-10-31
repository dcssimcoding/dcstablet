#!/usr/bin/env python3
"""Build or verify the offline guide from the repository's Markdown files.

Requires Python 3.11+ and requirements-docs.txt. No dependency is needed at runtime.
"""
import argparse
import base64
import html
from html.parser import HTMLParser
from pathlib import Path
import re
import sys
import tomllib
from urllib.parse import unquote, urlsplit

try:
    import markdown
except ImportError:
    sys.exit("Install documentation tools: python -m pip install -r requirements-docs.txt")

ROOT = Path(__file__).resolve().parents[1]
CHAPTERS = [
    ("README.md", "overview", "Overview"),
    ("docs/user-guide.md", "user-guide", "User guide"),
    ("docs/network.md", "network", "Network connection"),
    ("docs/virtual-display.md", "virtual-display", "Virtual display"),
    ("docs/troubleshooting.md", "troubleshooting", "Problems"),
    ("docs/removal.md", "removal", "Recovery and removal"),
    ("docs/data-safety.md", "data-safety", "Data safety"),
    ("docs/commands.md", "commands", "Command examples"),
    ("docs/glossary.md", "glossary", "Glossary"),
    ("CONTRIBUTING.md", "contributing", "Build and test"),
    ("docs/architecture.md", "architecture", "Technical description"),
    ("docs/control-protocol.md", "control-protocol", "Control protocol"),
    ("docs/validation.md", "validation", "Test record"),
    ("docs/writing-guide.md", "writing-guide", "Writing guide"),
    ("CHANGELOG.md", "history", "Version history"),
    ("THIRD-PARTY-NOTICES.md", "third-party", "Third-party notices"),
]


def render(text):
    return markdown.markdown(text, extensions=["extra", "toc"], output_format="html5")


class Links(HTMLParser):
    def __init__(self, text):
        super().__init__()
        self.ids = set()
        self.references = []
        self.feed(text)

    def handle_starttag(self, tag, attributes):
        attrs = dict(attributes)
        if "id" in attrs:
            if attrs["id"] in self.ids:
                raise ValueError("Duplicate HTML ID: " + attrs["id"])
            self.ids.add(attrs["id"])
        key = "href" if tag == "a" else "src" if tag == "img" else None
        if key and key in attrs:
            self.references.append(attrs[key])
        if tag == "img" and not attrs.get("alt"):
            raise ValueError("An image has no alternative text")


def validate(guide):
    documents = {ROOT / name: Links(render((ROOT / name).read_text(encoding="utf-8")))
                 for name, _, _ in CHAPTERS}
    documents[ROOT / "START-HERE.html"] = Links(guide)
    for source, parsed in documents.items():
        for reference in parsed.references:
            parts = urlsplit(reference)
            if parts.scheme or parts.netloc:
                continue
            target = (source.parent / unquote(parts.path)).resolve() if parts.path else source
            if not target.is_file():
                raise ValueError(f"{source.name}: missing local file {reference}")
            if parts.fragment and target in documents and unquote(parts.fragment) not in documents[target].ids:
                raise ValueError(f"{source.name}: missing local anchor {reference}")


def build():
    ids = {(ROOT / name).resolve(): ident for name, ident, _ in CHAPTERS}
    ids[ROOT / "START-HERE.html"] = "overview"
    ids[ROOT / "LICENSE"] = "license"
    sections = []
    for name, ident, label in CHAPTERS:
        source = ROOT / name
        text = render(source.read_text(encoding="utf-8"))
        text = re.sub(r'id="([^"]+)"', lambda m: f'id="{ident}-{m[1]}"', text)

        def link(match):
            parts = urlsplit(html.unescape(match[1]))
            if parts.scheme or parts.netloc:
                return match[0]
            target = (source.parent / unquote(parts.path)).resolve() if parts.path else source.resolve()
            if target in ids:
                destination = "#" + ids[target] + ("-" + parts.fragment if parts.fragment else "")
            else:
                destination = target.relative_to(ROOT).as_posix() + ("#" + parts.fragment if parts.fragment else "")
            return 'href="' + html.escape(destination, quote=True) + '"'

        def image(match):
            parts = urlsplit(html.unescape(match[1]))
            if parts.scheme or parts.netloc:
                return match[0]
            target = (source.parent / unquote(parts.path)).resolve()
            target.relative_to(ROOT)
            mime = "image/png" if target.suffix.lower() == ".png" else "image/svg+xml"
            return 'src="data:' + mime + ';base64,' + base64.b64encode(target.read_bytes()).decode() + '"'

        text = re.sub(r'href="([^"]*)"', link, text)
        text = re.sub(r'src="([^"]*)"', image, text)
        text = re.sub(r'<(/?)h([1-6])(\b[^>]*)>', lambda m: '<' + m[1] + 'h' + str(min(int(m[2]) + 1, 6)) + m[3] + '>', text)
        sections.append(f'<section id="{ident}" aria-label="{html.escape(label)}">{text}</section>')
    sections.append('<section id="license"><h2>MIT License</h2><pre>' + html.escape((ROOT / "LICENSE").read_text(encoding="utf-8")) + '</pre></section>')
    navigation = ''.join(f'<a href="#{ident}">{html.escape(label)}</a>' for _, ident, label in CHAPTERS[:8])
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    template = (ROOT / "docs/guide-template.html").read_text(encoding="utf-8")
    result = template.replace("$VERSION", version).replace("$NAVIGATION", navigation).replace("$CONTENT", ''.join(sections))
    validate(result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Fail if START-HERE.html needs regeneration")
    args = parser.parse_args()
    result = build()
    target = ROOT / "START-HERE.html"
    if args.check:
        if not target.exists() or target.read_text(encoding="utf-8") != result:
            sys.exit("START-HERE.html is stale. Run python scripts/Build-Guide.py")
        print("Offline guide, local links, anchors and image descriptions: PASS")
    else:
        target.write_text(result, encoding="utf-8", newline="\n")
        print(f"Built {target.name} ({len(result.encode())} bytes)")


if __name__ == "__main__":
    main()
