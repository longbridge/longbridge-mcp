#!/usr/bin/env python3
"""
Check whether open-source projects that Longbridge sponsors / has partnered with
still mention Longbridge in their README or website.

Source: tests/awesome-finance-opensource.md
Scope:  rows where 值得联系 == ★★★  (highest priority)
        + any explicit sponsors list (tests/watch-sponsors.json if present)

Writes tests/reports/sponsors-YYYY-MM-DD.md
"""

import re
import sys
import json
import datetime
import urllib.request
import urllib.error
from pathlib import Path

KEYWORDS = ["longbridge", "长桥", "longbridge.com", "longportapp"]
TIMEOUT = 15
HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (compatible; LongbridgeSponsorCheck/1.0)"
    ),
    "Accept": "text/html,application/json,*/*",
}

ROOT = Path(__file__).parent.parent
AWESOME_MD = ROOT / "tests" / "awesome-finance-opensource.md"
WATCH_JSON = ROOT / "tests" / "watch-sponsors.json"   # optional override
REPORT_DIR = ROOT / "tests" / "reports"

# GitHub raw README URL template
GH_RAW = "https://raw.githubusercontent.com/{repo}/HEAD/README.md"
GH_RE = re.compile(r"https://github\.com/([^/\s|)\]>\"']+/[^/\s|)\]>\"']+)")


def extract_high_priority(md_text: str):
    """Return list of {name, github_url} for ★★★ rows."""
    results = []
    for line in md_text.splitlines():
        if "★★★" not in line:
            continue
        m = GH_RE.search(line)
        if not m:
            continue
        gh_url = m.group(0).rstrip(".,;)")
        repo = m.group(1).rstrip(".,;)")
        # Name: first bold item **...**
        name_m = re.search(r"\*\*([^*]+)\*\*", line)
        name = name_m.group(1) if name_m else repo.split("/")[-1]
        results.append({"name": name, "github_url": gh_url, "repo": repo})
    return results


def load_extra_sponsors():
    """Load tests/watch-sponsors.json if it exists."""
    if WATCH_JSON.exists():
        data = json.loads(WATCH_JSON.read_text())
        return data.get("sponsors", [])
    return []


def fetch_content(url: str):
    try:
        req = urllib.request.Request(url, headers=HEADERS)
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            return resp.read(1024 * 256).decode("utf-8", errors="replace")
    except Exception:
        return ""


def check_project(entry: dict):
    repo = entry.get("repo", "")
    gh_url = entry.get("github_url", "")

    # Try README first
    readme_url = GH_RAW.format(repo=repo) if repo else ""
    content = fetch_content(readme_url) if readme_url else ""

    # Fallback: GitHub repo HTML page
    if not content and gh_url:
        content = fetch_content(gh_url)

    has_keyword = any(kw.lower() in content.lower() for kw in KEYWORDS) if content else False
    reachable = bool(content)

    return {
        **entry,
        "reachable": reachable,
        "has_keyword": has_keyword,
        "icon": "✅" if has_keyword else ("⚠️" if reachable else "❌"),
        "checked_url": readme_url or gh_url,
    }


def main():
    today = datetime.date.today().isoformat()

    entries = []
    if AWESOME_MD.exists():
        text = AWESOME_MD.read_text(encoding="utf-8")
        entries = extract_high_priority(text)
        print(f"awesome-finance-opensource.md: {len(entries)} ★★★ projects")

    # Merge explicit sponsor list
    extra = load_extra_sponsors()
    if extra:
        print(f"watch-sponsors.json: {len(extra)} additional entries")
        seen = {e["github_url"] for e in entries}
        for s in extra:
            if s.get("github_url") not in seen:
                entries.append(s)

    if not entries:
        print("No projects to check.")
        return

    results = []
    for i, entry in enumerate(entries, 1):
        print(f"  [{i}/{len(entries)}] {entry['name']}", end=" ", flush=True)
        r = check_project(entry)
        print(r["icon"])
        results.append(r)

    ok = sum(1 for r in results if r["icon"] == "✅")
    warn = sum(1 for r in results if r["icon"] == "⚠️")
    fail = sum(1 for r in results if r["icon"] == "❌")

    REPORT_DIR.mkdir(exist_ok=True)

    json_path = REPORT_DIR / f"sponsors-{today}.json"
    json_path.write_text(json.dumps(results, ensure_ascii=False, indent=2))

    md_lines = [
        f"# Sponsor / Partner Project Check — {today}",
        "",
        "Checks whether high-priority open-source projects still mention Longbridge.",
        "",
        f"| Status | Count |",
        f"|--------|-------|",
        f"| ✅ Mentions Longbridge | {ok} |",
        f"| ⚠️ Reachable but no mention | {warn} |",
        f"| ❌ Unreachable | {fail} |",
        f"| Total | {len(results)} |",
        "",
        "## Details",
        "",
        "| Icon | Project | GitHub | Checked URL |",
        "|------|---------|--------|-------------|",
    ]
    for r in results:
        md_lines.append(
            f"| {r['icon']} | {r['name']} "
            f"| [{r.get('repo', '')}]({r.get('github_url', '')}) "
            f"| {r.get('checked_url', '')} |"
        )

    md_lines += [
        "",
        f"_Generated by scripts/check_sponsors.py at {datetime.datetime.utcnow().isoformat()}Z_",
    ]

    md_path = REPORT_DIR / f"sponsors-{today}.md"
    md_path.write_text("\n".join(md_lines), encoding="utf-8")
    (REPORT_DIR / "sponsors-latest.md").write_text("\n".join(md_lines), encoding="utf-8")
    (REPORT_DIR / "sponsors-latest.json").write_text(
        json.dumps(results, ensure_ascii=False, indent=2)
    )

    print(f"\nReport: {md_path}")
    print(f"Summary: ✅{ok}  ⚠️{warn}  ❌{fail}")


if __name__ == "__main__":
    main()
