//! One original Rust runtime/allocator across this C or C++ guest instance.
#![deny(unsafe_code)]
pub use morrow_fs_watch_ffi_v1 as codec;
pub use morrow_plugin_sdk as allocator;
