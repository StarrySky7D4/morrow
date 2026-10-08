//! Durable, one-shot fixed-input execution coordination. No automatic OS backend.
use crate::{
    Action, Error, ExecutionFacts, Intent, Outcome, Request, Result, ToolInfo, ToolPhase,
    authority::{Admission, Right, SessionExecHost, core_error, random},
    hash, identity, validate_observation_progress,
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

/// Complete historical operation identity; none of these fields is authority.
/// A restored identity cannot revive a permit, claim, admission or callback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolIdentity {
    pub operation_id: String,
    pub proposal_sha256: [u8; 32],
    pub intent_sha256: [u8; 32],
    pub execution_domain: String,
    pub session_id: String,
    pub session_epoch: u64,
    pub generation: u64,
    pub original_issuer_nonce: [u8; 32],
}

/// Latest trusted observations and the separately preserved accepted receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolObservation {
    pub identity: ToolIdentity,
    pub record_revision: u64,
    pub record_sha256: [u8; 32],
    pub phase: ToolPhase,
    pub invocation_started: bool,
    pub latest: Option<ExecutionFacts>,
    pub observation_revision: u64,
    pub accepted_report: Option<ExecutionFacts>,
}

/// Compact persistent retirement history; no execution authority or payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolRetirement {
    pub operation_id: String,
    pub generation: u64,
    pub record_sha256: [u8; 32],
    pub historical_phase: ToolPhase,
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
    #[prost(uint64, tag = "20")]
    observation_revision: u64,
    #[prost(uint64, tag = "21")]
    generation: u64,
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
    hash(b"agent-session-exec-v1/tool/2")
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
            observation: self
                .observed_facts
                .as_ref()
                .map(Facts::public)
                .transpose()?,
            observation_revision: self.observation_revision,
        };
        info.validate().map_err(|_| Error::Storage)?;
        Ok(info)
    }
    fn identity(&self) -> Result<ToolIdentity> {
        let intent = self.intent()?;
        Ok(ToolIdentity {
            operation_id: intent.operation_id.clone(),
            proposal_sha256: fixed(&self.request_sha256)?,
            intent_sha256: intent.digest()?,
            execution_domain: intent.execution_domain,
            session_id: self.session.clone(),
            session_epoch: self.session_epoch,
            generation: self.generation,
            original_issuer_nonce: token(&self.issuer)?,
        })
    }
    fn observation(&self, record: &Record) -> Result<ToolObservation> {
        Ok(ToolObservation {
            identity: self.identity()?,
            record_revision: record.revision(),
            record_sha256: hash(record.container()),
            phase: self.phase()?,
            invocation_started: self.invocation_started,
            latest: self
                .observed_facts
                .as_ref()
                .map(Facts::public)
                .transpose()?,
            observation_revision: self.observation_revision,
            accepted_report: self.facts.as_ref().map(Facts::public).transpose()?,
        })
    }
    fn validate(&self, operation: &str) -> Result<()> {
        if self.version != 2
            || self.generation == 0
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
                || self.observation_revision == 0
            {
                return Err(Error::Storage);
            }
            if let Some(accepted) = &self.facts {
                validate_observation_progress(&accepted.public()?, &observed.public()?)
                    .map_err(|_| Error::Storage)?;
            }
        } else if phase == ToolPhase::Reported || self.observation_revision != 0 {
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
    if state.observation_revision > record.revision() {
        return Err(Error::Storage);
    }
    Ok((record, state))
}
/// Startup verification only; durable history never reconstructs live grants.
pub(crate) fn validate_profile_tool_record(
    runtime: &HostRuntime,
    record: &Record,
    current_generation: u64,
) -> Result<()> {
    let (loaded, state) = load(runtime, record.id()).map_err(|_| Error::Storage)?;
    if loaded.container() != record.container() || state.generation != current_generation {
        return Err(Error::Storage);
    }
    Ok(())
}
fn save(
    host: &SessionExecHost,
    runtime: &mut HostRuntime,
    operation: &str,
    expected: u64,
    state: &mut State,
) -> Result<Record> {
    let payload = state.encode_reserved()?;
    state.validate(operation)?;
    let next = expected.checked_add(1).ok_or(Error::Limit)?;
    if state.observation_revision > next {
        return Err(Error::Storage);
    }
    let record = Record::new(domain(), operation, next, payload).map_err(core_error)?;
    host.publish_profile_record(runtime, &record, expected)?;
    Ok(record)
}
fn require_closed_observation(state: &State) -> Result<()> {
    let observed = state
        .observed_facts
        .as_ref()
        .ok_or(Error::Denied)?
        .public()?;
    if observed.exit_code.is_none() || !observed.output_closed {
        return Err(Error::Denied);
    }
    Ok(())
}
impl SessionExecHost {
    /// Validate the original live execution grant for a previously started tool.
    /// This read-only check creates no permit, claim, callback or durable receipt.
    /// Historical identity and a new read admission cannot restore execution rights.
    pub fn validate_started_tool(
        &self,
        runtime: &HostRuntime,
        connection: &Connection,
        executor: &Admission,
        expected: &ToolIdentity,
        mut clock: impl FnMut() -> u64,
    ) -> Result<()> {
        let _fence = self.execution_fence()?;
        let (_, state) = load(runtime, &expected.operation_id)?;
        self.live_executor(runtime, connection, executor, &state, &mut clock)?;
        if !state.invocation_started
            || state.phase()? != ToolPhase::DispatchUnknown
            || state.identity()? != *expected
        {
            return Err(Error::Denied);
        }
        self.live_bound_inputs(runtime, connection, executor, &state, clock())
    }
    /// Review complete identity and record revision under current native owner
    /// authority. Historical identity is data, never restored execution authority.
    pub fn inspect_tool_record(
        &self,
        runtime: &HostRuntime,
        operation: &str,
    ) -> Result<ToolObservation> {
        let _fence = self.execution_fence()?;
        self.ensure_owner(runtime)?;
        let (record, state) = load(runtime, operation)?;
        self.ensure_owner(runtime)?;
        state.observation(&record)
    }

