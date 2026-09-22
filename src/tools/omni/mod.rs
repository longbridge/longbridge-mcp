//! The `/omni` endpoint: three meta-tools (`search`, `docs`, `execute`) that
//! cover the whole tool catalogue without listing it.

pub(crate) mod dispatch;
pub(crate) mod index;
pub(crate) mod search;
