//! Bounded, process-local startup history. No transport, native capability,
//! clocks, I/O, callbacks, waits, authority tokens or owner references.
//! The extra Arc is allocated in ImportCore::new before import. Binding uses
//! that import's existing generation; cloning/dropping this data cannot retain
//! its original process/Job/lease. try_lock never waits or repairs business
//! first-error/Unknown. Missing/contended/poisoned data is never a clean zero.
//! Events describe function entry/return only. In particular stdout flush
//! return is NOT reader delivery, EOF, native status, admission or an ACK.
//! Existing catches alone record unwind; abort/OOM and uncaught construction
//! panics can leave Entered. Nothing here installs a panic hook or Drop effect.
use std::io;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU8, Ordering}};

pub const STARTUP_HISTORY_PER_TRACE: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontendStage { Arguments, PhaseCheck, ConfigBuild, ValidateBeforeSetup,
    FirmwareUuidCheck, ConfigPreflight, ProvisioningConfigBuild, LifecyclePreflight }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdkStage { ImportSuffix, OriginLive, EarlyInstall, EarlyReserve,
    EarlyObserve, EarlyRequire, EarlyAdmitted, CallerArtifact, RolePins, GuardReturnedError }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupStage { NotObserved, Sdk(SdkStage), Frontend(FrontendStage),
    FirstPromptWrite, FirstPromptFlush }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupOutcome { NotObserved, Entered, Returned, ReturnedVoid,
    IoError, OpaqueError, Unwind }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupEvent {
    pub sequence: u32, pub stage: StartupStage, pub outcome: StartupOutcome,
    pub io_kind: Option<io::ErrorKind>, pub raw_os_error: Option<i32>,
}
const EMPTY: StartupEvent = StartupEvent { sequence: 0, stage: StartupStage::NotObserved,
    outcome: StartupOutcome::NotObserved, io_kind: None, raw_os_error: None };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupTrace {
    /// Oldest retained event first; total_events > retained means older history
    /// was dropped. This fixed ring never claims an exhaustive native trace.
    pub events: [StartupEvent; STARTUP_HISTORY_PER_TRACE],
    pub retained: u8, pub total_events: u32, pub active: StartupEvent,
}
impl StartupTrace {
    const fn new() -> Self { Self { events: [EMPTY; STARTUP_HISTORY_PER_TRACE],
        retained: 0, total_events: 0, active: EMPTY } }
    fn push(&mut self, event: StartupEvent) -> bool {
        let Some(total) = self.total_events.checked_add(1) else { return false };
        if usize::from(self.retained) == STARTUP_HISTORY_PER_TRACE {
            self.events.copy_within(1..STARTUP_HISTORY_PER_TRACE, 0);
            self.events[STARTUP_HISTORY_PER_TRACE - 1] = event;
        } else {
            self.events[usize::from(self.retained)] = event;
            self.retained += 1;
        }
        self.total_events = total;
        true
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupSeal { Open, FirstError, FirstPromptFlushReturned }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupSnapshot {
    pub generation: u64, pub sdk: StartupTrace, pub frontend: StartupTrace,
    pub first_error: Option<StartupEvent>, pub seal: StartupSeal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupUnavailable { Unsupported, Unbound, Busy, Poisoned, Incomplete, WrongGeneration }

struct State { snapshot: StartupSnapshot, sequence: u32, prompt_claimed: bool }
impl State {
    fn new() -> Self { Self { snapshot: StartupSnapshot { generation: 0,
        sdk: StartupTrace::new(), frontend: StartupTrace::new(),
        first_error: None, seal: StartupSeal::Open }, sequence: 0, prompt_claimed: false } }
    fn trace(&mut self, sdk: bool) -> &mut StartupTrace {
        if sdk { &mut self.snapshot.sdk } else { &mut self.snapshot.frontend }
    }
}
struct Inner { state: Mutex<State>, gap: AtomicBool, terminal: AtomicU8 }
impl Inner {
    fn new() -> Self { Self { state: Mutex::new(State::new()),
        gap: AtomicBool::new(false), terminal: AtomicU8::new(0) } }
    fn incomplete(&self) {
        self.gap.store(true, Ordering::Release);
        let _ = self.terminal.compare_exchange(0, 3, Ordering::AcqRel, Ordering::Acquire);
    }
}
/// Allocation and binding are SDK-private; the Arc owns data ONLY.
#[derive(Clone)]
pub(crate) struct StartupCarrier(Arc<Inner>);
impl StartupCarrier {
    pub(crate) fn new() -> Self { Self(Arc::new(Inner::new())) }
    pub(crate) fn bind(&self, generation: u64) {
        let Ok(mut state) = self.0.state.try_lock() else { self.0.incomplete(); return };
        if generation == 0 || state.snapshot.generation != 0 {
            self.0.incomplete(); return;
        }
        state.snapshot.generation = generation;
    }
    pub(crate) fn observation(&self, generation: u64) -> StartupObservation {
        StartupObservation { inner: Some(self.0.clone()), generation }
    }
    pub(crate) fn unavailable(&self) {
        if self.0.terminal.load(Ordering::Acquire) == 0 { self.0.incomplete(); }
    }
    pub(crate) fn sdk_io_current(&self, error: &io::Error) {
        self.current_failure(true, StartupOutcome::IoError, Some(error.kind()), error.raw_os_error());
    }
    pub(crate) fn sdk_unwind_current(&self) {
        self.current_failure(true, StartupOutcome::Unwind, None, None);
    }
    fn current_failure(&self, sdk: bool, outcome: StartupOutcome,
        kind: Option<io::ErrorKind>, raw: Option<i32>) {
        record_current_failure(&self.0, None, sdk, outcome, kind, raw);
    }
}
/// Sealed data producer for the same imported generation. It cannot operate
/// on any original owner; readers may retain history after owner retirement.
#[derive(Clone)]
pub struct StartupObservation { inner: Option<Arc<Inner>>, generation: u64 }
#[derive(Clone)]
pub struct StartupHistory { inner: Option<Arc<Inner>>, generation: u64 }
impl Default for StartupObservation { fn default() -> Self { Self::unsupported() } }
impl StartupObservation {
    pub fn unsupported() -> Self { Self { inner: None, generation: 0 } }
    pub fn history(&self) -> StartupHistory {
        StartupHistory { inner: self.inner.clone(), generation: self.generation }
    }
    pub fn enter(&self, stage: FrontendStage) -> StartupStep {
        self.begin(StartupStage::Frontend(stage), false)
    }
    pub(crate) fn sdk_enter(&self, stage: SdkStage) -> StartupStep {
        self.begin(StartupStage::Sdk(stage), true)
    }
    pub fn frontend_opaque_error(&self) {
        if let Some(inner) = &self.inner {
            record_current_failure(inner, Some(self.generation), false, StartupOutcome::OpaqueError, None, None);
        }
    }
    pub fn frontend_unwind(&self) {
        if let Some(inner) = &self.inner {
            record_current_failure(inner, Some(self.generation), false, StartupOutcome::Unwind, None, None);
        }
    }
    fn begin(&self, stage: StartupStage, sdk: bool) -> StartupStep {
        let mut result = StartupStep { inner: None, generation: self.generation,
            sequence: 0, stage, sdk };
        let Some(inner) = &self.inner else { return result };
        if inner.terminal.load(Ordering::Acquire) != 0 { return result; }
        let Ok(mut state) = inner.state.try_lock() else { inner.incomplete(); return result };
        if inner.terminal.load(Ordering::Acquire) != 0 { return result; }
        if self.generation == 0 || state.snapshot.generation != self.generation
            || state.trace(sdk).active != EMPTY {
            inner.incomplete(); return result;
        }
        let Some(sequence) = state.sequence.checked_add(1) else { inner.incomplete(); return result };
        state.sequence = sequence;
        let event = StartupEvent { sequence, stage, outcome: StartupOutcome::Entered,
            io_kind: None, raw_os_error: None };
        if !state.trace(sdk).push(event) { inner.incomplete(); return result; }
        state.trace(sdk).active = event;
        result.inner = Some(inner.clone()); result.sequence = sequence;
        result
    }
    /// Claimed once for the initial fixed confirmation. Later confirmations
    /// use the original unobserved function. Failed claim does not authorize,
    /// suppress, duplicate or retry output.
    pub fn first_prompt(&self) -> StartupPrompt {
        let mut prompt = StartupPrompt { observation: Self::unsupported(), phase: 0 };
        let Some(inner) = &self.inner else { return prompt };
        if inner.terminal.load(Ordering::Acquire) != 0 { return prompt; }
        let Ok(mut state) = inner.state.try_lock() else { inner.incomplete(); return prompt };
        if inner.terminal.load(Ordering::Acquire) != 0 { return prompt; }
        if self.generation == 0 || state.snapshot.generation != self.generation
            || state.prompt_claimed || state.snapshot.frontend.active != EMPTY
            || state.snapshot.sdk.active != EMPTY {
            inner.incomplete(); return prompt;
        }
        state.prompt_claimed = true; prompt.observation = self.clone(); prompt
    }
}
impl StartupHistory {
    /// A read has no callbacks, native queries, business mutation or retry.
    pub fn snapshot(&self) -> Result<StartupSnapshot, StartupUnavailable> {
        let inner = self.inner.as_ref().ok_or(StartupUnavailable::Unsupported)?;
        if inner.gap.load(Ordering::Acquire) { return Err(StartupUnavailable::Incomplete); }
        let state = match inner.state.try_lock() {
            Ok(state) => state,
            Err(std::sync::TryLockError::WouldBlock) => return Err(StartupUnavailable::Busy),
            Err(std::sync::TryLockError::Poisoned(_)) => return Err(StartupUnavailable::Poisoned),
        };
        if inner.gap.load(Ordering::Acquire) { return Err(StartupUnavailable::Incomplete); }
        if state.snapshot.generation == 0 { return Err(StartupUnavailable::Unbound); }
        if state.snapshot.generation != self.generation { return Err(StartupUnavailable::WrongGeneration); }
        Ok(state.snapshot)
    }
}
/// Private identity/sequence fields prevent constructing a completion from
/// scalar observations. This is a history ticket, never a native proof.
pub struct StartupStep { inner: Option<Arc<Inner>>, generation: u64,
    sequence: u32, stage: StartupStage, sdk: bool }
impl StartupStep {
    pub fn returned(self) { self.finish(StartupOutcome::Returned, None, None, false); }
    pub(crate) fn returned_void(self) { self.finish(StartupOutcome::ReturnedVoid, None, None, false); }
    pub fn io_error(self, error: &io::Error) {
        self.finish(StartupOutcome::IoError, Some(error.kind()), error.raw_os_error(), false);
    }
    fn finish(self, outcome: StartupOutcome, kind: Option<io::ErrorKind>, raw: Option<i32>, flush: bool) {
        let Some(inner) = &self.inner else { return };
        if inner.terminal.load(Ordering::Acquire) != 0 { return; }
        let Ok(mut state) = inner.state.try_lock() else { inner.incomplete(); return };
        if inner.terminal.load(Ordering::Acquire) != 0 { return; }
        let active = state.trace(self.sdk).active;
        if state.snapshot.generation != self.generation || active.sequence != self.sequence
            || active.stage != self.stage || active.outcome != StartupOutcome::Entered
            || (flush && self.stage != StartupStage::FirstPromptFlush) {
            inner.incomplete(); return;
        }
        let event = StartupEvent { outcome, io_kind: kind, raw_os_error: raw, ..active };
        if !state.trace(self.sdk).push(event) { inner.incomplete(); return; }
        state.trace(self.sdk).active = EMPTY;
        if matches!(outcome, StartupOutcome::IoError | StartupOutcome::OpaqueError | StartupOutcome::Unwind) {
            state.snapshot.first_error = Some(event); state.snapshot.seal = StartupSeal::FirstError;
            inner.terminal.store(1, Ordering::Release);
        } else if flush {
            state.snapshot.seal = StartupSeal::FirstPromptFlushReturned;
            inner.terminal.store(2, Ordering::Release);
        }
    }
}
fn record_current_failure(inner: &Arc<Inner>, generation: Option<u64>, sdk: bool, outcome: StartupOutcome,
    kind: Option<io::ErrorKind>, raw: Option<i32>) {
    if inner.terminal.load(Ordering::Acquire) != 0 { return; }
    let Ok(mut state) = inner.state.try_lock() else { inner.incomplete(); return };
    if inner.terminal.load(Ordering::Acquire) != 0 { return; }
    if state.snapshot.generation == 0 || generation.is_some_and(|value|value != state.snapshot.generation) {
        inner.incomplete(); return;
    }
    let mut active = state.trace(sdk).active;
    if active == EMPTY {
        // SDK's existing catch/returned-error branch is itself a concrete
        // codepoint. An unlocated frontend error cannot invent a past stage.
        if !sdk { inner.incomplete(); return; }
        let Some(sequence) = state.sequence.checked_add(1) else { inner.incomplete(); return };
        state.sequence = sequence;
        active = StartupEvent { sequence, stage: StartupStage::Sdk(SdkStage::GuardReturnedError),
            outcome: StartupOutcome::Entered, io_kind: None, raw_os_error: None };
        if !state.trace(true).push(active) { inner.incomplete(); return; }
    }
    let event = StartupEvent { outcome, io_kind: kind, raw_os_error: raw, ..active };
    if !state.trace(sdk).push(event) { inner.incomplete(); return; }
    state.trace(sdk).active = EMPTY;
    state.snapshot.first_error = Some(event); state.snapshot.seal = StartupSeal::FirstError;
    inner.terminal.store(1, Ordering::Release);
}
pub struct StartupPrompt { observation: StartupObservation, phase: u8 }
impl StartupPrompt {
    pub fn write_entered(&mut self) -> StartupStep {
        if self.phase != 0 {
            if let Some(inner) = &self.observation.inner { inner.incomplete(); }
            return StartupObservation::unsupported().begin(StartupStage::FirstPromptWrite, false);
        }
        self.phase = 1; self.observation.begin(StartupStage::FirstPromptWrite, false)
    }
    pub fn flush_entered(&mut self) -> StartupStep {
        if self.phase != 1 {
            if let Some(inner) = &self.observation.inner { inner.incomplete(); }
            return StartupObservation::unsupported().begin(StartupStage::FirstPromptFlush, false);
        }
        self.phase = 2; self.observation.begin(StartupStage::FirstPromptFlush, false)
    }
    pub fn flush_result(&mut self, step: StartupStep, result: &io::Result<()>) {
        // A ticket from another data instance/generation cannot seal this one.
        let same = match (&self.observation.inner, &step.inner) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b) && self.observation.generation == step.generation,
            (None, None) => true,
            _ => false,
        };
        if self.phase != 2 || !same || step.stage != StartupStage::FirstPromptFlush || step.sdk {
            if let Some(inner) = &self.observation.inner { inner.incomplete(); }
            return;
        }
        self.phase = 3;
        match result {
            Ok(()) => step.finish(StartupOutcome::Returned, None, None, true),
            Err(error) => step.io_error(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bound(generation: u64) -> (StartupCarrier, StartupObservation) {
        let carrier = StartupCarrier::new(); carrier.bind(generation);
        let observation = carrier.observation(generation); (carrier, observation)
    }
    #[test] fn unsupported_and_unbound_are_not_empty_success() {
        assert_eq!(StartupObservation::unsupported().history().snapshot(), Err(StartupUnavailable::Unsupported));
        let carrier = StartupCarrier::new();
        assert_eq!(carrier.observation(7).history().snapshot(), Err(StartupUnavailable::Unbound));
    }
    #[test] fn entered_does_not_imply_return_or_admission() {
        let (_, observation) = bound(7); let step = observation.sdk_enter(SdkStage::EarlyObserve);
        assert_eq!(observation.history().snapshot().unwrap().sdk.active.outcome, StartupOutcome::Entered);
        step.returned_void(); let snapshot = observation.history().snapshot().unwrap();
        assert_eq!(snapshot.sdk.events[1].outcome, StartupOutcome::ReturnedVoid);
        assert_eq!(snapshot.seal, StartupSeal::Open); assert_eq!(snapshot.first_error, None);
    }
    #[test] fn typed_io_preserves_raw_and_first_failure() {
        let (carrier, observation) = bound(7);
        observation.sdk_enter(SdkStage::EarlyRequire).io_error(&io::Error::from_raw_os_error(123));
        carrier.sdk_io_current(&io::Error::from_raw_os_error(5));
        let snapshot = observation.history().snapshot().unwrap();
        assert_eq!(snapshot.first_error.unwrap().raw_os_error, Some(123));
        assert_eq!(snapshot.seal, StartupSeal::FirstError);
    }
    #[test] fn opaque_error_never_extracts_or_invents_raw_os_code() {
        let (_, observation) = bound(7); observation.enter(FrontendStage::LifecyclePreflight);
        observation.frontend_opaque_error(); let error = observation.history().snapshot().unwrap().first_error.unwrap();
        assert_eq!(error.outcome, StartupOutcome::OpaqueError);
        assert_eq!((error.io_kind, error.raw_os_error), (None, None));
    }
    #[test] fn existing_unwind_freezes_current_stage_without_native_claims() {
        let (_, observation) = bound(7); observation.enter(FrontendStage::ConfigBuild);
        observation.frontend_unwind(); let snapshot = observation.history().snapshot().unwrap();
        assert_eq!(snapshot.first_error.unwrap().outcome, StartupOutcome::Unwind);
        assert_eq!(snapshot.first_error.unwrap().stage, StartupStage::Frontend(FrontendStage::ConfigBuild));
    }
    #[test] fn wrong_generation_and_rebinding_degrade_without_repair() {
        let (carrier, _) = bound(7);
        assert_eq!(carrier.observation(8).history().snapshot(), Err(StartupUnavailable::WrongGeneration));
        carrier.observation(8).enter(FrontendStage::Arguments);
        assert_eq!(carrier.observation(7).history().snapshot(), Err(StartupUnavailable::Incomplete));
        let (carrier, observation) = bound(7);
        observation.enter(FrontendStage::Arguments);
        carrier.observation(8).frontend_opaque_error();
        assert_eq!(observation.history().snapshot(), Err(StartupUnavailable::Incomplete));
        let (carrier, observation) = bound(7); carrier.bind(7);
        assert_eq!(observation.history().snapshot(), Err(StartupUnavailable::Incomplete));
    }
    #[test] fn busy_read_is_typed_and_busy_write_cannot_report_clean() {
        let (carrier, observation) = bound(7); let _held = carrier.0.state.lock().unwrap();
        assert_eq!(observation.history().snapshot(), Err(StartupUnavailable::Busy));
        observation.enter(FrontendStage::Arguments);
        assert!(carrier.0.gap.load(Ordering::Acquire));
    }
    #[test] fn poisoned_data_is_never_unwrapped_or_recovered() {
        let (carrier, observation) = bound(7);
        let _ = std::panic::catch_unwind(|| { let _held = carrier.0.state.lock().unwrap(); panic!("pure poison"); });
        assert_eq!(observation.history().snapshot(), Err(StartupUnavailable::Poisoned));
        observation.enter(FrontendStage::Arguments);
        assert_eq!(observation.history().snapshot(), Err(StartupUnavailable::Incomplete));
    }
    #[test] fn same_generation_different_instance_cannot_finish_prompt() {
        let (_, a) = bound(7); let (_, b) = bound(7);
        let mut pa = a.first_prompt(); pa.write_entered().returned(); let sa = pa.flush_entered();
        let mut pb = b.first_prompt(); pb.write_entered().returned(); let sb = pb.flush_entered();
        pa.flush_result(sb, &Ok(()));
        assert_eq!(a.history().snapshot(), Err(StartupUnavailable::Incomplete));
        assert_eq!(b.history().snapshot().unwrap().seal, StartupSeal::Open); drop(sa);
    }
    #[test] fn flush_return_is_only_a_local_function_event_and_seals_once() {
        let (_, observation) = bound(7); let mut prompt = observation.first_prompt();
        prompt.write_entered().returned(); let flush = prompt.flush_entered();
        assert_eq!(observation.history().snapshot().unwrap().seal, StartupSeal::Open);
        prompt.flush_result(flush, &Ok(())); let snapshot = observation.history().snapshot().unwrap();
        assert_eq!(snapshot.seal, StartupSeal::FirstPromptFlushReturned);
        assert_eq!(snapshot.first_error, None);
        observation.frontend_opaque_error(); observation.frontend_unwind();
        assert_eq!(observation.history().snapshot().unwrap(), snapshot);
    }
    #[test] fn real_flush_io_failure_is_not_delivery_or_clean_flush() {
        let (_, observation) = bound(7); let mut prompt = observation.first_prompt();
        prompt.write_entered().returned(); let flush = prompt.flush_entered();
        prompt.flush_result(flush, &Err(io::Error::from_raw_os_error(5)));
        let snapshot = observation.history().snapshot().unwrap();
        assert_eq!(snapshot.seal, StartupSeal::FirstError);
        assert_eq!(snapshot.first_error.unwrap().raw_os_error, Some(5));
        assert_eq!(snapshot.first_error.unwrap().stage, StartupStage::FirstPromptFlush);
    }
    #[test] fn wrong_order_duplicate_prompt_and_dropped_ticket_do_not_invent_return() {
        let (_, observation) = bound(7); let mut prompt = observation.first_prompt();
        prompt.flush_entered();
        assert_eq!(observation.history().snapshot(), Err(StartupUnavailable::Incomplete));
        let (_, observation) = bound(7); observation.first_prompt(); observation.first_prompt();
        assert_eq!(observation.history().snapshot(), Err(StartupUnavailable::Incomplete));
        let (_, observation) = bound(7); drop(observation.enter(FrontendStage::Arguments));
        assert_eq!(observation.history().snapshot().unwrap().frontend.active.outcome, StartupOutcome::Entered);
    }
    #[test] fn history_outlives_only_data_and_ring_stays_bounded() {
        let (carrier, observation) = bound(7);
        for _ in 0..32 { observation.enter(FrontendStage::Arguments).returned(); }
        let history = observation.history(); drop(observation); drop(carrier);
        let snapshot = history.snapshot().unwrap();
        assert_eq!(snapshot.frontend.retained, STARTUP_HISTORY_PER_TRACE as u8);
        assert_eq!(snapshot.frontend.total_events, 64); assert_eq!(snapshot.seal, StartupSeal::Open);
        assert!(std::mem::size_of::<StartupSnapshot>() <= 768);
    }
    #[test] fn sdk_failure_preserves_concurrent_frontend_entry_without_faking_return() {
        let (carrier, observation) = bound(7); observation.enter(FrontendStage::ValidateBeforeSetup);
        observation.sdk_enter(SdkStage::EarlyReserve); carrier.sdk_io_current(&io::Error::from_raw_os_error(123));
        let snapshot = observation.history().snapshot().unwrap();
        assert_eq!(snapshot.frontend.active.outcome, StartupOutcome::Entered);
        assert_eq!(snapshot.first_error.unwrap().stage, StartupStage::Sdk(SdkStage::EarlyReserve));
    }

    // Synthetic data-only guard-contention scenario. No OriginGuard, native
    // fixture, OS operation, query, wait or control owner is created here.
    #[test] fn borrowed_guard_cannot_finish_inflight_sdk_or_rewrite_sealed_first_error() {
        let (carrier, observation) = bound(7);
        let _inflight = observation.sdk_enter(SdkStage::EarlyObserve);
        let before = observation.history().snapshot().unwrap();
        assert_eq!(before.sdk.active.stage, StartupStage::Sdk(SdkStage::EarlyObserve));
        assert_eq!(before.sdk.active.outcome, StartupOutcome::Entered);
        carrier.unavailable();
        assert_eq!(observation.history().snapshot(), Err(StartupUnavailable::Incomplete));
        let state = carrier.0.state.lock().unwrap();
        assert_eq!(state.snapshot, before);
        assert_eq!(state.snapshot.sdk.active.outcome, StartupOutcome::Entered);
        assert_eq!(state.snapshot.first_error, None);
        assert!(state.snapshot.sdk.events[..usize::from(state.snapshot.sdk.retained)]
            .iter().all(|event| event.outcome == StartupOutcome::Entered));
        drop(state);

        let (carrier, observation) = bound(9);
        observation.sdk_enter(SdkStage::EarlyRequire)
            .io_error(&io::Error::from_raw_os_error(123));
        let sealed = observation.history().snapshot().unwrap();
        assert_eq!(sealed.seal, StartupSeal::FirstError);
        assert_eq!(sealed.first_error.unwrap().outcome, StartupOutcome::IoError);
        assert_eq!(sealed.first_error.unwrap().raw_os_error, Some(123));
        carrier.unavailable();
        assert_eq!(observation.history().snapshot().unwrap(), sealed);
    }

    // This invokes only the data method used by the EXISTING SDK catch branch;
    // it neither synthesizes an io::Error nor invokes a native catch fixture.
    #[test] fn sdk_existing_catch_records_unwind_kind_without_fabricating_io_failure() {
        let (carrier, observation) = bound(11);
        let _inflight = observation.sdk_enter(SdkStage::EarlyObserve);
        carrier.sdk_unwind_current();
        let sealed = observation.history().snapshot().unwrap();
        let first = sealed.first_error.unwrap();
        assert_eq!(sealed.seal, StartupSeal::FirstError);
        assert_eq!(first.stage, StartupStage::Sdk(SdkStage::EarlyObserve));
        assert_eq!(first.outcome, StartupOutcome::Unwind);
        assert_eq!((first.io_kind, first.raw_os_error), (None, None));
        assert_eq!(sealed.sdk.active, EMPTY);
        assert_eq!(sealed.sdk.total_events, 2);
        assert_eq!(sealed.sdk.events[0].outcome, StartupOutcome::Entered);
        assert_eq!(sealed.sdk.events[1].outcome, StartupOutcome::Unwind);
        carrier.unavailable();
        assert_eq!(observation.history().snapshot().unwrap(), sealed);
    }
}
