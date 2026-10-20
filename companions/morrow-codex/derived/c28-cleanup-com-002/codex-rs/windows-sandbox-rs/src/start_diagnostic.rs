//! Per-invocation data-only observations of the existing Windows start path.
//! A returned backend handle is not a child exit, EOF, acknowledgment or join.
use std::fmt;
use std::io::ErrorKind;
use std::sync::Arc;
use std::sync::Mutex;

#[cfg(windows)]
#[path = "startup_context.rs"]
pub(crate) mod context_query;

/// Finite historical scalars only. No SID, name, descriptor, token or callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextUnavailable {
    NotAttempted, NotApplicable, OriginalUnavailable, Busy, Poisoned,
    BoundExceeded, InvalidBuffer, Win32 { code: u32 },
    /// The named upstream operation prevented this operation from running.
    /// This is not a return value from the downstream API.
    Upstream { stage: ContextApiStage, reason: ContextFailure },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextFailure {
    NotAttempted, NotApplicable, OriginalUnavailable, Busy, Poisoned,
    BoundExceeded, InvalidBuffer, Win32 { code: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ContextApiStage {
    QueryPrimaryOpen, QueryUser, QueryLogonGroups, QuerySession,
    QueryRestrictions, QueryAppContainer, QueryPrimaryClose,
    CallerUser, CallerLogonGroups, CallerSession, IntendedSid,
    DuplicateSourceOpen, DuplicateToken, StationDescriptor, DesktopDescriptor,
    StationVisibility, DesktopVisibility,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TokenOpenPurpose { #[default] NotRequested, PrimaryQuery, DuplicationSource }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenOutputAllocation { NotAllocated, AllocatedOriginal, UncertainOutput }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenAllocationObservation {
    pub api_returned_success: bool,
    pub allocation: TokenOutputAllocation,
    pub win32_error: Option<u32>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TokenOpenObservation {
    pub purpose: TokenOpenPurpose,
    /// Actual passed mask when result is Observed; zero if the API was skipped.
    pub requested_mask: u32,
    /// Observed means the API was called; its success/failure is inside.
    pub result: ContextFact<TokenAllocationObservation>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrimaryTokenReadResults {
    pub user: ContextFact<()>,
    pub logon_groups: ContextFact<()>,
    pub session: ContextFact<()>,
    pub restrictions: ContextFact<()>,
    pub app_container: ContextFact<()>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartupQueryStages {
    pub primary_open: TokenOpenObservation,
    pub primary_reads: PrimaryTokenReadResults,
    pub duplicate_source_open: TokenOpenObservation,
    /// DuplicateToken at SecurityIdentification; never used for impersonation.
    pub impersonation_duplicate: ContextFact<TokenAllocationObservation>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextFact<T> { Observed(T), NotObserved(ContextUnavailable) }
impl<T> Default for ContextFact<T> {
    fn default() -> Self { Self::NotObserved(ContextUnavailable::NotAttempted) }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupApplicationSource { VerifiedResolvedPinnedImage, RegisteredAliasNotYetVerified }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupCallParameters {
    pub creation_flags: u32,
    pub logon_flags: u32,
    pub registered_profile_branch: bool,
    /// NULL asks CreateProcessWithLogonW for the specified user's profile environment.
    /// No claim that profile loading or particular environment values were observed.
    pub environment_is_null_user_profile_source: bool,
    pub application_source: StartupApplicationSource,
    pub cwd_equals_request_cwd: bool,
    /// Lexical relation to request roots, not an independent root source attestation.
    pub cwd_under_request_workspace_root: ContextFact<bool>,
    pub desktop_pointer_is_same_owned_qualified_name: bool,
    pub station_still_same_borrowed_object: ContextFact<bool>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryHandleClose { NotAllocated, Held, ClosedVerified, CloseFailed { code: u32 } }
impl Default for QueryHandleClose { fn default()->Self { Self::NotAllocated } }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartupTokenRelations {
    pub user_equals_intended_sandbox: ContextFact<bool>,
    pub user_equals_caller_user: ContextFact<bool>,
    pub logon_sid_equals_caller_logon_sid: ContextFact<bool>,
    pub runner_logon_group_attributes: ContextFact<u32>,
    pub session_equals_caller_session: ContextFact<bool>,
    pub has_restrictions: ContextFact<bool>,
    pub is_app_container: ContextFact<bool>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupAccessResult {
    pub access_status: bool,
    pub granted_mask: u32,
    pub denial_win32_error: Option<u32>,
}
/// Local Authz only: never an object-open, MIC/SACL/CAP, native-start or ACK proof.
/// Explicit byte tags keep the two-attempt Copy history bounded. Error numbers
/// are meaningful only for the corresponding Failed state, never a boolean PASS.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum AuthzAllocation {
    #[default] NotAttempted, InFlight, FailedNoOutput, Held,
    FailedUncertainOutput, InvalidSuccessOutput,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum AuthzRetirement {
    #[default] NotAllocated, Held, InFlight, FreedVerified, Failed,
    BlockedByContext, Uncertain,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AuthzResourceObservation {
    pub allocation: AuthzAllocation,
    pub retirement: AuthzRetirement,
    pub initialization_error: u32,
    pub retirement_error: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum AuthzObjectState {
    #[default] NotAttempted, PrimaryUnavailable, RestrictedToken, AppContainer,
    ResourceManagerUnavailable, ContextUnavailable, ObjectIdentityUnavailable,
    DescriptorUnavailable, VisibilityUnavailable, InvalidBuffer, BoundExceeded,
    NullOrAbsentDacl, OverlappingLayout, UnsupportedAce, PrincipalSelf,
    InFlight, ApiFailed, ReplyReturned,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum AuthzObjectMapping {
    #[default] NotSelected, NoninteractiveStation, VisibleStation, Desktop,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AuthzObjectObservation {
    pub state: AuthzObjectState,
    pub mapping: AuthzObjectMapping,
    /// ApiFailed: immediate Win32 error. ReplyReturned: reply.Error[0].
    /// DescriptorUnavailable: original descriptor Win32 error, if any; its full
    /// classified reason remains in the original object's descriptor_read.
    pub error: u32,
    /// Defined only for ReplyReturned; never consumed on API failure/unwind.
    pub granted_mask: u32,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub struct AuthzDaclProjection {
    // Init order is RM -> context; failure stops initialization. Free order is
    // context -> RM; failure blocks the dependency. Thus at most ONE init error
    // and ONE free error can exist, independently. Sharing each number loses no
    // returned error; the typed tags identify which API actually returned it.
    initialization_error:u32, retirement_error:u32,
    station_error:u32, station_mask:u32, desktop_error:u32, desktop_mask:u32,
    manager_allocation:AuthzAllocation, context_allocation:AuthzAllocation,
    manager_retirement:AuthzRetirement, context_retirement:AuthzRetirement,
    station_state:AuthzObjectState, station_mapping:AuthzObjectMapping,
    desktop_state:AuthzObjectState, desktop_mapping:AuthzObjectMapping,
}
const _: ()=assert!(std::mem::size_of::<AuthzDaclProjection>()==32);
impl AuthzDaclProjection {
    pub(crate) fn from_observations(manager:AuthzResourceObservation,context:AuthzResourceObservation,
        station:AuthzObjectObservation,desktop:AuthzObjectObservation)->Self {
        let init_failed=|v|matches!(v,AuthzAllocation::FailedNoOutput|AuthzAllocation::FailedUncertainOutput);
        // Only the original sequential producer calls this constructor. It never
        // initializes context after RM failure or frees RM after context failure.
        Self {initialization_error:if init_failed(manager.allocation) {manager.initialization_error}
                else{context.initialization_error},
            retirement_error:if context.retirement==AuthzRetirement::Failed {context.retirement_error}
                else{manager.retirement_error},
            station_error:station.error,station_mask:station.granted_mask,
            desktop_error:desktop.error,desktop_mask:desktop.granted_mask,
            manager_allocation:manager.allocation,context_allocation:context.allocation,
            manager_retirement:manager.retirement,context_retirement:context.retirement,
            station_state:station.state,station_mapping:station.mapping,
            desktop_state:desktop.state,desktop_mapping:desktop.mapping}
    }
    pub fn resource_manager(&self)->AuthzResourceObservation {
        self.resource(self.manager_allocation,self.manager_retirement)
    }
    pub fn context(&self)->AuthzResourceObservation {
        self.resource(self.context_allocation,self.context_retirement)
    }
    fn resource(&self,allocation:AuthzAllocation,retirement:AuthzRetirement)->AuthzResourceObservation {
        AuthzResourceObservation {allocation,retirement,
            initialization_error:if matches!(allocation,AuthzAllocation::FailedNoOutput|AuthzAllocation::FailedUncertainOutput)
                {self.initialization_error}else{0},
            retirement_error:if retirement==AuthzRetirement::Failed {self.retirement_error}else{0}}
    }
    pub fn station(&self)->AuthzObjectObservation {
        AuthzObjectObservation {state:self.station_state,mapping:self.station_mapping,
            error:self.station_error,granted_mask:self.station_mask}
    }
    pub fn desktop(&self)->AuthzObjectObservation {
        AuthzObjectObservation {state:self.desktop_state,mapping:self.desktop_mapping,
            error:self.desktop_error,granted_mask:self.desktop_mask}
    }
    /// Fixed operation order, evaluated from retained facts, without another query.
    /// None is not a permission or native-completion proof. Missing evidence stays
    /// visible in the individual states rather than being converted into error 0.
    pub fn first_failure(&self)->Option<(&'static str,Option<u32>)> {
        for (name,value) in [("InitializeResourceManager",self.resource_manager()),
            ("InitializeContextFromToken",self.context())] {
            match value.allocation {
                AuthzAllocation::FailedNoOutput|AuthzAllocation::FailedUncertainOutput=>
                    return Some((name,Some(value.initialization_error))),
                AuthzAllocation::InFlight|AuthzAllocation::InvalidSuccessOutput=>return Some((name,None)),
                _=>{},
            }
        }
        for (name,value) in [("StationAuthzAccessCheck",self.station()),("DesktopAuthzAccessCheck",self.desktop())] {
            match value.state {
                AuthzObjectState::ApiFailed=>return Some((name,Some(value.error))),
                AuthzObjectState::ReplyReturned if value.error!=0=>return Some((name,Some(value.error))),
                AuthzObjectState::ReplyReturned if value.granted_mask==0=>return Some((name,None)),
                AuthzObjectState::InFlight=>return Some((name,None)),
                _=>{},
            }
        }
        for (name,value) in [("FreeContext",self.context()),("FreeResourceManager",self.resource_manager())] {
            match value.retirement {
                AuthzRetirement::Failed=>return Some((name,Some(value.retirement_error))),
                AuthzRetirement::InFlight|AuthzRetirement::Uncertain=>return Some((name,None)),
                _=>{},
            }
        }
        None
    }
}
impl fmt::Debug for AuthzDaclProjection {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {
        f.debug_struct("AuthzDaclProjection")
            .field("scope",&"QUERY8_LOCAL_RM5_MAPPED_SIMPLE_DACL_ONLY_MIC_SACL_CAP_NATIVE_NOT_PROVEN")
            .field("resource_manager",&self.resource_manager()).field("context",&self.context())
            .field("station",&self.station()).field("desktop",&self.desktop())
            .field("first_failure",&self.first_failure()).finish()
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartupObjectObservation {
    pub name_equals_creation_object: ContextFact<bool>,
    pub visible: ContextFact<bool>,
    /// Desktop UOI_IO only; independent of station WSF_VISIBLE and access.
    pub receiving_user_input: ContextFact<bool>,
    pub descriptor_owner_equals_runner_user: ContextFact<bool>,
    pub descriptor_group_equals_runner_logon_sid: ContextFact<bool>,
    pub dacl_present: ContextFact<bool>,
    pub dacl_null: ContextFact<bool>,
    pub descriptor_read: ContextFact<()>,
    pub access_check_invoked: bool,
    /// AccessCheck(MAXIMUM_ALLOWED), not a startup/ACK/cleanup permission.
    pub access: ContextFact<StartupAccessResult>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowsStartupContext {
    pub startup_id: u64,
    pub call: ContextFact<StartupCallParameters>,
    pub original_runner_identity: Option<(u32, u64)>,
    /// Legacy entry marker only, not a successful token/API observation.
    pub query_status: ContextFact<()>,
    pub query_stages: StartupQueryStages,
    /// Latched original pre-query sample, independent of later connect updates.
    pub runner_before_token_query: ContextFact<WindowsStartRunnerObservation>,
    /// One original zero-wait sample after this finite query sequence.
    /// Neither sample is an atomic interval or proof user code started.
    pub runner_after_token_query: ContextFact<WindowsStartRunnerObservation>,
    pub token: StartupTokenRelations,
    pub station: StartupObjectObservation,
    pub desktop: StartupObjectObservation,
    /// Retained two-slot projection for existing readers: primary QUERY, derived token.
    /// Normal retirement checks ALL THREE real slots, never this projection.
    pub query_handle_closes: [QueryHandleClose; 2],
    /// Independent primary QUERY, DUP source, derived impersonation slots.
    pub query_slot_closes: [QueryHandleClose; 3],
    /// Same original QUERY, independently labelled, finite non-operational history.
    pub authz_dacl: AuthzDaclProjection,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsStartStage {
    #[default]
    NotObserved,
    RunnerSelection,
    RequestPrepare,
    ProcessMapReserve,
    MatchedRouteCheck,
    CodexHomeResolve,
    PermissionResolve,
    SandboxBaseReady,
    AccountSelect,
    SetupRefresh,
    CapabilityResolve,
    BlockingTransportTask,
    DesktopPolicy,
    RunnerResolve,
    RunnerPinVerify,
    RegisteredAlias,
    PrivateDesktop,
    PipeCreateIn,
    PipeCreateOut,
    RunnerLogon,
    RegisteredImageVerify,
    PipeConnectIn,
    PipeConnectOut,
    ControlHello,
    SpawnRequestWrite,
    SpawnReadyRead,
    CheckedDriverAssemble,
    SpawnReturned,
    // Original AsUser preparation boundaries, not proof of native absence.
    AsUserFilesystemPolicy,
    AsUserOfflinePolicy,
    AsUserHomeAndCwdIdentity,
    AsUserWriteRoots,
    AsUserReadDeny,
    AsUserWriteDeny,
    AsUserReadRoots,
    AsUserExistingCapabilities,
    AsUserCapabilitySid,
    AsUserCapabilityCount,
    AsUserOwnedCredentials,
    // Same-call read-root derivation/validation only; not native absence or completion.
    AsUserReadRootsOverride,
    AsUserReadRootsDefault,
    AsUserReadRootsRequestNormalize,
    AsUserReadRootsAppliedNormalize,
    AsUserReadRootsEquality,
    // Same-invocation attempted transport boundaries, never native absence,
    // successful completion, permission or cleanup/replay authority.
    CheckedStartupReserve,
    BlockingTransportDispatch,
    BlockingTransportEntered,
    CheckedTransportCallerAck,
    AsUserTransportReservation,
    AsUserTransportPolicy,
    AsUserNativeReservation,
    AsUserCallerAdmission,
    AsUserCommandPrepare,
    AsUserStartupEntropy,
    AsUserNativeStart,
    AsUserAdmissionBind,
    AsUserPipeNames,
    AsUserTransportFiles,
    AsUserNativeTransfer,
    AsUserConnectInput,
    AsUserConnectOutput,
    AsUserBootstrapPrepare,
    AsUserRuntimeLookup,
    AsUserBootstrapWrite,
    AsUserControlsHandshake,
    AsUserSpawnRequestWrite,
    AsUserSpawnReadyRead,
    AsUserTransportLeaseCommit,
}

/// Finite resolver rejection, with the original public error text preserved.
/// No policy path, account, capability or request content is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PermissionProfileResolveError {
    ManagedProfileRequired,
    RestrictedFilesystemRequired,
}

impl std::fmt::Display for PermissionProfileResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ManagedProfileRequired =>
                "only managed permission profiles can be enforced by the Windows sandbox",
            Self::RestrictedFilesystemRequired =>
                "only restricted managed filesystem permissions can be enforced by the Windows sandbox",
        })
    }
}

impl std::error::Error for PermissionProfileResolveError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsStartError {
    #[default]
    NotObserved,
    InvalidParams,
    InvalidRequest,
    ServerInternal,
    OtherRpc,
    RouteRejected,
    PinRejected,
    IdentityRejected,
    ControlRejected,
    IoAccessDenied,
    IoNotFound,
    IoInvalidInput,
    IoTimedOut,
    IoInterrupted,
    IoBrokenPipe,
    IoOther,
    OsReported,
    RunnerReported,
    BlockingCancelled,
    BlockingPanic,
    OriginalRunnerExitedBeforeConnect,
    OriginalRunnerWaitUnavailable,
    Unclassified,
    ManagedProfileRequired,
    RestrictedFilesystemRequired,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsRunnerErrorStage {
    #[default]
    NotObserved,
    ReadSpawnRequest,
    SpawnChild,
    WriteSpawnReady,
    TerminalWitness,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsOsError {
    #[default]
    NotObserved,
    AccessDenied,
    FileNotFound,
    PathNotFound,
    LogonFailure,
    AccountDisabled,
    NoLogonSession,
    OperationAborted,
    TimedOut,
    Other,
}

impl WindowsOsError {
    /// Consume an already observed code; never retain or print the raw number.
    pub fn from_code(code: u32) -> Self {
        match code {
            5 => Self::AccessDenied,
            2 => Self::FileNotFound,
            3 => Self::PathNotFound,
            1326 => Self::LogonFailure,
            1331 => Self::AccountDisabled,
            1312 => Self::NoLogonSession,
            995 => Self::OperationAborted,
            258 | 1460 => Self::TimedOut,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsStartCompletion {
    #[default]
    NotObserved,
    InProgressOrUnwound,
    ReturnedError,
    ReturnedBackendHandle,
    UnsupportedBackend,
}

/// Historical observation of the original RUNNER, never the business child.
/// No HANDLE, nonce, account, command line or operational authority is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsStartRunnerState {
    RunningAtObservation,
    Exited { raw_dword: u32 },
    SignaledExitQueryFailed { os_class: WindowsOsError },
    WaitFailed { os_class: WindowsOsError },
    UnexpectedWait { wait_result: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowsStartRunnerObservation {
    pub startup_id: u64,
    pub runner_pid: u32,
    pub runner_creation_filetime_100ns: u64,
    pub state: WindowsStartRunnerState,
}

/// Same original A1 owner's saved first-failure stage, not an exact Win32 API.
/// These historical tags confer no HANDLE, retry, cleanup or admission authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WindowsAsUserNativeStage {
    Admission, ThreadToken, Login, Grant, Token, TokenSecurity, Broker,
    ProcessSd, ThreadSd, Create, ProcessReadback, ThreadReadback,
    Transfer, Retirement, Unwind, Deadline,
}
/// Source operation in the same saved first failure. Result sites include the
/// existing output-slot validation. Only an actual saved raw error at a direct
/// call/result site can identify that call's reported code. Boundary sites
/// (notably subject/query/descriptor/channel/readback) may contain several APIs;
/// None never proves an API rejection, a successful call, or process absence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum WindowsAsUserNativeSite {
    #[default]
    NotObserved,
    AdmissionBind,
    PipeNamesGuard,
    StartCommandGuard,
    LoginInputGuard,
    LoginCallerIdentity,
    LoginCallerBinding,
    ResolveObjectQuery,
    ThreadTokenReservation,
    OpenThreadTokenResult,
    ThreadTokenAbsenceGuard,
    PrepareCaller,
    LoginReservation,
    LogonUserResult,
    TokenHandleInformation,
    TokenInheritanceGuard,
    TargetFacts,
    TargetPolicyGuard,
    TokenSecurity,
    DefaultInitialization,
    TransportPipeInputs,
    TransportPipeDescriptor,
    TransportPipeReservation,
    CreateNamedPipeResult,
    TransportPipeCheckpoint,
    StageOriginalTransport,
    BrokerInputs,
    BrokerStart,
    TargetBeforeCreate,
    CreateInputGuard,
    ProcessDescriptor,
    ThreadDescriptor,
    CallerBeforeCreate,
    CreateCommandGuard,
    CreateReservation,
    CreateProcessAsUserResult,
    CreateOutputIdentityGuard,
    CallerAfterCreate,
    NativeReadback,
    DeadlineGuard,
    Unwind,
    BrokerInputGuard,
    BrokerDeadline,
    BrokerCallerIdentity,
    BrokerOriginalBinding,
    BrokerTableGuard,
    BrokerNoImpersonation,
    BrokerCallerSubject,
    BrokerEntropy,
    BrokerNonceGuard,
    BrokerDescriptor,
    BrokerPipeReservation,
    BrokerCreateNamedPipeResult,
    BrokerChannelPrepare,
    BrokerSpawn,
    BrokerConnect,
    BrokerGetClientPidResult,
    BrokerPeerPidGuard,
    BrokerPeerSubject,
    BrokerPeerContextGuard,
    BrokerBinding,
    BrokerHelloSend,
    BrokerOfferReceive,
    BrokerPullStation,
    BrokerPullDesktop,
    BrokerGuiIdentity,
    BrokerAcceptSend,
    BrokerClosedReceive,
    BrokerProcessWait,
    BrokerProcessExitCodeResult,
    BrokerProcessExitGuard,
    BrokerThreadWait,
    BrokerThreadExitCodeResult,
    BrokerThreadExitGuard,
    BrokerReaderWait,
    BrokerReaderExitCodeResult,
    BrokerReaderCapture,
    BrokerReaderCompletionGuard,
    BrokerReaderClose,
    BrokerOriginalClose,
    BrokerCompletion,
    HelperSpawnDeadline,
    HelperStdoutPipePair,
    HelperStderrPipePair,
    HelperNulReservation,
    HelperCreateNulResult,
    HelperParentReservation,
    HelperDuplicateParentResult,
    HelperAttributeList,
    HelperCommandGuard,
    HelperProcessDescriptor,
    HelperThreadDescriptor,
    HelperCreateReservation,
    HelperCreateProcessResult,
    HelperIdentity,
    HelperIdentityGuard,
    HelperImageReadback,
    HelperPrivateClose,
    HelperReaderReservation,
    HelperCreateReaderResult,
    HelperReaderIdGuard,
    LaunchTokenDerivation,
}

pub use crate::asuser_a1_target::WindowsTargetPolicyRejection;

/// Values copied from the owner's already captured first Failure only.
/// IoAccessDenied is an ErrorKind classification; absent raw codes stay absent.
/// A raw code identifies no specific API when the retained stage covers several.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowsAsUserNativeFailure {
    pub owner_stage: WindowsAsUserNativeStage,
    pub site: WindowsAsUserNativeSite,
    /// Same original target guard failure only; never reconstructed from IoAccessDenied.
    pub target_policy_rejection: Option<WindowsTargetPolicyRejection>,
    pub error: WindowsStartError,
    pub win32: Option<i32>,
    pub ntstatus: Option<i32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowsStartAttempt {
    pub stage: WindowsStartStage,
    pub error: WindowsStartError,
    pub runner_stage: WindowsRunnerErrorStage,
    pub os_class: WindowsOsError,
    pub original_runner: Option<WindowsStartRunnerObservation>,
    pub startup_context: Option<WindowsStartupContext>,
    pub asuser_native_failure: Option<WindowsAsUserNativeFailure>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowsStartSnapshot {
    pub current: WindowsStartAttempt,
    /// Only the existing credential refresh may begin a second subattempt.
    pub first_attempt: Option<WindowsStartAttempt>,
    pub attempt: u8,
    pub completion: WindowsStartCompletion,
}

/// Finite enums and original-runner historical scalars only: no callback,
/// executable, HANDLE or resource owner.
#[derive(Clone, Default)]
pub struct WindowsStartDiagnostic(Arc<Mutex<WindowsStartSnapshot>>);

impl WindowsStartDiagnostic {
    /// Same invocation only; observation never waits or modifies native ownership.
    pub(crate) fn observe_asuser_native_failure(&self, owner_stage: WindowsAsUserNativeStage,
        site: WindowsAsUserNativeSite, kind: ErrorKind, win32: Option<i32>, ntstatus: Option<i32>) {
        self.observe_asuser_native_failure_with_target_policy(owner_stage,site,kind,win32,ntstatus,None);
    }
    /// Copy only the same original first failure's finite reason. No wait/query,
    /// callback, native ownership, retry or overwrite of completed/first history.
    pub(crate) fn observe_asuser_native_failure_with_target_policy(&self,
        owner_stage: WindowsAsUserNativeStage, site: WindowsAsUserNativeSite,
        kind: ErrorKind, win32: Option<i32>, ntstatus: Option<i32>,
        target_policy_rejection: Option<WindowsTargetPolicyRejection>) {
        let target_policy_rejection=if owner_stage==WindowsAsUserNativeStage::Token
            &&site==WindowsAsUserNativeSite::TargetPolicyGuard
            &&kind==ErrorKind::PermissionDenied&&win32.is_none()&&ntstatus.is_none()
            {target_policy_rejection}else{None};
        if let Ok(mut state)=self.0.try_lock() {
            if !matches!(state.completion,WindowsStartCompletion::NotObserved
                |WindowsStartCompletion::InProgressOrUnwound)
                ||state.current.error!=WindowsStartError::NotObserved
                ||state.current.asuser_native_failure.is_some() {return;}
            state.current.asuser_native_failure=Some(WindowsAsUserNativeFailure {
                owner_stage,site,target_policy_rejection,error:Self::io_error_class(kind),win32,ntstatus,
            });
        }
    }
    /// Finite substeps inside the existing single native-cell borrow. A busy or
    /// poisoned recorder drops this optional observation, never blocks the owner.
    pub(crate) fn observe_asuser_progress(&self, stage: WindowsStartStage) {
        if !matches!(stage,WindowsStartStage::AsUserAdmissionBind
            |WindowsStartStage::AsUserPipeNames|WindowsStartStage::AsUserNativeStart) {return;}
        if let Ok(mut state)=self.0.try_lock() {
            if matches!(state.completion,WindowsStartCompletion::NotObserved
                |WindowsStartCompletion::InProgressOrUnwound)
                &&state.current.error==WindowsStartError::NotObserved {
                state.current.stage=stage;
            }
        }
    }
    pub(crate) fn observe_startup_context(&self, mut observed: WindowsStartupContext) {
        // Observation must not wait for a data reader or a poisoned recorder.
        if let Ok(mut state) = self.0.try_lock() {
            if !matches!(state.completion,WindowsStartCompletion::NotObserved
                |WindowsStartCompletion::InProgressOrUnwound) {return;}
            for sample in [observed.runner_before_token_query,observed.runner_after_token_query] {
                if let ContextFact::Observed(sample)=sample {
                    if sample.startup_id!=observed.startup_id
                        || observed.original_runner_identity!=Some((
                            sample.runner_pid,sample.runner_creation_filetime_100ns)) {return;}
                }
            }
            if let Some(original)=state.current.original_runner {
                if original.startup_id!=observed.startup_id
                    || observed.original_runner_identity.is_some_and(|identity|identity!=(
                        original.runner_pid,original.runner_creation_filetime_100ns))
                {return;}
            }
            if let Some(prior) = state.current.startup_context {
                if prior.startup_id != observed.startup_id
                    || (prior.original_runner_identity.is_some()
                        && prior.original_runner_identity != observed.original_runner_identity)
                { return; }
                // The first completed finite sequence is historical. Later
                // connect samples must never replace its before/after window.
                if matches!(prior.query_status,ContextFact::Observed(())) {return;}
                if observed.call==ContextFact::NotObserved(ContextUnavailable::NotAttempted) {
                    observed.call=prior.call;
                }
            }
            // Exactly one pre-call and one anchored post-call projection.
            // Never turn query failures into the execution's sticky first error.
            state.current.startup_context = Some(observed);
        }
    }
    pub(crate) fn startup_context_unavailable(&self,startup_id:u64,reason:ContextUnavailable) {
        if let Ok(mut state)=self.0.try_lock() {
            if !matches!(state.completion,WindowsStartCompletion::NotObserved
                |WindowsStartCompletion::InProgressOrUnwound) {return;}
            let context=state.current.startup_context.get_or_insert_with(WindowsStartupContext::default);
            if context.startup_id!=0 && context.startup_id!=startup_id {return;}
            if matches!(context.query_status,ContextFact::Observed(())) {return;}
            if context.call==ContextFact::NotObserved(ContextUnavailable::NotAttempted) {
                context.call=ContextFact::NotObserved(reason);
            }
            context.startup_id=startup_id;context.query_status=ContextFact::NotObserved(reason);
            context.token=StartupTokenRelations {
                user_equals_intended_sandbox:ContextFact::NotObserved(reason),
                user_equals_caller_user:ContextFact::NotObserved(reason),
                logon_sid_equals_caller_logon_sid:ContextFact::NotObserved(reason),
                runner_logon_group_attributes:ContextFact::NotObserved(reason),
                session_equals_caller_session:ContextFact::NotObserved(reason),
                has_restrictions:ContextFact::NotObserved(reason),
                is_app_container:ContextFact::NotObserved(reason),
            };
            context.station.access=ContextFact::NotObserved(reason);
            context.desktop.access=ContextFact::NotObserved(reason);
        }
    }
    pub(crate) fn observe_original_runner(&self, observed: WindowsStartRunnerObservation) {
        self.record(|state| {
            if let Some(original) = state.current.original_runner {
                if (original.startup_id, original.runner_pid, original.runner_creation_filetime_100ns)
                    != (observed.startup_id, observed.runner_pid, observed.runner_creation_filetime_100ns)
                {
                    if state.current.error == WindowsStartError::NotObserved {
                        state.current.error = WindowsStartError::IdentityRejected;
                    }
                    return; // Never replace a different original with a later query.
                }
                if matches!(original.state, WindowsStartRunnerState::Exited { .. }) { return; }
            }
            state.current.original_runner = Some(observed);
        });
    }

    fn record(&self, change: impl FnOnce(&mut WindowsStartSnapshot)) {
        if let Ok(mut state) = self.0.lock() {
            if matches!(
                state.completion,
                WindowsStartCompletion::NotObserved | WindowsStartCompletion::InProgressOrUnwound
            ) {
                change(&mut state);
            }
        }
    }

    pub fn snapshot(&self) -> Option<WindowsStartSnapshot> {
        self.0.lock().ok().map(|state| *state)
    }

    pub fn mark(&self, stage: WindowsStartStage) {
        self.record(|state| {
            if state.current.error == WindowsStartError::NotObserved {
                state.current.stage = stage;
            }
            state.attempt = state.attempt.max(1);
            state.completion = WindowsStartCompletion::InProgressOrUnwound;
        });
    }

    fn failure(
        &self,
        error: WindowsStartError,
        runner_stage: WindowsRunnerErrorStage,
        os_class: WindowsOsError,
    ) {
        if error == WindowsStartError::NotObserved {
            return;
        }
        self.record(|state| {
            if state.current.error == WindowsStartError::NotObserved {
                state.current.error = error;
                state.current.runner_stage = runner_stage;
                state.current.os_class = os_class;
            }
            state.attempt = state.attempt.max(1);
            state.completion = WindowsStartCompletion::InProgressOrUnwound;
        });
    }

    pub fn fail(&self, error: WindowsStartError) {
        self.failure(
            error,
            WindowsRunnerErrorStage::NotObserved,
            WindowsOsError::NotObserved,
        );
    }

    /// Classify only the error already returned by the same backend call.
    /// Never format the error chain or retain its paths/credentials/free text.
    pub(crate) fn observe_backend_error(&self, error: &anyhow::Error) {
        if let Some(cause) = error.downcast_ref::<PermissionProfileResolveError>() {
            self.fail(match cause {
                PermissionProfileResolveError::ManagedProfileRequired =>
                    WindowsStartError::ManagedProfileRequired,
                PermissionProfileResolveError::RestrictedFilesystemRequired =>
                    WindowsStartError::RestrictedFilesystemRequired,
            });
        } else if let Some(error) = error.downcast_ref::<std::io::Error>() {
            // Ordinary IO preserves the baseline finite ErrorKind category.
            // It is not a runner/native OS observation.
            self.fail_io_kind(error.kind());
        } else {
            self.fail(WindowsStartError::Unclassified);
        }
    }

    fn io_error_class(kind: ErrorKind) -> WindowsStartError {
        match kind {
            ErrorKind::PermissionDenied => WindowsStartError::IoAccessDenied,
            ErrorKind::NotFound => WindowsStartError::IoNotFound,
            ErrorKind::InvalidInput => WindowsStartError::IoInvalidInput,
            ErrorKind::TimedOut => WindowsStartError::IoTimedOut,
            ErrorKind::Interrupted => WindowsStartError::IoInterrupted,
            ErrorKind::BrokenPipe => WindowsStartError::IoBrokenPipe,
            _ => WindowsStartError::IoOther,
        }
    }

    pub fn fail_io_kind(&self, kind: ErrorKind) {
        self.fail(Self::io_error_class(kind));
    }

    pub fn fail_os(&self, class: WindowsOsError) {
        self.failure(
            WindowsStartError::OsReported,
            WindowsRunnerErrorStage::NotObserved,
            class,
        );
    }

    pub fn fail_runner(&self, stage: WindowsRunnerErrorStage, os_class: WindowsOsError) {
        self.failure(WindowsStartError::RunnerReported, stage, os_class);
    }

    /// Observe an already selected original refresh branch; does not invoke it.
    pub fn begin_existing_retry(&self) {
        self.record(|state| {
            if state.first_attempt.is_none() {
                state.first_attempt = Some(state.current);
                state.current = WindowsStartAttempt::default();
                state.current.stage = WindowsStartStage::SetupRefresh;
                state.attempt = 2;
                state.completion = WindowsStartCompletion::InProgressOrUnwound;
            }
        });
    }

    pub fn finish_failed(&self) {
        self.record(|state| state.completion = WindowsStartCompletion::ReturnedError);
    }

    pub fn finish_returned_handle(&self) {
        self.record(|state| state.completion = WindowsStartCompletion::ReturnedBackendHandle);
    }

    pub fn unsupported(&self) {
        self.record(|state| state.completion = WindowsStartCompletion::UnsupportedBackend);
    }
}

impl fmt::Debug for WindowsStartDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.snapshot() {
            Some(snapshot) => snapshot.fmt(f),
            None => f.write_str("WindowsStartDiagnosticUnavailable"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asuser_error2357_is_same_call_first_error_and_not_rpc_or_native_evidence() {
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::PermissionResolve);
        d.mark(WindowsStartStage::AsUserHomeAndCwdIdentity);
        let error = anyhow::Error::new(std::io::Error::new(ErrorKind::PermissionDenied, "labelled io"))
            .context("labelled secret path that must never be exported");
        d.observe_backend_error(&error);
        d.fail(WindowsStartError::ServerInternal);
        d.finish_failed();
        let snapshot = d.snapshot().unwrap();
        assert_eq!(snapshot.current.stage, WindowsStartStage::AsUserHomeAndCwdIdentity);
        assert_eq!(snapshot.current.error, WindowsStartError::IoAccessDenied);
        assert_eq!(snapshot.current.os_class, WindowsOsError::NotObserved);
        assert_eq!(snapshot.current.original_runner, None);
        assert_eq!(snapshot.current.startup_context, None);
        assert_eq!(snapshot.completion, WindowsStartCompletion::ReturnedError);
        let output = format!("{snapshot:?}");
        assert!(!output.contains("labelled secret"));
        assert!(!output.contains("path that"));
        d.mark(WindowsStartStage::RunnerLogon);
        d.observe_backend_error(&anyhow::anyhow!("later error"));
        assert_eq!(d.snapshot(), Some(snapshot));
        assert!(std::mem::size_of::<WindowsStartSnapshot>() <= 2048);
    }

    #[cfg(windows)]
    #[test]
    fn returned_io2364_keeps_baseline_kind_without_native_os_inference() {
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::AsUserHomeAndCwdIdentity);
        d.observe_backend_error(&anyhow::Error::new(std::io::Error::from_raw_os_error(5)));
        let snapshot = d.snapshot().unwrap();
        assert_eq!(snapshot.current.error, WindowsStartError::IoAccessDenied);
        assert_eq!(snapshot.current.os_class, WindowsOsError::NotObserved);
        assert_eq!(snapshot.current.original_runner, None);
    }

    #[test]
    fn resolver_error2357_survives_context_and_success_stages_do_not_create_error() {
        for (cause, expected) in [
            (PermissionProfileResolveError::ManagedProfileRequired,
                WindowsStartError::ManagedProfileRequired),
            (PermissionProfileResolveError::RestrictedFilesystemRequired,
                WindowsStartError::RestrictedFilesystemRequired),
        ] {
            let d = WindowsStartDiagnostic::default();
            d.mark(WindowsStartStage::PermissionResolve);
            d.observe_backend_error(&anyhow::Error::new(cause).context("private context"));
            d.fail(WindowsStartError::ServerInternal);
            assert_eq!(d.snapshot().unwrap().current.stage, WindowsStartStage::PermissionResolve);
            assert_eq!(d.snapshot().unwrap().current.error, expected);
        }
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::PermissionResolve);
        d.mark(WindowsStartStage::AsUserFilesystemPolicy);
        d.mark(WindowsStartStage::AsUserOwnedCredentials);
        let snapshot = d.snapshot().unwrap();
        assert_eq!(snapshot.current.error, WindowsStartError::NotObserved);
        assert_eq!(snapshot.current.original_runner, None);
        let error = anyhow::anyhow!("unclassified private value");
        d.observe_backend_error(&error);
        assert_eq!(d.snapshot().unwrap().current.error, WindowsStartError::Unclassified);
        assert_eq!(d.snapshot().unwrap().current.stage, WindowsStartStage::AsUserOwnedCredentials);
        assert!(!format!("{:?}", d.snapshot()).contains("private value"));
    }

    #[test]
    fn query_samples_are_latched_independently_of_later_connect_and_survive_data_owner_drop() {
        // Labelled historical data model, not native process/ACK evidence.
        let d=WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::RunnerLogon);
        d.fail_io_kind(ErrorKind::TimedOut);
        let before=WindowsStartRunnerObservation{startup_id:39,runner_pid:4,
            runner_creation_filetime_100ns:99,state:WindowsStartRunnerState::RunningAtObservation};
        d.observe_original_runner(before);
        let context=WindowsStartupContext {startup_id:39,original_runner_identity:Some((4,99)),
            query_status:ContextFact::Observed(()),runner_before_token_query:ContextFact::Observed(before),
            runner_after_token_query:ContextFact::Observed(before),..WindowsStartupContext::default()};
        d.observe_startup_context(context);
        let mut connect=before;
        connect.state=WindowsStartRunnerState::Exited{raw_dword:0xc0000142};
        d.observe_original_runner(connect);
        let mut late=context;late.runner_before_token_query=ContextFact::Observed(connect);
        d.observe_startup_context(late);
        let retained=d.clone();d.finish_failed();drop(d);
        let snapshot=retained.snapshot().unwrap();
        assert_eq!(snapshot.current.original_runner,Some(connect));
        assert_eq!(snapshot.current.startup_context,Some(context));
        assert_eq!(snapshot.current.error,WindowsStartError::IoTimedOut);
        assert_eq!(snapshot.completion,WindowsStartCompletion::ReturnedError);
    }

    #[test]
    fn foreign_query_sample_identity_or_generation_is_rejected_without_changing_first_error() {
        let d=WindowsStartDiagnostic::default();d.mark(WindowsStartStage::RunnerLogon);
        d.fail_io_kind(ErrorKind::Interrupted);
        let original=WindowsStartRunnerObservation{startup_id:39,runner_pid:4,
            runner_creation_filetime_100ns:99,state:WindowsStartRunnerState::RunningAtObservation};
        d.observe_original_runner(original);
        let mut foreign=original;foreign.runner_creation_filetime_100ns+=1;
        let mut context=WindowsStartupContext {startup_id:39,original_runner_identity:Some((4,99)),
            query_status:ContextFact::Observed(()),runner_before_token_query:ContextFact::Observed(foreign),
            ..WindowsStartupContext::default()};
        d.observe_startup_context(context);
        assert_eq!(d.snapshot().unwrap().current.startup_context,None);
        context.runner_before_token_query=ContextFact::Observed(original);
        foreign=original;foreign.startup_id+=1;
        context.runner_after_token_query=ContextFact::Observed(foreign);
        d.observe_startup_context(context);
        assert_eq!(d.snapshot().unwrap().current.startup_context,None);
        context.runner_after_token_query=ContextFact::Observed(original);
        d.observe_startup_context(context);
        assert_eq!(d.snapshot().unwrap().current.startup_context,Some(context));
        assert_eq!(d.snapshot().unwrap().current.error,WindowsStartError::IoInterrupted);
    }

    #[test]
    fn startup_context_is_same_invocation_data_only_and_preserves_first_error_after_drop() {
        let diagnostic=WindowsStartDiagnostic::default();
        diagnostic.mark(WindowsStartStage::RunnerLogon);
        diagnostic.fail_io_kind(ErrorKind::TimedOut);
        let history=diagnostic.clone();
        let mut context=WindowsStartupContext{startup_id:38,..WindowsStartupContext::default()};
        context.original_runner_identity=Some((42,12345));
        context.query_status=ContextFact::Observed(());
        context.token.user_equals_intended_sandbox=ContextFact::NotObserved(ContextUnavailable::Win32{code:5});
        diagnostic.observe_startup_context(context);
        let mut foreign=context;foreign.startup_id=39;
        diagnostic.observe_startup_context(foreign);
        assert_eq!(diagnostic.snapshot().unwrap().current.startup_context,Some(context));
        assert_eq!(diagnostic.snapshot().unwrap().current.original_runner,None);
        // Neither logical ReturnedError nor access data becomes native DWORD.
        diagnostic.finish_failed();drop(diagnostic);
        let snapshot=history.snapshot().unwrap();
        assert_eq!(snapshot.current.error,WindowsStartError::IoTimedOut);
        assert_eq!(snapshot.current.startup_context,Some(context));
        assert_eq!(snapshot.current.original_runner,None);
        history.startup_context_unavailable(38,ContextUnavailable::Busy);
        assert_eq!(history.snapshot(),Some(snapshot));
        assert!(std::mem::size_of::<WindowsStartSnapshot>()<=2048);
    }

    #[test]
    fn startup_context_busy_and_poison_never_create_observed_grants_or_block() {
        let d=WindowsStartDiagnostic::default();
        {
            let _busy=d.0.lock().unwrap();
            d.observe_startup_context(WindowsStartupContext{startup_id:38,..WindowsStartupContext::default()});
            d.startup_context_unavailable(38,ContextUnavailable::Busy);
        }
        assert_eq!(d.snapshot().unwrap().current.startup_context,None);
        d.startup_context_unavailable(38,ContextUnavailable::Busy);
        let context=d.snapshot().unwrap().current.startup_context.unwrap();
        assert_eq!(context.query_status,ContextFact::NotObserved(ContextUnavailable::Busy));
        assert_eq!(context.station.access,ContextFact::NotObserved(ContextUnavailable::Busy));
        let other=d.clone();
        assert!(std::thread::spawn(move||{
            let _guard=other.0.lock().unwrap();panic!("labelled recorder poison");
        }).join().is_err());
        d.observe_startup_context(context);
        d.startup_context_unavailable(38,ContextUnavailable::Poisoned);
        assert_eq!(d.snapshot(),None);
    }

    #[test]
    fn startup_context_cannot_replace_a_different_original_or_cross_an_existing_refresh_attempt() {
        let d=WindowsStartDiagnostic::default();
        let original=WindowsStartRunnerObservation{startup_id:38,runner_pid:4,
            runner_creation_filetime_100ns:99,state:WindowsStartRunnerState::RunningAtObservation};
        d.observe_original_runner(original);
        let mut context=WindowsStartupContext{startup_id:38,original_runner_identity:Some((5,99)),
            ..WindowsStartupContext::default()};
        d.observe_startup_context(context);
        assert_eq!(d.snapshot().unwrap().current.startup_context,None);
        context.original_runner_identity=Some((4,100));d.observe_startup_context(context);
        assert_eq!(d.snapshot().unwrap().current.startup_context,None);
        context.original_runner_identity=Some((4,99));d.observe_startup_context(context);
        d.begin_existing_retry();
        let s=d.snapshot().unwrap();
        assert_eq!(s.first_attempt.unwrap().startup_context,Some(context));
        assert_eq!(s.current.startup_context,None);
        assert_eq!(s.current.original_runner,None);
    }

    #[test]
    fn startup_runner_history_preserves_same_identity_full_dword_first_error_and_terminal_state() {
        // Labelled historical data only; not OS observations or a native receipt.
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::PipeConnectIn);
        let original = WindowsStartRunnerObservation { startup_id: 35, runner_pid: 71,
            runner_creation_filetime_100ns: 12345, state: WindowsStartRunnerState::RunningAtObservation };
        d.observe_original_runner(original);
        d.fail_io_kind(ErrorKind::TimedOut);
        let mut exited = original;
        exited.state = WindowsStartRunnerState::Exited { raw_dword: 0x80000007 };
        d.observe_original_runner(exited);
        d.observe_original_runner(original);
        assert_eq!(d.snapshot().unwrap().current.original_runner, Some(exited));
        assert_eq!(d.snapshot().unwrap().current.error, WindowsStartError::IoTimedOut);
        let mut foreign = original;
        foreign.startup_id += 1;
        foreign.runner_pid += 1;
        d.observe_original_runner(foreign);
        assert_eq!(d.snapshot().unwrap().current.original_runner, Some(exited));
        d.finish_failed();
        assert_eq!(d.snapshot().unwrap().completion, WindowsStartCompletion::ReturnedError);
        assert!(format!("{d:?}").len() < 2048);
    }

    #[test]
    fn separate_invocations_do_not_share_failure_or_stage() {
        let a = WindowsStartDiagnostic::default();
        let b = WindowsStartDiagnostic::default();
        a.mark(WindowsStartStage::RunnerLogon);
        a.fail_os(WindowsOsError::LogonFailure);
        assert_eq!(b.snapshot(), Some(WindowsStartSnapshot::default()));
        b.mark(WindowsStartStage::ControlHello);
        assert_eq!(
            a.snapshot().unwrap().current.stage,
            WindowsStartStage::RunnerLogon
        );
    }

    #[test]
    fn lower_first_failure_survives_outer_error_mapping() {
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::PipeConnectIn);
        d.fail_io_kind(ErrorKind::TimedOut);
        d.mark(WindowsStartStage::SpawnReturned);
        d.fail(WindowsStartError::ServerInternal);
        d.finish_failed();
        let s = d.snapshot().unwrap();
        assert_eq!(s.current.stage, WindowsStartStage::PipeConnectIn);
        assert_eq!(s.current.error, WindowsStartError::IoTimedOut);
        assert_eq!(s.completion, WindowsStartCompletion::ReturnedError);
    }

    #[test]
    fn existing_retry_preserves_prior_failure_without_turning_recovery_into_failure() {
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::RunnerLogon);
        d.fail_os(WindowsOsError::AccountDisabled);
        d.begin_existing_retry();
        d.mark(WindowsStartStage::SpawnReturned);
        d.finish_returned_handle();
        let s = d.snapshot().unwrap();
        assert_eq!(s.attempt, 2);
        assert_eq!(
            s.first_attempt.unwrap().os_class,
            WindowsOsError::AccountDisabled
        );
        assert_eq!(s.current.error, WindowsStartError::NotObserved);
        assert_eq!(s.completion, WindowsStartCompletion::ReturnedBackendHandle);
    }

    #[test]
    fn completed_observation_is_frozen_and_does_not_claim_exit_or_join() {
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::SpawnReturned);
        d.finish_returned_handle();
        let before = d.snapshot();
        d.mark(WindowsStartStage::RunnerLogon);
        d.fail(WindowsStartError::Unclassified);
        d.begin_existing_retry();
        assert_eq!(d.snapshot(), before);
        let text = format!("{d:?}");
        assert!(
            !text.contains("Joined") && !text.contains("Exited") && !text.contains("Acknowledged")
        );
    }

    #[test]
    fn fixed_classes_never_retain_os_codes_or_error_messages() {
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::SpawnReadyRead);
        d.fail_runner(
            WindowsRunnerErrorStage::SpawnChild,
            WindowsOsError::from_code(0xdeadbeef),
        );
        let s = d.snapshot().unwrap();
        assert_eq!(s.current.os_class, WindowsOsError::Other);
        let text = format!("{d:?}");
        assert!(!text.contains("deadbeef") && !text.contains("3735928559"));
        for (kind, class) in [
            (
                ErrorKind::PermissionDenied,
                WindowsStartError::IoAccessDenied,
            ),
            (ErrorKind::NotFound, WindowsStartError::IoNotFound),
            (ErrorKind::InvalidInput, WindowsStartError::IoInvalidInput),
            (ErrorKind::TimedOut, WindowsStartError::IoTimedOut),
            (ErrorKind::Interrupted, WindowsStartError::IoInterrupted),
            (ErrorKind::BrokenPipe, WindowsStartError::IoBrokenPipe),
            (ErrorKind::Other, WindowsStartError::IoOther),
        ] {
            let d = WindowsStartDiagnostic::default();
            d.fail_io_kind(kind);
            assert_eq!(d.snapshot().unwrap().current.error, class);
        }
    }

    #[test]
    fn unsupported_backend_is_distinct_from_observed_failure_or_success() {
        let d = WindowsStartDiagnostic::default();
        d.unsupported();
        d.finish_returned_handle();
        let s = d.snapshot().unwrap();
        assert_eq!(s.completion, WindowsStartCompletion::UnsupportedBackend);
        assert_eq!(s.current, WindowsStartAttempt::default());
        assert_eq!(s.attempt, 0);
    }

    #[test]
    fn poisoned_recorder_is_unavailable_without_panicking_in_execution_observers() {
        let d = WindowsStartDiagnostic::default();
        let copy = d.clone();
        assert!(
            std::thread::spawn(move || {
                let _lock = copy.0.lock().unwrap();
                panic!("synthetic poison");
            })
            .join()
            .is_err()
        );
        d.mark(WindowsStartStage::RunnerLogon);
        d.fail(WindowsStartError::Unclassified);
        d.finish_failed();
        assert_eq!(d.snapshot(), None);
        assert_eq!(format!("{d:?}"), "WindowsStartDiagnosticUnavailable");
    }

    #[test]
    fn concurrent_failure_updates_leave_one_coherent_first_error() {
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::SpawnReadyRead);
        let a = d.clone();
        let b = d.clone();
        let x = std::thread::spawn(move || a.fail_os(WindowsOsError::LogonFailure));
        let y = std::thread::spawn(move || {
            b.fail_runner(
                WindowsRunnerErrorStage::SpawnChild,
                WindowsOsError::AccessDenied,
            )
        });
        x.join().unwrap();
        y.join().unwrap();
        let s = d.snapshot().unwrap();
        assert!(matches!(
            (s.current.error, s.current.runner_stage, s.current.os_class),
            (
                WindowsStartError::OsReported,
                WindowsRunnerErrorStage::NotObserved,
                WindowsOsError::LogonFailure
            ) | (
                WindowsStartError::RunnerReported,
                WindowsRunnerErrorStage::SpawnChild,
                WindowsOsError::AccessDenied
            )
        ));
        assert_eq!(s.current.stage, WindowsStartStage::SpawnReadyRead);
    }
}

#[cfg(test)]
mod transport_stage_recorder_tests2586 {
    use super::*;
    #[test]
    fn completed_snapshot_and_first_error_remain_frozen_for_new_transport_markers() {
        let d=WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::AsUserCallerAdmission);
        d.fail(WindowsStartError::IoInvalidInput);d.finish_failed();
        let before=d.snapshot();
        for stage in [WindowsStartStage::BlockingTransportEntered,
            WindowsStartStage::AsUserNativeStart,WindowsStartStage::AsUserTransportLeaseCommit] {
            d.mark(stage);d.fail(WindowsStartError::ServerInternal);
        }
        assert_eq!(d.snapshot(),before);
    }
    #[test]
    fn poisoned_recorder_is_unavailable_never_a_clean_transport_or_native_result() {
        let d=WindowsStartDiagnostic::default();
        let poison=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
            let _held=d.0.lock().unwrap();
            std::panic::resume_unwind(Box::new("synthetic recorder poison"));
        }));
        assert!(poison.is_err());
        d.mark(WindowsStartStage::BlockingTransportEntered);
        d.observe_backend_error(&anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::TimedOut)));
        d.finish_failed();
        assert_eq!(d.snapshot(),None);
        assert_eq!(format!("{d:?}"),"WindowsStartDiagnosticUnavailable");
    }
}

#[cfg(test)]
mod asuser_first_failure_observation_tests2659 {
    use super::*;
    #[test]
    fn internal_permission_denial_has_no_invented_win32_or_native_identity() {
        let d=WindowsStartDiagnostic::default();d.mark(WindowsStartStage::AsUserAdmissionBind);
        d.observe_asuser_native_failure(WindowsAsUserNativeStage::Admission,WindowsAsUserNativeSite::NotObserved,ErrorKind::PermissionDenied,None,None);
        let s=d.snapshot().unwrap();
        assert_eq!(s.current.asuser_native_failure,Some(WindowsAsUserNativeFailure {
            owner_stage:WindowsAsUserNativeStage::Admission,site:WindowsAsUserNativeSite::NotObserved,error:WindowsStartError::IoAccessDenied,
            win32:None,ntstatus:None,target_policy_rejection:None,
        }));
        assert_eq!(s.current.error,WindowsStartError::NotObserved);
        assert_eq!(s.current.original_runner,None);assert_eq!(s.current.startup_context,None);
        assert_eq!(s.current.os_class,WindowsOsError::NotObserved);
    }
    #[test]
    fn raw_first_failure_is_finite_sticky_and_freezes_with_original_completion() {
        let d=WindowsStartDiagnostic::default();d.mark(WindowsStartStage::AsUserNativeStart);
        // Synthetic saved values, not an API invocation or OS qualification.
        d.observe_asuser_native_failure(WindowsAsUserNativeStage::Create,WindowsAsUserNativeSite::NotObserved,ErrorKind::PermissionDenied,Some(1314),None);
        let first=d.snapshot().unwrap().current.asuser_native_failure;
        d.observe_asuser_native_failure(WindowsAsUserNativeStage::Retirement,WindowsAsUserNativeSite::NotObserved,ErrorKind::Other,Some(5),Some(-1));
        assert_eq!(d.snapshot().unwrap().current.asuser_native_failure,first);
        d.fail_io_kind(ErrorKind::PermissionDenied);d.finish_failed();let before=d.snapshot();
        d.observe_asuser_native_failure(WindowsAsUserNativeStage::Broker,WindowsAsUserNativeSite::NotObserved,ErrorKind::Other,None,Some(-2));
        d.observe_asuser_progress(WindowsStartStage::AsUserPipeNames);
        assert_eq!(d.snapshot(),before);
        assert_eq!(first.unwrap().win32,Some(1314));assert_eq!(first.unwrap().ntstatus,None);
    }
    #[test]
    fn busy_poison_and_prior_error_never_turn_missing_observation_into_zero_codes() {
        let d=WindowsStartDiagnostic::default();
        {let held=d.0.lock().unwrap();
            d.observe_asuser_native_failure(WindowsAsUserNativeStage::Broker,WindowsAsUserNativeSite::NotObserved,ErrorKind::Other,Some(5),None);
            d.observe_asuser_progress(WindowsStartStage::AsUserPipeNames);
            assert_eq!(held.current.asuser_native_failure,None);
            assert_eq!(held.current.stage,WindowsStartStage::NotObserved);
        }
        d.fail_io_kind(ErrorKind::TimedOut);
        d.observe_asuser_native_failure(WindowsAsUserNativeStage::Broker,WindowsAsUserNativeSite::NotObserved,ErrorKind::Other,Some(5),None);
        assert_eq!(d.snapshot().unwrap().current.asuser_native_failure,None);
        let poison=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
            let _held=d.0.lock().unwrap();std::panic::resume_unwind(Box::new("synthetic diagnostic poison"));
        }));
        assert!(poison.is_err());
        d.observe_asuser_native_failure(WindowsAsUserNativeStage::Unwind,WindowsAsUserNativeSite::NotObserved,ErrorKind::Other,None,None);
        d.observe_asuser_progress(WindowsStartStage::AsUserNativeStart);
        assert_eq!(d.snapshot(),None);
    }
    #[test]
    fn existing_retry_history_keeps_first_projection_separate_and_does_not_invoke_retry() {
        let d=WindowsStartDiagnostic::default();d.mark(WindowsStartStage::AsUserNativeStart);
        d.observe_asuser_native_failure(WindowsAsUserNativeStage::Token,WindowsAsUserNativeSite::NotObserved,ErrorKind::Other,None,Some(-1073741790));
        let first=d.snapshot().unwrap().current;d.begin_existing_retry();
        let s=d.snapshot().unwrap();assert_eq!(s.first_attempt,Some(first));
        assert_eq!(s.current.asuser_native_failure,None);assert_eq!(s.current.original_runner,None);
        d.observe_asuser_progress(WindowsStartStage::AsUserPipeNames);
        assert_eq!(d.snapshot().unwrap().current.stage,WindowsStartStage::AsUserPipeNames);
        d.observe_asuser_progress(WindowsStartStage::SpawnReturned);
        assert_eq!(d.snapshot().unwrap().current.stage,WindowsStartStage::AsUserPipeNames);
    }
}