    /// Review the compact tombstone under current trusted owner authority.
    /// Rollover permanently closes this generation rather than reusing its IDs.
    pub fn inspect_retired_tool_record(
        &self,
        runtime: &HostRuntime,
        operation: &str,
    ) -> Result<Option<ToolRetirement>> {
        let _fence = self.execution_fence()?;
        self.ensure_owner(runtime)?;
        let summary = self.retired_identity_summary(runtime, &domain(), operation)?;
        summary
            .map(|(record_sha256, phase)| {
                let historical_phase = match phase {
                    2 => ToolPhase::Revoked,
                    3 => ToolPhase::DispatchUnknown,
                    4 => ToolPhase::Reported,
                    _ => return Err(Error::Storage),
                };
                Ok(ToolRetirement {
                    operation_id: operation.into(),
                    generation: self.generation(runtime)?,
                    record_sha256,
                    historical_phase,
                })
            })
            .transpose()
    }

    /// Retain independently verified late facts without renewing execution.
    /// Only a trusted native owner may use this path; the owner must verify the
    /// process/handles and output itself. SHA alone cannot prove prefix growth.
    /// The exact historical identity and expected ledger revision bind the CAS.
    pub fn reconcile_tool_observation(
        &self,
        runtime: &mut HostRuntime,
        expected_identity: &ToolIdentity,
        expected_revision: u64,
        facts: &ExecutionFacts,
    ) -> Result<ToolObservation> {
        let _fence = self.execution_fence()?;
        self.ensure_owner(runtime)?;
        let (record, mut state) = load(runtime, &expected_identity.operation_id)?;
        self.check_generation(runtime, state.generation)?;
        if state.identity()? != *expected_identity {
            return Err(Error::Denied);
        }
        if record.revision() != expected_revision {
            return Err(Error::Conflict);
        }
        if !state.invocation_started
            || !matches!(
                state.phase()?,
                ToolPhase::DispatchUnknown | ToolPhase::Reported
            )
        {
            return Err(Error::Denied);
        }
        facts.validate()?;
        if let Some(previous) = &state.observed_facts {
            validate_observation_progress(&previous.public()?, facts)?;
        }
        let next = expected_revision.checked_add(1).ok_or(Error::Limit)?;
        state.observed_facts = Some(facts.into());
        state.observation_revision = next;
        self.ensure_owner(runtime)?;
        let next_record = save(
            self,
            runtime,
            &expected_identity.operation_id,
            expected_revision,
            &mut state,
        )?;
        self.ensure_owner(runtime)
            .map_err(|_| Error::CommitUnknown)?;
        state.observation(&next_record)
    }

