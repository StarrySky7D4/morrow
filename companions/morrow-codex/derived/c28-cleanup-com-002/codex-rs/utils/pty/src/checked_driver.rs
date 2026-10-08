use super::{CheckedControlCapabilities, TerminalSize};
use std::future::Future;
use std::io;
use std::pin::Pin;

/// Trusted adapter attached to the original driver and its actual I/O owner.
/// Ok means applied OS/correlated remote ACK; queuing is never sufficient.
pub trait CheckedDriverControls: Send + Sync {
    fn capabilities(&self) -> CheckedControlCapabilities;
    /// Sticky evidence loss, never an OS exit/EOF. Exec must publish Failed and
    /// prevent facts completion when this channel reports any failure.
    fn lifecycle_failure(&self) -> Option<tokio::sync::watch::Receiver<Option<String>>> {
        None
    }
    /// Must synchronously latch its one close request before returning this future.
    /// Repeated calls join the same result and never dispatch another close.
    fn close_stdin(&self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>>;
    fn resize(
        &self,
        size: TerminalSize,
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>>;
}
