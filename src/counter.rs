//! Symbol ↔ counter_id conversion.
//!
//! Re-exports the public `longbridge::counter` module so call sites can keep
//! using `crate::counter::*`. The conversion logic and the embedded
//! ETF + index + warrant directory live in the openapi SDK.
pub use longbridge::counter::{
    counter_id_to_symbol, index_symbol_to_counter_id, is_etf, symbol_to_counter_id,
};
