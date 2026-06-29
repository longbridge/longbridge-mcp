#!/usr/bin/env python3
"""
Generate docs/index.html — Longbridge MCP presence dashboard.
Reads the latest reports from tests/reports/ and server.json for version info.
"""

import json
import datetime
from pathlib import Path

ROOT = Path(__file__).parent.parent
REPORT_DIR = ROOT / "tests" / "reports"
OUTPUT = ROOT / "docs" / "index.html"
SERVER_JSON = ROOT / "server.json"


def load_json(path: Path):
    if path.exists():
        return json.loads(path.read_text(encoding="utf-8"))
    return []


def version_info():
    if SERVER_JSON.exists():
        d = json.loads(SERVER_JSON.read_text())
        return d.get("version", "—"), d.get("name", "com.longbridge/mcp")
    return "—", "com.longbridge/mcp"


def badge(icon):
    if icon == "✅":
        return '<span class="badge ok">✅ Live</span>'
    if icon == "⚠️":
        return '<span class="badge warn">⚠️ No mention</span>'
    return '<span class="badge fail">❌ Down</span>'


def render_registry_rows(rows):
    if not rows:
        return "<tr><td colspan='5'>No data yet</td></tr>"
    html = []
    for r in rows:
        html.append(
            f"<tr>"
            f"<td>{badge(r.get('icon','❌'))}</td>"
            f"<td>{r.get('source','')}</td>"
            f"<td>{r.get('name','')[:50]}</td>"
            f"<td><a href='{r.get('url','')}' target='_blank'>{r.get('url','')[:60]}</a></td>"
            f"<td>{r.get('status_code') or r.get('error') or ''}</td>"
            f"</tr>"
        )
    return "\n".join(html)


def render_sponsor_rows(rows):
    if not rows:
        return "<tr><td colspan='4'>No data yet — add projects to tests/watch-sponsors.json</td></tr>"
    html = []
    for r in rows:
        html.append(
            f"<tr>"
            f"<td>{badge(r.get('icon','❌'))}</td>"
            f"<td>{r.get('name','')}</td>"
            f"<td><a href='{r.get('github_url','')}' target='_blank'>{r.get('repo','')}</a></td>"
            f"<td><a href='{r.get('checked_url','')}' target='_blank'>README</a></td>"
            f"</tr>"
        )
    return "\n".join(html)


def summary_counts(rows, key="icon"):
    ok = sum(1 for r in rows if r.get(key) == "✅")
    warn = sum(1 for r in rows if r.get(key) == "⚠️")
    fail = sum(1 for r in rows if r.get(key) == "❌")
    return ok, warn, fail, len(rows)


def main():
    version, server_name = version_info()
    registry_rows = load_json(REPORT_DIR / "registry-latest.json")
    sponsor_rows = load_json(REPORT_DIR / "sponsors-latest.json")
    now = datetime.datetime.utcnow().strftime("%Y-%m-%d %H:%M UTC")

    rok, rwarn, rfail, rtotal = summary_counts(registry_rows)
    sok, swarn, sfail, stotal = summary_counts(sponsor_rows)

    html = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Longbridge MCP — Presence Dashboard</title>
