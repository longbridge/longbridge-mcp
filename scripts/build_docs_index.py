#!/usr/bin/env python3
"""Build data/docs-index.json from open.longbridge.com.

Reads the llms.txt link index, fetches each page's Markdown source for the
configured languages, strips front matter and JSX, splits by `##` headings and
writes a compact snapshot for the omni `docs` tool. Standard library only.
"""
import json
import re
import sys
import time
import urllib.request
from datetime import datetime, timezone

INDEX_URL = "https://open.longbridge.com/llms.txt"
BASE = "https://open.longbridge.com"
LANGS = {"en": "", "zh-CN": "zh-CN/"}
MAX_SECTION_CHARS = 4000
OUT = "data/docs-index.json"
LINK_RE = re.compile(r"^- \[(.+?)\]\((https?://[^)]+?/docs/([^)\s]+))\)")
FRONT_MATTER_RE = re.compile(r"\A---\n.*?\n---\n", re.S)
FRONT_MATTER_TITLE_RE = re.compile(r"\A---\n.*?\ntitle:\s*(.+?)\s*\n.*?\n---\n", re.S)
CLI_COMMAND_RE = re.compile(r"<CliCommand>.*?</CliCommand>\n?", re.S)
JSX_RE = re.compile(r"<[A-Za-z][^>]*?/>|</?[A-Za-z][^>]*>")


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": "longbridge-mcp docs-index/1.0"})
    with urllib.request.urlopen(req, timeout=20) as resp:
        return resp.read().decode("utf-8")


def front_matter_title(md):
    m = FRONT_MATTER_TITLE_RE.match(md)
    if not m:
        return ""
    return m.group(1).strip().strip('"').strip("'")


def clean(md):
    md = FRONT_MATTER_RE.sub("", md)
    # `<CliCommand>` bodies are shell examples (often starting with a `#`
    # comment) rather than prose; drop the whole block so those comment
    # lines are never mistaken for a Markdown heading.
    md = CLI_COMMAND_RE.sub("", md)
    md = JSX_RE.sub("", md)
    return re.sub(r"\n{3,}", "\n\n", md).strip()


def split_sections(md):
    """Split by `#`/`##` headings, ignoring lines inside fenced code blocks
    (e.g. `# Submit order` inside a ```python example is not a heading)."""
    title = ""
    sections = []
    heading = ""
    buf = []
    in_fence = False
    for line in md.splitlines():
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            buf.append(line)
            continue
        if in_fence:
            buf.append(line)
            continue
        if line.startswith("# ") and not title:
            title = line[2:].strip()
            continue
        if line.startswith("## "):
            if buf:
                sections.append((heading, "\n".join(buf).strip()))
            heading = line[3:].strip()
            buf = []
        else:
            buf.append(line)
    if buf:
        sections.append((heading, "\n".join(buf).strip()))
    return title, [(h, t[:MAX_SECTION_CHARS]) for h, t in sections if t]


def main():
    index_md = fetch(INDEX_URL)
    paths = []
    for line in index_md.splitlines():
        m = LINK_RE.match(line.strip())
        if m:
            title, _, path = m.groups()
            paths.append((title.strip(), path.strip("/")))
    pages = []
    for title_from_index, path in paths:
        for lang, prefix in LANGS.items():
            url = f"{BASE}/{prefix}docs/{path}.md"
            try:
                raw = fetch(url)
            except Exception as exc:  # noqa: BLE001
                print(f"skip {url}: {exc}", file=sys.stderr)
                continue
            fm_title = front_matter_title(raw)
            md = clean(raw)
            title, sections = split_sections(md)
            pages.append({
                "path": path,
                "lang": lang,
                "title": title or fm_title or title_from_index,
                "markdown": md,
                "sections": [{"heading": h, "text": t} for h, t in sections],
            })
            time.sleep(0.05)
    with open("data/docs-tool-pages.json", encoding="utf-8") as f:
        tool_pages = json.load(f)
    snapshot = {
        "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "source": INDEX_URL,
        "pages": pages,
        "tool_pages": tool_pages,
    }
    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(snapshot, f, ensure_ascii=False, separators=(",", ":"))
    print(f"wrote {OUT}: {len(pages)} pages")


if __name__ == "__main__":
    main()
