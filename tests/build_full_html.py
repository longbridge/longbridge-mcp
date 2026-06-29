"""Convert FULL_TEST_20260421.md -> FULL_TEST_20260421.html with project styling."""

import pathlib
import markdown

HERE = pathlib.Path(__file__).parent
MD = (HERE / "FULL_TEST_20260421.md").read_text(encoding="utf-8")

body = markdown.markdown(
    MD,
    extensions=["tables", "fenced_code", "toc", "attr_list"],
    output_format="html5",
)

STYLE = """
:root {
  --bg: #0f1116;
  --panel: #161922;
  --panel-2: #1d2130;
  --border: #2a2f42;
  --text: #e6e8ee;
  --muted: #9aa4b2;
  --accent: #7cb7ff;
  --accent-2: #a78bfa;
  --red: #ff6b6b;
  --amber: #ffb454;
  --green: #5ddb94;
  --code-bg: #0a0c12;
  --code-inline-bg: #1f2333;
  --code-inline-fg: #ffb4de;
}
@media (prefers-color-scheme: light) {
  :root {
    --bg: #f7f8fb;
    --panel: #ffffff;
    --panel-2: #eef1f7;
    --border: #dde2ed;
    --text: #1b1f2a;
    --muted: #5c6578;
    --accent: #1d6fd6;
    --accent-2: #7c3aed;
    --red: #c0392b;
    --amber: #b47100;
    --green: #118a4e;
    --code-bg: #0f1116;
    --code-inline-bg: #edf0fa;
    --code-inline-fg: #b3266c;
  }
  pre { color: #e6e8ee; }
}
* { box-sizing: border-box; }
html, body { background: var(--bg); color: var(--text); }
body {
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
               "Hiragino Sans GB", "Microsoft YaHei", Helvetica, Arial, sans-serif;
  line-height: 1.65;
  font-size: 14.5px;
}
.wrap { max-width: 1280px; margin: 0 auto; padding: 32px 24px 96px; }
h1 { font-size: 26px; margin: 0 0 12px; letter-spacing: 0.2px; }
h2 { font-size: 20px; margin: 32px 0 12px; border-left: 4px solid var(--accent); padding-left: 10px; }
h3 { font-size: 17px; margin: 20px 0 10px; color: var(--accent-2); }
p, li { color: var(--text); }
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }
hr { border: none; border-top: 1px solid var(--border); margin: 28px 0; }
code {
  background: var(--code-inline-bg);
  color: var(--code-inline-fg);
  padding: 2px 6px;
  border-radius: 5px;
  font-family: "SF Mono", "JetBrains Mono", Menlo, Consolas, monospace;
  font-size: 12.5px;
  word-break: break-word;
}
pre {
  background: var(--code-bg);
  padding: 14px 16px;
  border-radius: 8px;
  overflow: auto;
  border: 1px solid var(--border);
}
pre code { background: transparent; color: inherit; padding: 0; font-size: 12.5px; }
table {
  width: 100%;
  border-collapse: collapse;
  margin: 14px 0 22px;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
  font-size: 13.5px;
}
thead th {
  background: var(--panel-2);
  text-align: left;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
  font-weight: 600;
  color: var(--muted);
}
tbody td {
  padding: 9px 12px;
  border-bottom: 1px solid var(--border);
  vertical-align: top;
}
tbody tr:last-child td { border-bottom: none; }
tbody tr:hover { background: var(--panel-2); }
tbody td:nth-child(1) { color: var(--muted); white-space: nowrap; }
tbody td:nth-child(2) code { color: var(--accent); }
tbody td:nth-child(4) { white-space: nowrap; text-align: center; }
blockquote {
  margin: 12px 0;
  padding: 8px 14px;
  border-left: 4px solid var(--accent-2);
  background: var(--panel);
  border-radius: 0 6px 6px 0;
}
ul, ol { padding-left: 24px; }
li { margin: 4px 0; }
.meta { color: var(--muted); font-size: 13px; margin: -4px 0 18px; }
"""

HTML = f"""<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>Longbridge MCP 全量回归测试 — 2026-04-21</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
{STYLE}
</style>
</head>
<body>
<div class="wrap">
{body}
</div>
</body>
</html>
"""

out = HERE / "FULL_TEST_20260421.html"
out.write_text(HTML, encoding="utf-8")
print(f"wrote {out} ({len(HTML)} bytes)")