    /// Physically retire one reviewed execution record with a durable tombstone.
    /// A consumed Unknown keeps its historical classification. Retirement does
    /// not prove that external effects never happened or terminate a process.
    /// An unstarted SDK marker can be retired without calling its executor;
    /// a started marker requires independently retained exit and output closure.
    #[allow(clippy::too_many_arguments)]
    pub fn retire_tool(
        &self,
        runtime: &mut HostRuntime,
        connection: &Connection,
        retirer: &Admission,
        review: &ToolObservation,
        acknowledge_possible_effect: bool,
        mut clock: impl FnMut() -> u64,
    ) -> Result<[u8; 32]> {
        let _fence = self.execution_fence()?;
        self.ensure_owner(runtime)?;
        let (record, state) = load(runtime, &review.identity.operation_id)?;
        self.check(
            runtime,
            connection,
            retirer,
            Right::Retire,
            Some(&state.session),
            clock(),
        )?;
        if state.identity()? != review.identity {
            return Err(Error::Denied);
        }
        if state.observation(&record)? != *review {
            return Err(Error::Conflict);
        }
        match state.phase()? {
            ToolPhase::Revoked => {}
            ToolPhase::DispatchUnknown => {
                if !acknowledge_possible_effect {
                    return Err(Error::Denied);
                }
                if state.invocation_started {
                    require_closed_observation(&state)?;
                }
            }
            ToolPhase::Reported => require_closed_observation(&state)?,
            ToolPhase::Proposed | ToolPhase::Approved => return Err(Error::Denied),
        }
        self.check(
            runtime,
            connection,
            retirer,
            Right::Retire,
            Some(&state.session),
            clock(),
        )?;
        self.ensure_owner(runtime)?;
        self.retire_profile_record_with_phase(runtime, &record, state.phase)?;
        self.check(
            runtime,
            connection,
            retirer,
            Right::Retire,
            Some(&state.session),
            clock(),
        )
        .map_err(|_| Error::CommitUnknown)?;
        Ok(review.record_sha256)
    }

    /// Called under the profile execution fence before physical session delete.
    /// Every retained tool, including an Unknown, is a reference until reviewed
    /// tool retirement has atomically removed it and retained its tombstone.
    pub(crate) fn assert_session_tools_released(
        &self,
        runtime: &HostRuntime,
        session: &str,
    ) -> Result<()> {
        self.ensure_owner(runtime)?;
        identity(session)?;
        let ids = runtime
            .store_local()
            .agent_ledger_ids_local(&domain())
            .map_err(core_error)?;
        for operation in ids {
            let (_, state) = load(runtime, &operation)?;
            if state.session == session {
                return Err(Error::Conflict);
            }
        }
        self.ensure_owner(runtime)
    }

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
        self.check_generation(runtime, state.generation)?;
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
        self.check_generation(runtime, request.generation())?;
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
                    version: 2,
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
                    observation_revision: 0,
                    generation: request.generation(),
                };
                self.check(
                    runtime,
                    connection,
                    admission,
                    Right::Propose,
                    Some(session_id),
                    clock(),
                )?;
                save(self, runtime, &intent.operation_id, 0, &mut state)?;
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
                save(self, runtime, operation_id, record.revision(), &mut state)?;
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
                save(self, runtime, operation_id, record.revision(), &mut state)?;
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
        save(self, runtime, operation, record.revision(), &mut state)?;
        token(&state.permit)
    }
    /// Revoke a local tool. A consumed claim stays Unknown; revocation cannot
    /// rewrite history as proof that an effect never happened.
    pub fn revoke_tool(&self, runtime: &mut HostRuntime, operation: &str, _now: u64) -> Result<()> {
        let _fence = self.execution_fence()?;
        self.ensure_owner(runtime)?;
        let (record, mut state) = load(runtime, operation)?;
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
        save(self, runtime, operation, record.revision(), &mut state)?;
        Ok(())
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
        save(self, runtime, operation, record.revision(), &mut state)?;
        self.live_executor(runtime, connection, executor, &state, &mut clock)
            .map_err(|_| Error::CommitUnknown)?;
        let facts = callback(&intent).map_err(|_| Error::CommitUnknown)?;
        facts.validate().map_err(|_| Error::CommitUnknown)?;
        let authorization = self.live_executor(runtime, connection, executor, &state, &mut clock);
        state.observed_facts = Some((&facts).into());
        state.observation_revision = record
            .revision()
            .checked_add(2)
            .ok_or(Error::CommitUnknown)?;
        save(self, runtime, operation, record.revision() + 1, &mut state)
            .map_err(|_| Error::CommitUnknown)?;
        authorization.map_err(|_| Error::CommitUnknown)?;
        self.live_bound_inputs(runtime, connection, executor, &state, clock())
            .map_err(|_| Error::CommitUnknown)?;
        Ok(facts)
    }
}
