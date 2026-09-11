#![deny(unsafe_code)]

pub mod attachment;
pub mod content;
#[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
pub mod dispatch;
pub mod lifecycle;
pub mod response;
#[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
pub mod store;
pub mod transaction;
// Export attributes mark the reviewed native/Wasm ABI boundary.
#[allow(unsafe_code)]
pub mod bridge;
pub mod envelope;
pub mod runtime;
// The pinned Cap'n Proto generator emits unsafe schema metadata constructors.
#[allow(clippy::all, unsafe_code)]
pub mod runtime_capnp {
    include!(concat!(env!("OUT_DIR"), "/runtime_capnp.rs"));
}

use std::fmt;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Limit,
    Invalid(&'static str),
    UnsupportedVersion,
    RevisionConflict,
    Integrity,
    Storage,
    Io,
    Retained,
    StorageBusy,
    StorageFull,
    OperationConflict,
    NotFound,
    EventCapacity,
    CommitUnknown,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn identity(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(Error::Invalid("identity"));
    }
    Ok(())
}
pub(crate) fn title(value: &str) -> Result<()> {
    if value.len() > 16 * 1024 {
        return Err(Error::Limit);
    }
    Ok(())
}
