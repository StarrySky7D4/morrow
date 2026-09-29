//! Compile/run the actual pipe driver without the host's Core/SQLite/build stack.
//! This probe does not qualify the full host, native protocol, HTTP or upstream Core.
#![cfg(windows)]
#![forbid(unsafe_code)]
#![allow(dead_code)]

type Result<T> = std::result::Result<T, String>;
#[path = "../../../../native_session_stream_001/src/pipe_driver.rs"]
mod pipe_driver;
#[path = "../../../../native_session_stream_001/src/write_state.rs"]
mod write_state;

#[cfg(test)]
mod wire {
    // Only test locator formatting, not a substitute wire codec.
    pub fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
