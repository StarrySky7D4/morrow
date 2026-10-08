//! Explicit VM preparation helpers, using the original public SDK objects.
#![forbid(unsafe_code)]
#[cfg(windows)]
pub mod catalog;
#[cfg(windows)]
#[path = "../../c28-basic-support/wasm/controls.rs"]
pub mod controls;
#[cfg(windows)]
pub mod basic;
#[cfg(windows)]
pub mod basic_witness;
#[cfg(windows)]
pub mod basic_security;
pub mod evidence;
#[cfg(windows)]
pub mod formal;
#[cfg(windows)]
mod formal_cleanup;
#[cfg(windows)]
pub mod guest_identity;
pub mod interaction;
#[cfg(windows)]
pub mod live;
#[cfg(windows)]
pub mod params;
pub mod preflight;
#[cfg(windows)]
pub mod product;
#[cfg(windows)]
pub mod protected_owner;
#[cfg(windows)]
#[path = "../../c28-basic-support/provisioning/provisioning.rs"]
pub mod provisioning;
#[cfg(windows)]
#[path = "../../c28-basic-support/wasm/sealed.rs"]
pub mod sealed;
#[cfg(windows)]
pub mod sealed_preparation;
#[path = "../../c28-basic-support/wasm/witness.rs"]
pub mod witness;
#[cfg(windows)]
pub mod wrapper;
