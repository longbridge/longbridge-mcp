# Contributing to longbridge-mcp

## Prerequisites

- Rust (stable + nightly for formatting)
- Docker (optional, for container builds)

Install nightly toolchain for `rustfmt`:

```bash
rustup toolchain install nightly
rustup component add rustfmt --toolchain nightly
```

## Building

```bash
cargo build
cargo build --release
```

## Development Workflow

```bash
# Format
cargo +nightly fmt

# Lint (must pass with zero warnings)
cargo clippy --all-features --all-targets -- -D warnings

# Test
cargo test
```

CI runs all three checks on every pull request. Ensure they all pass locally before opening a PR.

## Code Style

- Place generic bounds in `where` clauses, not inline.
- Use `crate::` for imports in non-test code; `use super::*` is acceptable in `#[cfg(test)]` modules.
- Declare imports at the top with `use`; avoid long inline paths in function bodies.
- No decorative divider comments (`// ── Section ───`). Split into modules instead.
- `panic!`, `unreachable!`, `assert!`, and `debug_assert!` must include a descriptive message.
- Prefer `Result` over panics. Use `thiserror` for custom error types.
- Public items must have doc comments (`///` for items, `//!` for modules).

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

Examples:
- `feat(tools): add candlestick aggregation tool`
- `fix(auth): return 401 on expired token`
- `docs: update README configuration table`

## Pull Requests

1. Fork the repository and create a branch from `main`.
2. Make your changes and ensure CI checks pass.
3. Open a pull request against `main` with a clear description of what changed and why.

## Adding a New Tool

Tools live in `src/tools/`. Each file corresponds to a category (quote, trade, fundamental, etc.).

1. Add the tool handler function to the appropriate category file.
2. Register it in `src/tools/mod.rs`.
3. Update the tool count in `README.md`.

## License

By contributing, you agree that your contributions will be licensed under the same license as this project. See [LICENSE](LICENSE) for details.
