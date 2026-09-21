//! Archive adapters and tools share the trusted core's signature implementation.
#![deny(unsafe_code)]
pub use morrow_core::audit::*;
#[cfg(not(target_arch = "wasm32"))]
pub mod archive;

#[cfg(target_os = "windows")]
pub mod keys;
#[cfg(target_os = "windows")]
pub mod sealer;

#[cfg(target_os = "windows")]
pub mod session;

#[cfg(target_os = "windows")]
pub mod recovery;

#[cfg(target_os = "windows")]
mod backup;

#[cfg(target_os = "windows")]
pub mod snapshot;

#[cfg(target_os = "windows")]
pub mod identity;

#[cfg(target_os = "windows")]
pub mod library;

/// Local engineering receipts; independent of application audit signing.
pub mod build_receipt;

#[cfg(target_os = "windows")]
pub mod credentials;

#[cfg(target_os = "windows")]
pub mod tls_identity;
