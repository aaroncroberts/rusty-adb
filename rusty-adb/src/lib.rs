//! rusty-adb library — public API surface used by integration tests.
//!
//! The application binary lives in `main.rs`; this library target exposes
//! the modules that integration tests need to construct and call directly.

pub mod adb;
pub mod transfer;