<style>
  :root {{
    --green: #22c55e; --yellow: #eab308; --red: #ef4444;
    --bg: #0f172a; --surface: #1e293b; --border: #334155;
    --text: #e2e8f0; --muted: #94a3b8; --accent: #38bdf8;
  }}
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
          background: var(--bg); color: var(--text); padding: 2rem; }}
  header {{ display: flex; align-items: center; gap: 1rem; margin-bottom: 2rem; }}
  header h1 {{ font-size: 1.5rem; font-weight: 700; }}
  header .version {{ background: var(--accent); color: #0f172a;
                     padding: .2rem .6rem; border-radius: 9999px; font-size: .8rem; font-weight: 600; }}
  .updated {{ color: var(--muted); font-size: .8rem; margin-left: auto; }}
  .cards {{ display: grid; grid-template-columns: repeat(auto-fit,minmax(180px,1fr)); gap: 1rem; margin-bottom: 2rem; }}
  .card {{ background: var(--surface); border: 1px solid var(--border); border-radius: .75rem; padding: 1.25rem; }}
  .card .label {{ color: var(--muted); font-size: .75rem; text-transform: uppercase; letter-spacing: .05em; }}
  .card .value {{ font-size: 2rem; font-weight: 700; margin-top: .25rem; }}
  .card.ok .value {{ color: var(--green); }}
  .card.warn .value {{ color: var(--yellow); }}
  .card.fail .value {{ color: var(--red); }}
  section {{ margin-bottom: 2.5rem; }}
  section h2 {{ font-size: 1.1rem; font-weight: 600; margin-bottom: 1rem;
                border-bottom: 1px solid var(--border); padding-bottom: .5rem; }}
  table {{ width: 100%; border-collapse: collapse; font-size: .85rem; }}
  th {{ text-align: left; color: var(--muted); font-weight: 500; padding: .5rem .75rem;
        border-bottom: 1px solid var(--border); }}
  td {{ padding: .5rem .75rem; border-bottom: 1px solid var(--border); vertical-align: middle; }}
  tr:hover td {{ background: var(--surface); }}
  a {{ color: var(--accent); text-decoration: none; }}
  a:hover {{ text-decoration: underline; }}
  .badge {{ padding: .2rem .5rem; border-radius: .375rem; font-size: .75rem; font-weight: 600; }}
  .badge.ok {{ background: rgba(34,197,94,.15); color: var(--green); }}
  .badge.warn {{ background: rgba(234,179,8,.15); color: var(--yellow); }}
  .badge.fail {{ background: rgba(239,68,68,.15); color: var(--red); }}
  .install {{ background: var(--surface); border: 1px solid var(--border);
              border-radius: .75rem; padding: 1.25rem; font-family: monospace; font-size: .85rem; }}
  .install p {{ color: var(--muted); font-family: sans-serif; font-size: .8rem; margin-bottom: .5rem; }}
  .install code {{ color: var(--accent); }}
</style>
</head>
<body>
<header>
  <h1>Longbridge MCP</h1>
  <span class="version">v{version}</span>
  <span class="updated">Last checked: {now}</span>
</header>

<div class="cards">
  <div class="card ok">
    <div class="label">Registries Live</div>
    <div class="value">{rok}</div>
  </div>
  <div class="card warn">
    <div class="label">No Mention</div>
    <div class="value">{rwarn}</div>
  </div>
  <div class="card fail">
    <div class="label">Down</div>
    <div class="value">{rfail}</div>
  </div>
  <div class="card ok">
    <div class="label">Sponsors Live</div>
    <div class="value">{sok}</div>
  </div>
</div>

<section>
  <h2>Quick Install</h2>
  <div class="install">
    <p>Claude Code</p>
    <code>claude mcp add --transport http longbridge https://mcp.longbridge.com</code>
  </div>
</section>

<section>
  <h2>Registry &amp; Directory Status ({rtotal} entries)</h2>
  <table>
    <thead>
      <tr><th>Status</th><th>Source</th><th>Name</th><th>URL</th><th>HTTP</th></tr>
    </thead>
    <tbody>
      {render_registry_rows(registry_rows)}
    </tbody>
  </table>
</section>

<section>
  <h2>Sponsor / Partner Projects ({stotal} entries)</h2>
  <table>
    <thead>
      <tr><th>Status</th><th>Project</th><th>Repo</th><th>Checked</th></tr>
    </thead>
    <tbody>
      {render_sponsor_rows(sponsor_rows)}
    </tbody>
  </table>
</section>

</body>
</html>
"""
    OUTPUT.parent.mkdir(exist_ok=True)
    OUTPUT.write_text(html, encoding="utf-8")
    print(f"Homepage written to {OUTPUT}")


if __name__ == "__main__":
    main()