#[cfg(test)]
mod target_policy_recorder_tests2740 {
    use super::*;
    fn record(d:&WindowsStartDiagnostic,site:WindowsAsUserNativeSite,reason:WindowsTargetPolicyRejection) {
        d.observe_asuser_native_failure_with_target_policy(WindowsAsUserNativeStage::Token,site,
            ErrorKind::PermissionDenied,None,None,Some(reason));
    }
    #[test] fn bounded_reason_requires_the_same_guard_and_remains_first_and_frozen() {
        let d=WindowsStartDiagnostic::default();
        record(&d,WindowsAsUserNativeSite::TargetFacts,WindowsTargetPolicyRejection::Restricted);
        assert_eq!(d.snapshot().unwrap().current.asuser_native_failure.unwrap().target_policy_rejection,None);
        let d=WindowsStartDiagnostic::default();
        record(&d,WindowsAsUserNativeSite::TargetPolicyGuard,WindowsTargetPolicyRejection::Restricted);
        let first=d.snapshot().unwrap().current.asuser_native_failure;
        assert_eq!(first.unwrap().target_policy_rejection,Some(WindowsTargetPolicyRejection::Restricted));
        record(&d,WindowsAsUserNativeSite::TargetPolicyGuard,WindowsTargetPolicyRejection::AdministratorMembership);
        assert_eq!(d.snapshot().unwrap().current.asuser_native_failure,first);
        d.fail_io_kind(ErrorKind::PermissionDenied);d.finish_failed();let frozen=d.snapshot();
        record(&d,WindowsAsUserNativeSite::TargetPolicyGuard,WindowsTargetPolicyRejection::Virtualization);
        assert_eq!(d.snapshot(),frozen);
        assert!(std::mem::size_of::<WindowsTargetPolicyRejection>()<=1);
    }
    #[test] fn busy_poison_and_legacy_observation_never_fabricate_clean_or_a_reason() {
        let d=WindowsStartDiagnostic::default();
        let held=d.0.lock().unwrap();
        record(&d,WindowsAsUserNativeSite::TargetPolicyGuard,WindowsTargetPolicyRejection::Restricted);
        drop(held);assert_eq!(d.snapshot().unwrap().current.asuser_native_failure,None);
        d.observe_asuser_native_failure(WindowsAsUserNativeStage::Token,
            WindowsAsUserNativeSite::TargetPolicyGuard,ErrorKind::PermissionDenied,None,None);
        assert_eq!(d.snapshot().unwrap().current.asuser_native_failure.unwrap().target_policy_rejection,None);
        let d=WindowsStartDiagnostic::default();
        let poison=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
            let _held=d.0.lock().unwrap();std::panic::resume_unwind(Box::new("synthetic recorder poison"));
        }));assert!(poison.is_err());
        record(&d,WindowsAsUserNativeSite::TargetPolicyGuard,WindowsTargetPolicyRejection::Restricted);
        assert_eq!(d.snapshot(),None);
    }
}
