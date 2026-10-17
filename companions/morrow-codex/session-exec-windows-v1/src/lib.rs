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
mod runner_observation;
#[cfg(windows)]
mod native_completion;
#[cfg(windows)]
pub use native_completion::{ReviewedNativeCompletion,NativeExecutionCompletion};
#[cfg(windows)]
mod native_factory;
#[cfg(windows)]
pub use native_factory::{NativeFactoryRetirement,NativeFactoryCompletion,RetiredExecutionCompletion};
#[cfg(windows)]
pub use runner_observation::ReviewedRunnerObservation;
#[cfg(windows)]
mod provisioned;
mod state;
#[cfg(windows)]
mod windows_runner;

#[cfg(windows)]
pub use borrowed::{BorrowedStopHandle, BorrowedWindowsExecutionPort, ReviewedBorrowedInvocation};
#[cfg(windows)]
pub use borrowed::ReviewedStartObservation;
#[cfg(windows)]
pub use codex_sandboxing::{
    WindowsStartSnapshot, WindowsStartAttempt,
    ContextFact, ContextUnavailable, QueryHandleClose, StartupAccessResult,
    StartupApplicationSource, StartupCallParameters, StartupObjectObservation,
    StartupTokenRelations, WindowsStartupContext,
    ContextApiStage, ContextFailure, TokenOpenPurpose, TokenOutputAllocation,
    TokenAllocationObservation, TokenOpenObservation, PrimaryTokenReadResults, StartupQueryStages,
};
pub use codex_exec_server::{
    ExecBackend, ExecParams, ExecServerRuntimeOptions, ProcessId, StartedExecProcess,
};
pub use codex_http_client::{HttpClientFactory, OutboundProxyPolicy};
pub use codex_sandboxing::SandboxType;
#[cfg(windows)]
#[doc(hidden)]
pub use codex_sandboxing::{asuser_parent_dispatch,AsUserParentOrigin};
#[cfg(windows)]
#[doc(hidden)]
pub use codex_sandboxing::{ParentOriginalProcess,ParentCreate,ParentChildIdentity,ParentNativeExit,ParentCClosed};
#[cfg(windows)]
pub use codex_sandboxing::{AsUserMaterialization,AsUserFactoryBinding};
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
