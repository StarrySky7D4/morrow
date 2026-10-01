//! Pinned CCSwitch pure computation. No authority, I/O or automatic fallback.
mod claude_desktop_config;
mod model_capabilities;
mod profile;
pub use profile::{HANDLER, INPUT_TYPE, MAX_INPUT_BYTES, OUTPUT_TYPE, ProfileError, evaluate_json};
