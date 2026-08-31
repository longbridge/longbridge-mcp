//! Internal support utilities for tool implementations.
//!
//! These modules are not MCP tools themselves; they provide shared plumbing
//! (HTTP client, serde deserializers, parsing helpers) used by the tool
//! modules alongside this one.

pub mod dry_run;
pub mod http_client;
pub mod parse;
pub mod text;
pub mod tolerant;
pub mod us_market;
pub mod us_normalize;
