//! Shared helpers for unit tests, used from multiple modules' `#[cfg(test)]`
//! sections. This file only compiles under `#[cfg(test)]` (see the `mod`
//! declaration in `main.rs`), so it never ships in a release binary.

use std::io::Write;
use std::sync::{Arc, Mutex};

/// A `Write` sink a `tracing_subscriber::fmt` layer can write into, whose
/// contents a test can then read back and assert on.
#[derive(Clone, Default)]
pub(crate) struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl SharedBuffer {
    pub(crate) fn contents(&self) -> String {
        String::from_utf8(self.0.lock().expect("buffer poisoned").clone())
            .expect("log output is not utf-8")
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("buffer poisoned").write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Serializes every test that captures `tracing` output by installing a
/// thread-local subscriber and calling `tracing::callsite::rebuild_interest_cache()`.
///
/// That interest cache is process-global: `rebuild_interest_cache()` recomputes
/// every callsite's interest against the rebuilding thread's current subscriber
/// and writes it to the shared cache. Two such tests running concurrently (the
/// default under `cargo test`) therefore race — one can recompute a callsite the
/// other is about to assert on, cache it as disabled, and silently swallow the
/// event. The symptom is a log-capture test that passes alone but fails
/// intermittently in the full suite.
///
/// Every capturing test across modules (`tools`, `auth`) takes this single lock,
/// so at most one manipulates the global cache at a time. A `tokio::sync::Mutex`
/// so it can be held across `.await` in the `#[tokio::test]` sites without
/// tripping `clippy::await_holding_lock`.
pub(crate) static LOG_CAPTURE_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));
