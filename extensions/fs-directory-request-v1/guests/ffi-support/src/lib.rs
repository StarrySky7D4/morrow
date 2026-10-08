//! One Rust static archive for the independent directory C/C++ guest samples.
//! Unmodified directory codecs and original SDK allocation exports share the
//! same Rust allocator/runtime. This adds no import, protocol, or host authority.
#![deny(unsafe_code)]

pub use morrow_fs_directory_request_v1 as directory;
pub use morrow_plugin_sdk as allocator;
