//! Real frozen Core consumer. Native transport remains explicitly unimplemented
//! until the host's authoritative new contract is available. No default network.
#![deny(unsafe_code)]
pub mod driver;
pub mod fixture;
pub mod limits;
pub mod network;
pub mod request_task;
