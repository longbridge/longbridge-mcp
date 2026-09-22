//! The `/omni` endpoint: three meta-tools (`search`, `docs`, `execute`) that
//! cover the whole tool catalogue without listing it.

pub(crate) mod dispatch;
pub(crate) mod execute;
pub(crate) mod index;
pub(crate) mod pipeline;
pub(crate) mod search;
pub(crate) mod truncate;
