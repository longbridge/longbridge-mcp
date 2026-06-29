#!/usr/bin/env python3
"""
Check all ✅ URLs in REGISTRY_CHECKLIST.md and DISTRIBUTION_CHANNELS.md.
For each URL verify:
  1. HTTP reachable (status < 400 or 403 bot-blocked)
  2. Page content contains at least one Longbridge keyword
Writes tests/reports/registry-YYYY-MM-DD.md
"""

import re
import sys
import json
import datetime
import urllib.request
import urllib.error
from pathlib import Path

KEYWORDS = ["longbridge", "长桥", "longbridge-mcp", "com.longbridge"]
TIMEOUT = 15
HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
        "AppleWebKit/537.36 (KHTML, like Gecko) "
        "Chrome/124.0.0.0 Safari/537.36"
    ),
    "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
}

ROOT = Path(__file__).parent.parent
SOURCES = [
    ROOT / "tests" / "REGISTRY_CHECKLIST.md",
    ROOT / "tests" / "DISTRIBUTION_CHANNELS.md",
]
REPORT_DIR = ROOT / "tests" / "reports"


def extract_urls(md_text: str):
    """Return list of (name, url, status) from markdown tables."""
    results = []
    url_re = re.compile(r"https?://[^\s|)\]>\"']+")
    row_re = re.compile(r"^\|(.+)\|$")

    for line in md_text.splitlines():
        m = row_re.match(line.strip())
        if not m:
            continue
        cells = [c.strip() for c in m.group(1).split("|")]
        if len(cells) < 3:
            continue

        # Skip header / separator rows
        if all(re.match(r"^[-:]+$", c.replace(" ", "")) for c in cells if c):
            continue

        # Look for ✅ in any cell
        row_text = " ".join(cells)
        if "✅" not in row_text:
            continue

        # Extract first URL in the row
        url_match = url_re.search(row_text)
        if not url_match:
            continue

        url = url_match.group(0).rstrip(".,;)")
        # Best-effort name: first non-empty cell, strip markdown bold
        name = re.sub(r"[*_~`\[\]]", "", cells[0]).strip() or url
        results.append({"name": name, "url": url})

    return results


def check_url(url: str):
    """Return dict with reachable, has_keyword, status_code, error."""
    result = {"reachable": False, "has_keyword": False, "status_code": None, "error": None}
    try:
        req = urllib.request.Request(url, headers=HEADERS)
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            result["status_code"] = resp.status
            result["reachable"] = resp.status < 400
            body = resp.read(1024 * 512).decode("utf-8", errors="replace").lower()
            result["has_keyword"] = any(kw.lower() in body for kw in KEYWORDS)
    except urllib.error.HTTPError as e:
        result["status_code"] = e.code
        # 403/429 means the site is up but blocks bots — treat as reachable
        result["reachable"] = e.code in (403, 429, 401)
        result["error"] = f"HTTP {e.code}"
    except Exception as e:
        result["error"] = str(e)[:120]
    return result


def status_icon(r):
    if not r["reachable"]:
        return "❌"
    if not r["has_keyword"]:
        return "⚠️"
    return "✅"


def main():
    today = datetime.date.today().isoformat()
    all_entries = []

    for src in SOURCES:
        if not src.exists():
            print(f"SKIP (not found): {src}", file=sys.stderr)
            continue
        text = src.read_text(encoding="utf-8")
        entries = extract_urls(text)
        print(f"{src.name}: {len(entries)} live URLs to check")
        for e in entries:
            e["source"] = src.name
            all_entries.append(e)

    total = len(all_entries)
    print(f"Total: {total} URLs")

    rows = []
    ok = warn = fail = 0
    for i, entry in enumerate(all_entries, 1):
        print(f"  [{i}/{total}] {entry['url'][:80]}", end=" ", flush=True)
        r = check_url(entry["url"])
        icon = status_icon(r)
        print(icon)
        entry.update(r)
        entry["icon"] = icon
        rows.append(entry)
        if icon == "✅":
            ok += 1
        elif icon == "⚠️":
            warn += 1
        else:
            fail += 1

    # Write JSON
    REPORT_DIR.mkdir(exist_ok=True)
    json_path = REPORT_DIR / f"registry-{today}.json"
    json_path.write_text(json.dumps(rows, ensure_ascii=False, indent=2))

    # Write Markdown report
    md_lines = [
        f"# Registry Health Check — {today}",
        "",
        f"| Status | Count |",
        f"|--------|-------|",
        f"| ✅ Reachable + has Longbridge info | {ok} |",
        f"| ⚠️ Reachable but no Longbridge keyword | {warn} |",
        f"| ❌ Unreachable | {fail} |",
        f"| Total checked | {total} |",
        "",
        "## Details",
        "",
        "| Icon | Source | Name | URL | HTTP | Error |",
        "|------|--------|------|-----|------|-------|",
    ]
    for r in rows:
        md_lines.append(
            f"| {r['icon']} | {r['source']} | {r['name'][:40]} "
            f"| [{r['url'][:60]}]({r['url']}) "
            f"| {r['status_code'] or '-'} "
            f"| {r.get('error') or ''} |"
        )

    md_lines += [
        "",
        f"_Generated by scripts/check_registries.py at {datetime.datetime.utcnow().isoformat()}Z_",
    ]
    md_path = REPORT_DIR / f"registry-{today}.md"
    md_path.write_text("\n".join(md_lines), encoding="utf-8")

    # Also write a "latest" symlink-style file for easy CI access
    (REPORT_DIR / "registry-latest.md").write_text("\n".join(md_lines), encoding="utf-8")
    (REPORT_DIR / "registry-latest.json").write_text(
        json.dumps(rows, ensure_ascii=False, indent=2)
    )

    print(f"\nReport: {md_path}")
    print(f"Summary: ✅{ok}  ⚠️{warn}  ❌{fail}")

    # Exit non-zero if any failures so CI can flag it
    if fail > 0:
        sys.exit(1)


if __name__ == "__main__":
    main()
