//! rusty-adb library — public API surface used by integration tests.
//!
//! The application binary lives in `main.rs`; this library target exposes
//! the modules that integration tests need to construct and call directly.
//!
//! # Modules
//!
//! - [`adb`] — [`AdbClient`](adb::AdbClient), device detection, directory listing,
//!   file operations (rename, delete, pull)
//! - [`transfer`] — async file transfer engine, progress event streaming, adb output parsers

pub mod adb;
pub mod transfer;
