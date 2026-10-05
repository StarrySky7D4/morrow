//! Keeps the original host draft model intact, including crate::Result.
//! This crate validates shape only; it grants no capture or content authority.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[path = "lib.rs"]
pub mod model;
pub use model::*;
