use super::{CheckedControlCapabilities, TerminalSize};
use std::future::Future;
use std::io;
use std::pin::Pin;

/// Outcome of bounded native finalization for a checked driver owner.
///
/// Preserves physical reap with sticky `Unknown`/`noReplay` separately from
/// clean timely release so an `Unknown` reap is never promoted to ordinary
/// `Exited`/`Closed` completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckedOwnerFinalization {
    CleanRelease {
        exit_code: i32,
    },
    PhysicallyReapedWithStickyUnknown {
        exit_code: Option<i32>,
    },
}

/// Trusted adapter attached to the original driver and its actual I/O owner.
/// Ok means applied OS/correlated remote ACK; queuing is never sufficient.
pub trait CheckedDriverControls: Send + Sync {
    fn capabilities(&self) -> CheckedControlCapabilities;
    /// Fixed-size data reader; no effect and no native-owner lifetime retained.
    fn runner_observation(&self) -> crate::RunnerObservationReader {
        crate::RunnerObservationReader::unsupported()
    }
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
    /// Resolves once the host reader has observed genuine physical pipe EOF
    /// (or startup abort), so the driver exit task can run bounded native finalization.
    fn wait_ready_to_finalize(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
    /// Bounded finalization/reap of the original native process and I/O workers.
    fn finalize_checked_owner(
        &self,
        _wait_until: std::time::Instant,
    ) -> Pin<Box<dyn Future<Output = io::Result<CheckedOwnerFinalization>> + Send + '_>> {
        Box::pin(async {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "checked owner finalization is unavailable",
            ))
        })
    }
}

#[cfg(test)]
mod observation_tests {
    use super::*;
    struct UnsupportedDriver;
    impl CheckedDriverControls for UnsupportedDriver {
        fn capabilities(&self)->CheckedControlCapabilities { CheckedControlCapabilities::default() }
        fn close_stdin(&self)->Pin<Box<dyn Future<Output=io::Result<()>>+Send+'_>> {
            Box::pin(async {Err(io::Error::from(io::ErrorKind::Unsupported))})
        }
        fn resize(&self,_:TerminalSize)->Pin<Box<dyn Future<Output=io::Result<()>>+Send+'_>> {
            Box::pin(async {Err(io::Error::from(io::ErrorKind::Unsupported))})
        }
    }
    #[test]
    fn existing_checked_driver_defaults_to_unsupported_data_without_control_or_future() {
        let driver:&dyn CheckedDriverControls=&UnsupportedDriver;
        let data=driver.runner_observation().read();
        assert_eq!(data.availability,crate::RunnerObservationAvailability::Unsupported);
        assert!(data.key.is_none());assert!(data.revision.is_none());assert!(data.snapshot.is_none());
    }
}
