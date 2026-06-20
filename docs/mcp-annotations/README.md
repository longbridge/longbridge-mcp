# MCP Tool Annotations — OpenAI Submission Review

This directory addresses two items from the OpenAI MCP submission review for the
Longbridge MCP server:

1. **Every tool must explicitly set `readOnlyHint`, `openWorldHint`, and
   `destructiveHint`.** (Mandatory — fixed in code.)
2. **For each tool, justify why each explicit annotation value is accurate** and
   does not misrepresent what the tool does. (Per-tool rationale, below.)

It also notes the optional `outputSchema` recommendation (see the end).

## 1. Code fix: every tool now sets the three required hints

Before this change, the 123 read-only tools and `authenticate` declared
`readOnlyHint` and `openWorldHint` (and `idempotentHint`) but **omitted
`destructiveHint`**, which the review flagged as missing.

We added the explicit value to every tool that lacked it
(`src/tools/mod.rs`):

- **123 read-only tools + `now`** → `destructive_hint = false`
  (a read-only tool performs no updates at all, so it is non-destructive by
  construction — see the MCP semantics note below).
- **`authenticate`** → `destructive_hint = false`
  (it establishes a session from a one-time code; it creates credentials, it
  does not overwrite or delete existing data).

The 22 mutating tools already declared `destructiveHint` explicitly and were
left unchanged.

After the change, all **146** tools set `readOnlyHint`, `destructiveHint`,
`idempotentHint`, and `openWorldHint` explicitly. Verified by parsing
`src/tools/mod.rs`; the crate compiles (`cargo check --all-features`, exit 0).

## MCP annotation semantics (the basis for every justification)

Per the [MCP spec — Tool Annotations](https://modelcontextprotocol.io/specification/draft/server/tools#tool):

| Hint | Meaning |
|------|---------|
| `readOnlyHint = true` | The tool does not modify its environment — it is a pure read. |
| `destructiveHint = true` | The tool may perform destructive (irreversible / overwriting / deleting) updates. `false` = updates are additive or non-destructive. Only meaningful when `readOnlyHint = false`; for a read-only tool it is trivially `false` because the tool performs no updates at all. |
| `idempotentHint = true` | Repeated calls with the same arguments have no additional effect beyond the first. |
| `openWorldHint = true` | The tool interacts with external entities outside the server's closed system. Here, **every** tool reaches the Longbridge market/brokerage backend over the network, whose responses change with the live market and are not controlled by this server — hence `true` everywhere. |

## 2. Per-tool annotation justifications

All 146 tools are covered (verified: no tool missing, no duplicates). The
rationale is split by functional domain:

| File | Domain | Tools |
|------|--------|-------|
| [`group-a-market.md`](group-a-market.md) | Market & quote data (read-only) | 38 |
| [`group-b-account.md`](group-b-account.md) | Account & trade records (read-only) | 16 |
| [`group-c-fundamentals.md`](group-c-fundamentals.md) | Fundamentals & research (read-only) | 40 |
| [`group-d-content.md`](group-d-content.md) | News / screener / IPO / DCA / watchlist queries (read-only) | 29 |
| [`group-e-mutating.md`](group-e-mutating.md) | Mutating tools (`readOnlyHint = false`) | 23 |

Each entry gives a one-to-two sentence justification per explicit annotation
value, grounded in what the tool actually does (verified against its
implementation in `src/tools/*.rs`, not just its description).

### Items the reviewers flagged for a second look

The sub-agents that wrote the rationale surfaced a few values worth a conscious
sign-off (all judged accurate, but they are the non-obvious ones):

- **`screener_search`, `dca_check`, `quant_run`** issue an HTTP `POST` upstream
  but are **read-only**: the POST body only carries input parameters; the call
  computes/queries against live data and persists nothing. Annotated
  `readOnlyHint = true` accordingly.
- **`submit_order`** is annotated `destructiveHint = true`. Strictly, MCP's
  `destructiveHint` concerns overwriting/deleting *existing* data, whereas
  submitting an order is an *irreversible side effect*. We treat an action this
  consequential as destructive; this is the one tool where a narrow reading of
  the spec could differ.
- **`sharelist_add` (`idempotentHint = false`) vs `sharelist_remove`
  (`idempotentHint = true`)** — adding can create duplicate entries (not
  idempotent); removing the same symbol twice has no further effect
  (idempotent). The asymmetry is intentional.
- **`withdrawals` / `deposits` / `bank_cards`** are **history/listing queries**
  (GET only); they do not initiate fund movements. Annotated read-only.

## 3. `outputSchema` status and compact descriptors

The review also shows a non-blocking *"Recommended: Add an outputSchema"* notice
on each tool. Currently **81** tools declare a typed `output_schema` (via
`schema_for::<output::*>()`, defined in `src/tools/output.rs`); the remaining
**65** do not.

OpenAI Apps guidance expects tools that return `structuredContent` to declare an
`outputSchema`, so the descriptor keeps typed schemas where they exist. To keep
the large tool list manageable, the server compacts the `tools/list`
`outputSchema` by removing documentation-only schema keys (`$schema`, `title`,
and `description`) while preserving validation structure. Tools that already
declare an `outputSchema` also keep compact top-level descriptions: duplicated
`Returns ...` field-list prose is removed from the exposed descriptor, and a
metadata lint test keeps those descriptions under 240 characters.

The full verbose schemas remain available through MCP resources:

```text
lb://tools/{tool_name}/output-schema
```

For example, `lb://tools/depth/output-schema` returns the complete JSON Schema
with field descriptions intact. Adding output schemas for the remaining tools is
tracked separately — most return loosely-typed JSON proxied straight from the
upstream Longbridge API, so each needs a typed `output::*` struct matching the
post-`to_tool_json` snake_case + RFC3339 shape before its tool can reference it.
See `src/tools/output.rs` for the established pattern.
