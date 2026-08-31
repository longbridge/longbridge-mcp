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
