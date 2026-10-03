//! Standalone Linux control/SDK qualification fixture, never a product owner.
//! Shared SDK/runtime/host schemas and source are path dependencies, unchanged.
#![cfg(target_os = "linux")]
#![deny(unsafe_op_in_unsafe_fn)]

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub mod driver;
pub mod frozen;
pub mod state;
pub mod transport;
pub mod worker;
