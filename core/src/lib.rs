#![deny(unsafe_code)]

pub mod attachment;
pub mod audit;
pub mod content;
pub mod content_change;
pub mod dependency_call;
#[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
pub mod dispatch;
pub mod io;
pub mod io_evidence;
pub mod io_intent;
pub mod lifecycle;
pub mod plugin_package;
pub mod read_archive;
pub mod read_capture;
pub mod read_journal;
pub mod records;
pub mod response;
#[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
pub mod store;
pub mod tls_identity;
pub mod transaction;
// Export attributes mark the reviewed native/Wasm ABI boundary.
#[allow(unsafe_code)]
pub mod bridge;
pub mod envelope;
pub mod runtime;
pub mod shared_object;
pub mod shared_transfer;
pub mod task;
pub mod task_evidence;
pub mod ui;
// The fixed Cap'n Proto generator emits unsafe schema metadata; hand-written validation does not.
#[allow(clippy::all, unsafe_code)]
pub mod dependency_call_capnp {
    include!(concat!(env!("OUT_DIR"), "/dependency_call_capnp.rs"));
}
#[allow(clippy::all, unsafe_code)]
pub mod shared_transfer_capnp {
    include!(concat!(env!("OUT_DIR"), "/shared_transfer_capnp.rs"));
}
#[allow(clippy::all, unsafe_code)]
pub mod shared_object_capnp {
    include!(concat!(env!("OUT_DIR"), "/shared_object_capnp.rs"));
}
#[allow(clippy::all, unsafe_code)]
pub mod ui_capnp {
    include!(concat!(env!("OUT_DIR"), "/ui_capnp.rs"));
}
#[allow(clippy::all, unsafe_code)]
pub mod task_capnp {
    include!(concat!(env!("OUT_DIR"), "/task_capnp.rs"));
}
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
    EvidenceUnavailable,
    EventCapacity,
    ArchiveCapacity,
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

// Existing experimental IO schema; generated metadata only.
#[allow(clippy::all, unsafe_code)]
pub mod io_capnp {
    include!(concat!(env!("OUT_DIR"), "/io_capnp.rs"));
}

/// Bounded inbound service messages; decoding never authenticates a principal.
pub mod service;
/// Opt-in metadata for explicitly selected live service resources.
pub mod service_resources;
#[allow(clippy::all, unsafe_code)]
pub mod service_resources_capnp {
    include!(concat!(env!("OUT_DIR"), "/service_resources_capnp.rs"));
}
#[allow(clippy::all, unsafe_code)]
pub mod service_capnp {
    include!(concat!(env!("OUT_DIR"), "/service_capnp.rs"));
}

pub mod outbound_authority;
pub mod service_authority;
pub mod service_config;
/// Historical inbound service request identity and bounded retention metadata.
pub mod service_record;
