//! Durable, one-shot fixed-input execution coordination. No automatic OS backend.
use crate::{
    Action, Error, ExecutionFacts, Intent, Outcome, Request, Result, ToolInfo, ToolPhase,
    authority::{Admission, Right, SessionExecHost, core_error, random},
    hash, identity,
};
use morrow_core::{
    agent_ledger::Record,
    dispatch::{Connection, HostRuntime},
};
use prost::Message;

const MAX_RECORD_BYTES: usize = crate::MAX_FRAME_BYTES + 4096;
const TERMINAL_RESERVE: usize = 1024;

/// Material for trusted host review. It cannot be imported as live approval.
#[derive(Clone)]
pub struct ToolReview {
    pub proposal_sha256: [u8; 32],
    pub intent_sha256: [u8; 32],
    pub intent: Intent,
    pub session_id: String,
    pub session_epoch: u64,
    pub expires_ms: u64,
}

// The durable record is historical data. Its random issuer and opaque original
// live admission must still match this host before any permit can be consumed.
#[derive(Clone, PartialEq, Message)]
struct State {
    #[prost(uint32, tag = "1")]
    version: u32,
    #[prost(string, tag = "2")]
    session: String,
    #[prost(uint64, tag = "3")]
    session_epoch: u64,
    #[prost(bytes = "vec", tag = "4")]
    request_sha256: Vec<u8>,
    #[prost(bytes = "vec", tag = "5")]
    proposer: Vec<u8>,
    #[prost(bytes = "vec", tag = "6")]
    issuer: Vec<u8>,
    #[prost(uint64, tag = "7")]
    expires: u64,
    #[prost(message, optional, tag = "8")]
    intent: Option<FixedIntent>,
    #[prost(uint32, tag = "9")]
    phase: u32,
    #[prost(bytes = "vec", tag = "10")]
    executor: Vec<u8>,
    #[prost(bytes = "vec", tag = "11")]
    permit: Vec<u8>,
    #[prost(bytes = "vec", tag = "12")]
    claim: Vec<u8>,
    #[prost(bool, tag = "13")]
    invocation_started: bool,
    #[prost(bool, tag = "14")]
    revoked: bool,
    #[prost(message, optional, tag = "15")]
    facts: Option<Facts>,
    #[prost(bytes = "vec", tag = "16")]
    report_sha256: Vec<u8>,
    #[prost(uint32, tag = "17")]
    capacity: u32,
    #[prost(bytes = "vec", tag = "18")]
    padding: Vec<u8>,
    #[prost(message, optional, tag = "19")]
    observed_facts: Option<Facts>,
}
#[derive(Clone, PartialEq, Message)]
struct FixedIntent {
    #[prost(string, tag = "1")]
    operation: String,
    #[prost(string, tag = "2")]
    program: String,
    #[prost(string, repeated, tag = "3")]
    argv: Vec<String>,
    #[prost(string, tag = "4")]
    cwd: String,
    #[prost(message, repeated, tag = "5")]
    env: Vec<Variable>,
    #[prost(bytes = "vec", tag = "6")]
    input: Vec<u8>,
    #[prost(string, tag = "7")]
    domain: String,
    #[prost(uint64, tag = "8")]
    runtime: u64,
    #[prost(bytes = "vec", tag = "9")]
    artifact_sha256: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct Variable {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(string, tag = "2")]
    value: String,
}
#[derive(Clone, PartialEq, Message)]
struct Facts {
    #[prost(sint32, optional, tag = "1")]
    exit_code: Option<i32>,
    #[prost(bool, tag = "2")]
    output_closed: bool,
    #[prost(bytes = "vec", tag = "3")]
    stdout_sha256: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    stderr_sha256: Vec<u8>,
    #[prost(uint64, tag = "5")]
    stdout_bytes: u64,
    #[prost(uint64, tag = "6")]
    stderr_bytes: u64,
}
fn domain() -> [u8; 32] {
    hash(b"agent-session-exec-v1/tool")
}
fn fixed(bytes: &[u8]) -> Result<[u8; 32]> {
    bytes.try_into().map_err(|_| Error::Storage)
}
fn token(bytes: &[u8]) -> Result<[u8; 32]> {
    let token = fixed(bytes)?;
    if token == [0; 32] {
        return Err(Error::Storage);
    }
    Ok(token)
}
impl From<&Intent> for FixedIntent {
    fn from(v: &Intent) -> Self {
        Self {
            operation: v.operation_id.clone(),
            program: v.program.clone(),
            argv: v.argv.clone(),
            cwd: v.cwd.clone(),
            env: v
                .env
                .iter()
                .map(|e| Variable {
                    name: e.name.clone(),
                    value: e.value.clone(),
                })
                .collect(),
            input: v.input.clone(),
            domain: v.execution_domain.clone(),
            runtime: v.max_runtime_ms,
            artifact_sha256: v.artifact_sha256.to_vec(),
        }
    }
}
impl FixedIntent {
    fn public(&self) -> Result<Intent> {
        let v = Intent {
            operation_id: self.operation.clone(),
            program: self.program.clone(),
            argv: self.argv.clone(),
            cwd: self.cwd.clone(),
            env: self
                .env
                .iter()
                .map(|e| crate::Environment {
                    name: e.name.clone(),
                    value: e.value.clone(),
                })
                .collect(),
            input: self.input.clone(),
            execution_domain: self.domain.clone(),
            max_runtime_ms: self.runtime,
            artifact_sha256: fixed(&self.artifact_sha256)?,
        };
        v.validate().map_err(|_| Error::Storage)?;
        Ok(v)
    }
}
impl From<&ExecutionFacts> for Facts {
    fn from(v: &ExecutionFacts) -> Self {
        Self {
            exit_code: v.exit_code,
            output_closed: v.output_closed,
            stdout_sha256: v.stdout_sha256.to_vec(),
            stderr_sha256: v.stderr_sha256.to_vec(),
            stdout_bytes: v.stdout_bytes,
            stderr_bytes: v.stderr_bytes,
        }
    }
}
impl Facts {
    fn public(&self) -> Result<ExecutionFacts> {
        let v = ExecutionFacts {
            exit_code: self.exit_code,
            output_closed: self.output_closed,
            stdout_sha256: fixed(&self.stdout_sha256)?,
            stderr_sha256: fixed(&self.stderr_sha256)?,
            stdout_bytes: self.stdout_bytes,
            stderr_bytes: self.stderr_bytes,
        };
        v.validate().map_err(|_| Error::Storage)?;
        Ok(v)
    }
}
impl State {
    fn phase(&self) -> Result<ToolPhase> {
        match self.phase {
            0 => Ok(ToolPhase::Proposed),
            1 => Ok(ToolPhase::Approved),
            2 => Ok(ToolPhase::Revoked),
            3 => Ok(ToolPhase::DispatchUnknown),
            4 => Ok(ToolPhase::Reported),
            _ => Err(Error::Storage),
        }
    }
    fn intent(&self) -> Result<Intent> {
        self.intent.as_ref().ok_or(Error::Storage)?.public()
    }
    fn info(&self) -> Result<ToolInfo> {
        let intent = self.intent()?;
        let info = ToolInfo {
            operation_id: intent.operation_id.clone(),
            session_id: self.session.clone(),
            intent_sha256: intent.digest()?,
            phase: self.phase()?,
            facts: self.facts.as_ref().map(Facts::public).transpose()?,
        };
        info.validate().map_err(|_| Error::Storage)?;
        Ok(info)
    }
    fn validate(&self, operation: &str) -> Result<()> {
        if self.version != 1
            || self.capacity as usize > MAX_RECORD_BYTES
            || self.capacity as usize != self.encoded_len()
            || self.padding.iter().any(|&b| b != 0)
            || self.expires == 0
        {
            return Err(Error::Storage);
        }
        identity(&self.session).map_err(|_| Error::Storage)?;
        fixed(&self.request_sha256)?;
        token(&self.proposer)?;
        token(&self.issuer)?;
        let intent = self.intent()?;
        if intent.operation_id != operation {
            return Err(Error::Storage);
        }
        let phase = self.phase()?;
        match phase {
            ToolPhase::Proposed => {
                if !self.executor.is_empty()
                    || !self.permit.is_empty()
                    || !self.claim.is_empty()
                    || self.invocation_started
                    || self.revoked
                {
                    return Err(Error::Storage);
                }
            }
            ToolPhase::Approved => {
                token(&self.executor)?;
                token(&self.permit)?;
                if !self.claim.is_empty() || self.invocation_started || self.revoked {
                    return Err(Error::Storage);
                }
            }
            ToolPhase::Revoked => {
                if !self.revoked || !self.claim.is_empty() || self.invocation_started {
                    return Err(Error::Storage);
                }
            }
            ToolPhase::DispatchUnknown | ToolPhase::Reported => {
                token(&self.executor)?;
                token(&self.permit)?;
                token(&self.claim)?;
            }
        }
        if phase == ToolPhase::Reported {
            fixed(&self.report_sha256)?;
            if !self.invocation_started {
                return Err(Error::Storage);
            }
        } else if !self.report_sha256.is_empty() {
            return Err(Error::Storage);
        }
        if let Some(observed) = &self.observed_facts {
            observed.public()?;
            if !self.invocation_started
                || !matches!(phase, ToolPhase::DispatchUnknown | ToolPhase::Reported)
                || phase == ToolPhase::Reported && self.facts.as_ref() != Some(observed)
            {
                return Err(Error::Storage);
            }
        } else if phase == ToolPhase::Reported {
            return Err(Error::Storage);
        }
        self.info()?;
        Ok(())
    }
    // Keep every transition exactly the original size. This reserves terminal
    // facts before claim, so competing writers cannot consume their capacity.
    fn encode_reserved(&mut self) -> Result<Vec<u8>> {
        self.padding.clear();
        if self.capacity == 0 {
            self.capacity =
                u32::try_from(self.encoded_len() + TERMINAL_RESERVE).map_err(|_| Error::Limit)?;
        }
        let capacity = self.capacity as usize;
        if capacity > MAX_RECORD_BYTES {
            return Err(Error::Limit);
        }
        for _ in 0..8 {
            let len = self.encoded_len();
            if len == capacity {
                return Ok(self.encode_to_vec());
            }
            let padding = self.padding.len();
            let next = if len < capacity {
                padding.checked_add(capacity - len).ok_or(Error::Limit)?
            } else {
                padding.checked_sub(len - capacity).ok_or(Error::Limit)?
            };
            self.padding.resize(next, 0);
        }
        Err(Error::Storage)
    }
}
fn load(runtime: &HostRuntime, operation: &str) -> Result<(Record, State)> {
    identity(operation)?;
    let record = runtime
        .store_local()
        .load_agent_ledger_local(&domain(), operation)
        .map_err(core_error)?
        .ok_or(Error::NotFound)?;
    if record.payload().len() > MAX_RECORD_BYTES {
        return Err(Error::Storage);
    }
    let state = State::decode(record.payload()).map_err(|_| Error::Storage)?;
    if state.encode_to_vec() != record.payload() {
        return Err(Error::Storage);
    }
    state.validate(operation)?;
    Ok((record, state))
}
fn save(
    runtime: &mut HostRuntime,
    operation: &str,
    expected: u64,
    state: &mut State,
) -> Result<()> {
    let payload = state.encode_reserved()?;
    state.validate(operation)?;
    let next = expected.checked_add(1).ok_or(Error::Limit)?;
    let record = Record::new(domain(), operation, next, payload).map_err(core_error)?;
    runtime
        .store_local_mut()
        .compare_exchange_agent_ledger_local(&record, expected)
        .map_err(core_error)
}
impl SessionExecHost {
    /// Read retained host observations without advancing the historical phase.
    /// A new read admission may inspect an old Unknown result; it cannot restore
    /// the original execution admission, permit or consumed callback.
    pub fn inspect_tool_observation(
        &self,
        runtime: &HostRuntime,
        connection: &Connection,
        reader: &Admission,
        operation: &str,
        now: u64,
    ) -> Result<Option<ExecutionFacts>> {
        let _fence = self.execution_fence()?;
        let (_, state) = load(runtime, operation)?;
        self.check(
            runtime,
            connection,
            reader,
            Right::SessionRead,
            Some(&state.session),
            now,
        )?;
        let observation = state
            .observed_facts
            .as_ref()
            .map(Facts::public)
            .transpose()?;
        self.check(
            runtime,
            connection,
            reader,
            Right::SessionRead,
            Some(&state.session),
            now,
        )?;
        Ok(observation)
    }
    /// Read the entire fixed proposal for trusted review before approval.
    /// Wire Inspect deliberately exposes no credentials, input or permit.
    pub fn review_tool(
        &self,
        runtime: &HostRuntime,
        operation: &str,
        now: u64,
    ) -> Result<ToolReview> {
        let _fence = self.execution_fence()?;
        let (_, state) = load(runtime, operation)?;
        self.live_proposal(runtime, &state, now)?;
        if state.phase()? != ToolPhase::Proposed {
            return Err(Error::Denied);
        }
        let intent = state.intent()?;
        Ok(ToolReview {
            proposal_sha256: fixed(&state.request_sha256)?,
            intent_sha256: intent.digest()?,
            intent,
            session_id: state.session,
            session_epoch: state.session_epoch,
            expires_ms: state.expires,
        })
    }
    fn live_proposal(&self, runtime: &HostRuntime, state: &State, now: u64) -> Result<()> {
        if token(&state.issuer)? != self.issuer_nonce() || now >= state.expires {
            return Err(Error::Denied);
        }
        self.check_nonce(
            runtime,
            token(&state.proposer)?,
            Right::Propose,
            &state.session,
            now,
        )?;
        if crate::session::require_active(runtime, &state.session)? != state.session_epoch {
            return Err(Error::Conflict);
        }
        Ok(())
    }
    fn live_executor(
        &self,
        runtime: &HostRuntime,
        connection: &Connection,
        admission: &Admission,
        state: &State,
        clock: &mut impl FnMut() -> u64,
    ) -> Result<()> {
        self.live_bound_inputs(runtime, connection, admission, state, clock())?;
        if crate::session::require_active(runtime, &state.session)? != state.session_epoch {
            return Err(Error::Conflict);
        }
        // Session decoding validates its entire retained history. Sample the
        // clock after that work, then check only the already bound live inputs.
        self.live_bound_inputs(runtime, connection, admission, state, clock())
    }
    fn live_bound_inputs(
        &self,
        runtime: &HostRuntime,
        connection: &Connection,
        admission: &Admission,
        state: &State,
        now: u64,
    ) -> Result<()> {
        self.check(
            runtime,
            connection,
            admission,
            Right::Execute,
            Some(&state.session),
            now,
        )?;
        if token(&state.issuer)? != self.issuer_nonce()
            || token(&state.executor)? != admission.nonce()
            || state.intent.as_ref().ok_or(Error::Storage)?.domain != admission.execution_domain()
            || state.revoked
            || now >= state.expires
        {
            return Err(Error::Denied);
        }
        self.check_nonce(
            runtime,
            token(&state.proposer)?,
            Right::Propose,
            &state.session,
            now,
        )
    }
    pub(crate) fn safe_dispatch(
        &self,
        runtime: &mut HostRuntime,
        connection: &Connection,
        admission: &Admission,
        request: &Request,
        clock: &mut impl FnMut() -> u64,
    ) -> Result<Outcome> {
        self.safe_dispatch_inner(runtime, connection, admission, request, clock)
    }
    pub(crate) fn finalize_safe(
        &self,
        runtime: &HostRuntime,
        connection: &Connection,
        admission: &Admission,
        request: &Request,
        clock: &mut impl FnMut() -> u64,
    ) -> Result<()> {
        match request.action() {
            Action::Propose { session_id, .. } => self
                .check(
                    runtime,
                    connection,
                    admission,
                    Right::Propose,
                    Some(session_id),
                    clock(),
                )
                .map_err(|_| Error::CommitUnknown)?,
            Action::Claim { operation_id, .. } => {
                let (_, state) = load(runtime, operation_id)?;
                self.live_executor(runtime, connection, admission, &state, clock)
                    .map_err(|_| Error::CommitUnknown)?;
            }
            Action::Report { operation_id, .. } => {
                let (_, state) = load(runtime, operation_id)?;
                self.check_reporter(runtime, connection, admission, &state, clock())
                    .map_err(|_| Error::CommitUnknown)?;
            }
            Action::Inspect { operation_id } => {
                let (_, state) = load(runtime, operation_id)?;
                self.check(
                    runtime,
                    connection,
                    admission,
                    Right::SessionRead,
                    Some(&state.session),
                    clock(),
                )?;
            }
            _ => return Err(Error::Contract),
        }
        Ok(())
    }
    fn safe_dispatch_inner(
        &self,
        runtime: &mut HostRuntime,
        connection: &Connection,
        admission: &Admission,
        request: &Request,
        clock: &mut impl FnMut() -> u64,
    ) -> Result<Outcome> {
        match request.action() {
            Action::Propose { session_id, intent } => {
                self.check(
                    runtime,
                    connection,
                    admission,
                    Right::Propose,
                    Some(session_id),
                    clock(),
                )?;
                intent.validate()?;
                if intent.execution_domain != admission.execution_domain() {
                    return Err(Error::Denied);
                }
                let epoch = crate::session::require_active(runtime, session_id)?;
                if let Some(record) = runtime
                    .store_local()
                    .load_agent_ledger_local(&domain(), &intent.operation_id)
                    .map_err(core_error)?
                {
                    let (_, state) = load(runtime, &intent.operation_id)?;
                    if state.request_sha256 != request.digest()
                        || token(&state.proposer)? != admission.nonce()
                        || record.revision() == 0
                    {
                        return Err(Error::Conflict);
                    }
                    self.live_proposal(runtime, &state, clock())?;
                    return Ok(Outcome::Tool(state.info()?));
                }
                let mut state = State {
                    version: 1,
                    session: session_id.clone(),
                    session_epoch: epoch,
                    request_sha256: request.digest().to_vec(),
                    proposer: admission.nonce().to_vec(),
                    issuer: self.issuer_nonce().to_vec(),
                    expires: admission.expires(),
                    intent: Some(intent.into()),
                    phase: 0,
                    executor: vec![],
                    permit: vec![],
                    claim: vec![],
                    invocation_started: false,
                    revoked: false,
                    facts: None,
                    report_sha256: vec![],
                    capacity: 0,
                    padding: vec![],
                    observed_facts: None,
                };
                self.check(
                    runtime,
                    connection,
                    admission,
                    Right::Propose,
                    Some(session_id),
                    clock(),
                )?;
                save(runtime, &intent.operation_id, 0, &mut state)?;
                Ok(Outcome::Tool(state.info()?))
            }
            Action::Claim {
                operation_id,
                permit,
            } => {
                let (record, mut state) = load(runtime, operation_id)?;
                self.live_executor(runtime, connection, admission, &state, clock)?;
                if state.phase()? != ToolPhase::Approved || token(&state.permit)? != *permit {
                    return Err(Error::Denied);
                }
                state.claim = random()?.to_vec();
                state.phase = 3;
                self.live_executor(runtime, connection, admission, &state, clock)?;
                let outcome = Outcome::Claimed {
                    info: state.info()?,
                    intent: state.intent()?,
                    claim: token(&state.claim)?,
                };
                // The winning reply must fit before consuming this approval.
                crate::Reply::new(request, outcome.clone())?.encode()?;
                self.live_bound_inputs(runtime, connection, admission, &state, clock())?;
                save(runtime, operation_id, record.revision(), &mut state)?;
                // Only the strict-CAS winner receives input; an uncertain commit
                // never returns a claim token and must never reach execution.
                self.live_executor(runtime, connection, admission, &state, clock)
                    .map_err(|_| Error::CommitUnknown)?;
                Ok(outcome)
            }
            Action::Report {
                operation_id,
                claim,
                facts,
            } => {
                let (record, mut state) = load(runtime, operation_id)?;
                // A factual report still requires the original live executor;
                // retained historical tokens cannot reconstruct its authority.
                self.check_reporter(runtime, connection, admission, &state, clock())?;
                if token(&state.claim)? != *claim {
                    return Err(Error::Denied);
                }
                facts.validate()?;
                if state.phase()? == ToolPhase::Reported {
                    if state.report_sha256 != request.digest() {
                        return Err(Error::Conflict);
                    }
                    return Ok(Outcome::Tool(state.info()?));
                }
                if state.phase()? != ToolPhase::DispatchUnknown || !state.invocation_started {
                    return Err(Error::Denied);
                }
                if state
                    .observed_facts
                    .as_ref()
                    .map(Facts::public)
                    .transpose()?
                    .as_ref()
                    != Some(facts)
                {
                    return Err(Error::Conflict);
                }
                state.phase = 4;
                state.facts = Some(facts.into());
                state.report_sha256 = request.digest().to_vec();
                save(runtime, operation_id, record.revision(), &mut state)?;
                Ok(Outcome::Tool(state.info()?))
            }
            Action::Inspect { operation_id } => {
                let (_, state) = load(runtime, operation_id)?;
                self.check(
                    runtime,
                    connection,
                    admission,
                    Right::SessionRead,
                    Some(&state.session),
                    clock(),
                )?;
                Ok(Outcome::Tool(state.info()?))
            }
            _ => Err(Error::Contract),
        }
    }
    fn check_reporter(
        &self,
        runtime: &HostRuntime,
        connection: &Connection,
        admission: &Admission,
        state: &State,
        now: u64,
    ) -> Result<()> {
        // No stale admission is revived for reporting. Callers retain Unknown
        // if authorization expired before the factual receipt was accepted.
        self.check(
            runtime,
            connection,
            admission,
            Right::Execute,
            Some(&state.session),
            now,
        )?;
        if token(&state.issuer)? != self.issuer_nonce()
            || token(&state.executor)? != admission.nonce()
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    /// Trusted host approval for this exact immutable proposal. No wire approve.
    #[allow(clippy::too_many_arguments)]
    pub fn approve(
        &self,
        runtime: &mut HostRuntime,
        connection: &Connection,
        executor: &Admission,
        operation: &str,
        expected_proposal_digest: [u8; 32],
        expected_intent_digest: [u8; 32],
        now: u64,
    ) -> Result<[u8; 32]> {
        let _fence = self.execution_fence()?;
        let (record, mut state) = load(runtime, operation)?;
        self.check(
            runtime,
            connection,
            executor,
            Right::Execute,
            Some(&state.session),
            now,
        )?;
        self.live_proposal(runtime, &state, now)?;
        let intent = state.intent()?;
        if state.phase()? != ToolPhase::Proposed
            || state.request_sha256 != expected_proposal_digest
            || intent.digest()? != expected_intent_digest
            || intent.execution_domain != executor.execution_domain()
        {
            return Err(Error::Denied);
        }
        state.executor = executor.nonce().to_vec();
        state.permit = random()?.to_vec();
        state.phase = 1;
        state.expires = state.expires.min(executor.expires());
        save(runtime, operation, record.revision(), &mut state)?;
        token(&state.permit)
    }
    /// Revoke a local tool. A consumed claim stays Unknown; revocation cannot
    /// rewrite history as proof that an effect never happened.
    pub fn revoke_tool(&self, runtime: &mut HostRuntime, operation: &str, now: u64) -> Result<()> {
        let _fence = self.execution_fence()?;
        let (record, mut state) = load(runtime, operation)?;
        if token(&state.issuer)? != self.issuer_nonce() {
            return Err(Error::Denied);
        }
        self.check_nonce(
            runtime,
            token(&state.proposer)?,
            Right::Propose,
            &state.session,
            now,
        )?;
        if state.phase()? == ToolPhase::Reported {
            return Err(Error::Conflict);
        }
        if state.revoked {
            return Ok(());
        }
        state.revoked = true;
        if matches!(state.phase()?, ToolPhase::Proposed | ToolPhase::Approved) {
            state.phase = 2;
        }
        save(runtime, operation, record.revision(), &mut state)
    }
    /// Invoke a trusted fixed-input executor at most once. The durable marker is
    /// committed before calling it. Any failed/uncertain outcome remains Unknown.
    /// The callback, not this SDK, owns process, timeout and output enforcement.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_claimed(
        &self,
        runtime: &mut HostRuntime,
        connection: &Connection,
        executor: &Admission,
        operation: &str,
        claim: [u8; 32],
        mut clock: impl FnMut() -> u64,
        callback: impl FnOnce(&Intent) -> Result<ExecutionFacts>,
    ) -> Result<ExecutionFacts> {
        let _fence = self.execution_fence()?;
        let (record, mut state) = load(runtime, operation)?;
        self.live_executor(runtime, connection, executor, &state, &mut clock)?;
        if state.phase()? != ToolPhase::DispatchUnknown
            || state.invocation_started
            || token(&state.claim)? != claim
        {
            return Err(Error::Denied);
        }
        let intent = state.intent()?;
        self.live_bound_inputs(runtime, connection, executor, &state, clock())?;
        state.invocation_started = true;
        save(runtime, operation, record.revision(), &mut state)?;
        self.live_executor(runtime, connection, executor, &state, &mut clock)
            .map_err(|_| Error::CommitUnknown)?;
        let facts = callback(&intent).map_err(|_| Error::CommitUnknown)?;
        facts.validate().map_err(|_| Error::CommitUnknown)?;
        let authorization = self.live_executor(runtime, connection, executor, &state, &mut clock);
        state.observed_facts = Some((&facts).into());
        save(runtime, operation, record.revision() + 1, &mut state)
            .map_err(|_| Error::CommitUnknown)?;
        authorization.map_err(|_| Error::CommitUnknown)?;
        self.live_bound_inputs(runtime, connection, executor, &state, clock())
            .map_err(|_| Error::CommitUnknown)?;
        Ok(facts)
    }
}
