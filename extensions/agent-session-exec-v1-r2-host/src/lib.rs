//! Reviewed R2 package and native/Wasm host adapters.
//! Package declarations are ceilings; only independent host approval grants them.
#![deny(unsafe_code)]
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub mod fixed_executor;
pub mod native;
pub mod package;
pub use package::{AgentPackage, ApprovedSession, Declaration, PreparedAgentPackage};
