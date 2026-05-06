# Contributing to longbridge-mcp

## Prerequisites

- **Rust stable** — install via [rustup](https://rustup.rs/)
- **Rust nightly** — required for `rustfmt`:

  ```bash
  rustup toolchain install nightly
  rustup component add rustfmt --toolchain nightly
  ```

- **Docker** (optional) — for container builds only

The [longbridge](https://github.com/longbridge/openapi) Rust crate is fetched from GitHub automatically by Cargo; no extra setup is needed.

## Building

```bash
cargo build
cargo build --release
```

## Development Workflow

### Format

```bash
cargo +nightly fmt
```

Run this before every commit. The project uses nightly `rustfmt` for formatting.

### Lint

```bash
cargo clippy --all-features --all-targets -- -D warnings
```

All clippy warnings must be resolved before merging.

### Test

```bash
cargo test
```

CI runs format, lint, and test on every pull request. Ensure they all pass locally before opening a PR.

## Code Style

### Generic bounds

Place bounds in `where` clauses, not inline:

```rust
// correct
fn foo<T>(value: T) -> String
where
    T: serde::Serialize,
{ ... }

// wrong
fn foo<T: serde::Serialize>(value: T) -> String { ... }
```

### Imports

- Use `crate::` instead of `super::` in non-test code.
- Declare imports at the top with `use`; avoid long inline paths in function bodies.
- `use super::*` is acceptable inside `#[cfg(test)]` modules.

### Comments

Do not use decorative divider comments such as `// ── Section ───────`. Split large files into separate modules instead.

### Panic messages

Every `panic!`, `unreachable!`, `debug_assert!`, and `assert!` must include a descriptive message:

```rust
// correct
assert!(n > 0, "n must be positive, got {n}");

// wrong
assert!(n > 0);
```

### Error handling

- Prefer `Result` over `panic!` at all call sites.
- Define custom error types with `thiserror`.

### Documentation

- All public items must have `///` doc comments.
- Use `//!` for module-level documentation at the top of each file.

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

Examples:

```
feat(tools): add security_news tool
fix(auth): return 401 when bearer token is missing
docs: update CONTRIBUTING.md
```

## Pull Requests

1. Fork the repository and create a branch from `main`.
2. Make your changes and ensure `cargo +nightly fmt`, `cargo clippy --all-features --all-targets -- -D warnings`, and `cargo test` all pass.
3. Open a pull request against `main` with a clear description of what changed and why.

## Adding a New Tool

Tools are organised by category under `src/tools/`. Each `.rs` file corresponds to one category (`quote`, `trade`, `fundamental`, `market`, etc.). A new tool requires changes in three places:

### Step 1 — Define the parameter struct

Add a `#[derive(Debug, Deserialize, JsonSchema)]` struct in the appropriate category file. Field doc comments become the parameter descriptions shown to LLMs, so be precise.

```rust
// src/tools/quote.rs
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MyToolParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
}
```

If the tool takes no parameters, you can skip this step.

### Step 2 — Implement the handler function

Add a `pub async fn` in the same category file. Use `tool_json` to serialise a successful response and map SDK errors through `Error::longbridge`.

```rust
// src/tools/quote.rs
pub async fn my_tool(ctx: &McpContext, p: MyToolParam) -> Result<CallToolResult, McpError> {
    let quote_ctx = QuoteContext::new(ctx.create_config())
        .await
        .map_err(Error::longbridge)?;
    let result = quote_ctx
        .some_api_call(&p.symbol)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}
```

### Step 3 — Make the param type available in `mod.rs`

How you do this depends on which category the tool belongs to:

- **`quote` and `trade` categories**: add the new type to the existing `use` block near the top of `src/tools/mod.rs`:

  ```rust
  use crate::tools::quote::{..., MyToolParam};
  ```

- **All other categories** (`fundamental`, `market`, `content`, `alert`, `portfolio`, `statement`, `calendar`): no import is needed — reference the type via its qualified path in the handler registration (Step 4):

  ```rust
  Parameters(p): Parameters<fundamental::MyToolParam>,
  ```

### Step 4 — Register the tool in the `#[tool_router]` impl

Inside the `#[tool_router(vis = "pub(crate)")]` impl block in `src/tools/mod.rs`, add a new method. Wrap the call with `measured_tool_call` so it is included in Prometheus metrics.

```rust
/// One-line summary shown to LLMs as the tool description.
#[tool(description = "Detailed description of what this tool returns")]
async fn my_tool(
    &self,
    ctx: RequestContext<RoleServer>,
    Parameters(p): Parameters<MyToolParam>,
) -> Result<CallToolResult, McpError> {
    let mctx = extract_context(&ctx)?;
    measured_tool_call("my_tool", || quote::my_tool(&mctx, p)).await
}
```

The string passed to `measured_tool_call` must match the method name exactly; it is used as the Prometheus metric label.

### Step 5 — Update README

Update the tool count and, if applicable, the tool list table in `README.md`.

## License

By contributing, you agree that your contributions will be licensed under the same license as this project. See [LICENSE](LICENSE) for details.
