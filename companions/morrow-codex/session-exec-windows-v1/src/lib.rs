//! Native Windows ownership bridge over the actual pinned upstream ExecBackend.
//! No guest-selected backend, manufactured process, or replacement OS sandbox.
#![deny(unsafe_code)]

#[cfg(windows)]
mod artifact;
#[cfg(windows)]
mod connection;
mod process;
mod registry;
pub use registry::{MAX_NATIVE_EXECUTIONS, NativeExecutionRegistry, NativeExecutionStatus};
#[cfg(windows)]
mod borrowed;
#[cfg(windows)]
mod facts;
#[cfg(windows)]
mod provisioned;
mod state;
#[cfg(windows)]
mod windows_runner;

#[cfg(windows)]
pub use borrowed::{BorrowedStopHandle, BorrowedWindowsExecutionPort, ReviewedBorrowedInvocation};
pub use codex_exec_server::{
    ExecBackend, ExecParams, ExecServerRuntimeOptions, ProcessId, StartedExecProcess,
};
pub use codex_http_client::{HttpClientFactory, OutboundProxyPolicy};
pub use codex_sandboxing::SandboxType;
#[cfg(windows)]
pub use facts::{BorrowedNativeResources, FactsDrainReport, PendingFactStatus};
#[cfg(windows)]
pub use provisioned::ProvisionedWindowsBackend;
#[cfg(windows)]
pub use windows_runner::WindowsRunnerProvisioningSpec;

#[cfg(windows)]
pub use connection::{
    ReviewedWindowsInvocation, WindowsExecutionPort, fixed_intent, policy_domain,
};
pub use process::WindowsProcessProvider;
pub use state::{NativeR2State, SharedNativeR2State};
