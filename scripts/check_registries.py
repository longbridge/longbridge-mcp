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
from pathlib import Path

try:
    import requests as _requests
    _USE_REQUESTS = True
except ImportError:
    import urllib.request
    import urllib.error
    _USE_REQUESTS = False

KEYWORDS = ["longbridge", "长桥", "longbridge-mcp", "com.longbridge"]
TIMEOUT = 15

# Sites confirmed live by manual inspection but cannot be auto-verified
# (JS challenge / SPA / bot protection). Shown as ✅ Live (manual) in reports.
MANUAL_LIVE_DOMAINS = [
    "mcpmarket.com",    # Vercel Security Checkpoint — manually confirmed live
    "cursor.directory", # JS-rendered — manually confirmed live
    "lobehub.com",      # JS-rendered — manually confirmed live
    "mcpfinder.dev",    # SPA — manually confirmed live
    "skillhub.club",    # JS-rendered — manually confirmed live
    "skilldock.io",     # JS-rendered — manually confirmed live
    "aur.archlinux.org",  # AUR page — manually confirmed live
]

# Specific URLs (not full domains) confirmed live manually.
MANUAL_LIVE_URLS = [
    "https://smithery.ai/skills/longbridge-official/longbridge",
]
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


PENDING_MARKERS = ["已提交", "审核中", "已发送", "待审核", "🔄"]

def extract_urls(md_text: str):
    """Return list of entries from markdown tables.

    Each entry has:
      name, url, current_status ("live" | "pending")
    live    = row contains ✅
    pending = row contains 已提交/审核中 markers (no ✅)
    """
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

        row_text = " ".join(cells)

        # Determine row status
        is_live = "✅" in row_text
        is_pending = not is_live and any(p in row_text for p in PENDING_MARKERS)

        if not is_live and not is_pending:
            continue

        # Skip rows explicitly marked as not applicable / defunct
        if any(s in row_text for s in ["不适用", "已驳回", "已废弃", "❌"]):
            continue

        # Extract first URL in the row
        url_match = url_re.search(row_text)
        if not url_match:
            continue

        url = url_match.group(0).rstrip(".,;)")
        name = re.sub(r"[*_~`\[\]]", "", cells[0]).strip() or url
        results.append({
            "name": name,
            "url": url,
            "current_status": "live" if is_live else "pending",
        })

    return results


def check_url(url: str):
    """Return dict with reachable, has_keyword, status_code, error."""
    result = {"reachable": False, "has_keyword": False, "status_code": None, "error": None}
    try:
        if _USE_REQUESTS:
            resp = _requests.get(url, headers=HEADERS, timeout=TIMEOUT,
                                 allow_redirects=True, stream=False)
            result["status_code"] = resp.status_code
            # 403/429 = bot-blocked but site is up; treat as reachable
            result["reachable"] = resp.status_code < 400 or resp.status_code in (403, 429, 401)
            if result["reachable"]:
                body = resp.text[:1024 * 512].lower()
                result["has_keyword"] = any(kw.lower() in body for kw in KEYWORDS)
        else:
            import urllib.request, urllib.error, ssl
            try:
                import certifi
                ctx = ssl.create_default_context(cafile=certifi.where())
            except ImportError:
                ctx = ssl.create_default_context()
            req = urllib.request.Request(url, headers=HEADERS)
            with urllib.request.urlopen(req, timeout=TIMEOUT, context=ctx) as resp:
                result["status_code"] = resp.status
                result["reachable"] = resp.status < 400
                body = resp.read(1024 * 512).decode("utf-8", errors="replace").lower()
                result["has_keyword"] = any(kw.lower() in body for kw in KEYWORDS)
    except Exception as e:
        code = getattr(e, "response", None)
        if code is not None:
            result["status_code"] = code.status_code
            result["reachable"] = code.status_code in (403, 429, 401)
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
    ok = warn = fail = newly_live = 0
    for i, entry in enumerate(all_entries, 1):
        print(f"  [{i}/{total}] {entry['url'][:80]}", end=" ", flush=True)

        # Sites/URLs confirmed live manually — mark as ✅ without auto-check
        domain = entry["url"].split("/")[2] if "/" in entry["url"] else ""
        if entry["url"] in MANUAL_LIVE_URLS or any(d in domain for d in MANUAL_LIVE_DOMAINS):
            icon = "✅"
            entry.update({"reachable": True, "has_keyword": True,
                          "status_code": "manual", "error": None})
            entry["icon"] = icon
            print(icon, "(manual-live)")
            rows.append(entry)
            ok += 1
            continue

        r = check_url(entry["url"])
        icon = status_icon(r)

        # Pending entry just became live → flag as 🆕
        if entry.get("current_status") == "pending" and icon == "✅":
            icon = "🆕"
            newly_live += 1
            print(icon, "(newly live!)")
        else:
            print(icon)

        entry.update(r)
        entry["icon"] = icon
        rows.append(entry)
        if icon in ("✅", "🆕"):
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
        f"| ✅ Live (confirmed) | {ok} |",
        f"| 🆕 Newly live — was pending/submitted | {newly_live} |",
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
    print(f"Summary: ✅{ok}  🆕{newly_live}(newly live)  ⚠️{warn}  ❌{fail}")

    # Exit non-zero if any failures so CI can flag it
    if fail > 0:
        sys.exit(1)


if __name__ == "__main__":
    main()
