//! Experimental raw-frame IO guest for handler `morrow.http.forward.v1`.
//! The trusted host builds the core IO Request and validates its exact response.
//! This guest has no schema, SDK, trusted-core, credential or network dependency.
#![deny(unsafe_code)]

#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
mod imports;

#[cfg(any(target_arch = "wasm32", test))]
mod forward;
