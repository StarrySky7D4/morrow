//! Bounded durable session and one-shot safe execution interface.
//! Parsing wire data never creates a grant, approval or execution-domain authority.
#![deny(unsafe_code)]

use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
    traits::{HasStructSize, IntoInternalStructReader},
};
use morrow_core::runtime;
use sha2::{Digest, Sha256};

#[allow(clippy::all, unsafe_code)]
pub(crate) mod session_exec_capnp {
    include!(concat!(env!("OUT_DIR"), "/session_exec_capnp.rs"));
}
use session_exec_capnp as wire;

#[cfg(not(target_arch = "wasm32"))]
pub mod authority;
#[cfg(not(target_arch = "wasm32"))]
pub mod safe_exec;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;

pub const PROFILE: &str = "agent-session-exec-v1";
pub const VERSION: u16 = 1;
pub const REVISION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 128 * 1024;
pub const MAX_BODY_BYTES: usize = 32 * 1024;
pub const MAX_EVENTS: usize = 16;
pub const MAX_SESSIONS: usize = 16;
pub const MAX_ARGUMENTS: usize = 16;
pub const MAX_ENVIRONMENT: usize = 16;
pub const MAX_TEXT_BYTES: usize = 4096;
pub const MAX_RUNTIME_MS: u64 = 300_000;
pub const MAX_OUTPUT_BYTES: u64 = 1024 * 1024;
pub const SCHEMA: &[u8] = include_bytes!("../contracts/session_exec.capnp");
include!(concat!(env!("OUT_DIR"), "/schema_digest.rs"));
pub const fn schema_digest() -> [u8; 32] {
    SCHEMA_DIGEST
}
pub fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Contract,
    Limit,
    Correlation,
    Denied,
    Conflict,
    NotFound,
    CommitUnknown,
    Storage,
}
pub type Result<T> = std::result::Result<T, Error>;
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "session execution {self:?}")
    }
}
impl std::error::Error for Error {}
fn invalid<T>(_: T) -> Error {
    Error::Invalid
}
pub(crate) fn identity(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn bounded_text(value: &str) -> Result<()> {
    if value.len() > MAX_TEXT_BYTES {
        return Err(Error::Limit);
    }
    if value.chars().any(char::is_control) {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn absolute_path(value: &str) -> Result<()> {
    bounded_text(value)?;
    let bytes = value.as_bytes();
    let unix = value.starts_with('/');
    let windows = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    if (!unix && !windows) || value.split(['/', '\\']).any(|p| matches!(p, "." | "..")) {
        return Err(Error::Invalid);
    }
    Ok(())
}

#[derive(Clone, PartialEq, Eq)]
pub struct Event {
    pub event_id: String,
    pub body: Vec<u8>,
}
impl Event {
    pub fn validate(&self) -> Result<()> {
        identity(&self.event_id)?;
        if self.body.len() > MAX_BODY_BYTES {
            return Err(Error::Limit);
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct Environment {
    pub name: String,
    pub value: String,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Intent {
    pub operation_id: String,
    pub artifact_sha256: [u8; 32],
    pub program: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub env: Vec<Environment>,
    pub input: Vec<u8>,
    pub execution_domain: String,
    pub max_runtime_ms: u64,
}
impl Intent {
    pub fn validate(&self) -> Result<()> {
        identity(&self.operation_id)?;
        if self.artifact_sha256 == [0; 32] {
            return Err(Error::Invalid);
        }
        identity(&self.execution_domain)?;
        absolute_path(&self.program)?;
        absolute_path(&self.cwd)?;
        if self.argv.len() > MAX_ARGUMENTS
            || self.env.len() > MAX_ENVIRONMENT
            || self.input.len() > MAX_BODY_BYTES
            || self.max_runtime_ms == 0
            || self.max_runtime_ms > MAX_RUNTIME_MS
        {
            return Err(Error::Limit);
        }
        for arg in &self.argv {
            bounded_text(arg)?;
        }
        let mut keys = std::collections::BTreeSet::new();
        for env in &self.env {
            let mut chars = env.name.bytes();
            if !chars
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
                || !chars.all(|c| c.is_ascii_alphanumeric() || c == b'_')
                || env.name.len() > 256
                || !keys.insert(env.name.to_ascii_uppercase())
            {
                return Err(Error::Invalid);
            }
            bounded_text(&env.value)?;
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<[u8; 32]> {
        self.validate()?;
        let mut message = Builder::new_default();
        set_intent(message.init_root::<wire::intent::Builder<'_>>(), self);
        Ok(hash(&finish(&message)?))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionFacts {
    pub exit_code: Option<i32>,
    pub output_closed: bool,
    pub stdout_sha256: [u8; 32],
    pub stderr_sha256: [u8; 32],
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
}
impl ExecutionFacts {
    pub fn validate(&self) -> Result<()> {
        if self.stdout_bytes > MAX_OUTPUT_BYTES || self.stderr_bytes > MAX_OUTPUT_BYTES {
            return Err(Error::Limit);
        }
        if self.stdout_bytes == 0 && self.stdout_sha256 != hash(&[])
            || self.stderr_bytes == 0 && self.stderr_sha256 != hash(&[])
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq)]
pub enum Action {
    Create {
        session_id: String,
        parent: Option<String>,
        parent_tail: u64,
    },
    List,
    Snapshot {
        session_id: String,
        after: u64,
        limit: u32,
    },
    OpenWriter {
        session_id: String,
        expected_epoch: u64,
    },
    Append {
        session_id: String,
        epoch: u64,
        expected_tail: u64,
        events: Vec<Event>,
    },
    Checkpoint {
        session_id: String,
        epoch: u64,
        expected_tail: u64,
        state: Vec<u8>,
    },
    Archive {
        session_id: String,
        epoch: u64,
        expected_tail: u64,
    },
    Propose {
        session_id: String,
        intent: Intent,
    },
    Claim {
        operation_id: String,
        permit: [u8; 32],
    },
    Report {
        operation_id: String,
        claim: [u8; 32],
        facts: ExecutionFacts,
    },
    Inspect {
        operation_id: String,
    },
}
impl Action {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Create {
                session_id,
                parent,
                parent_tail,
            } => {
                identity(session_id)?;
                if let Some(parent) = parent {
                    identity(parent)?;
                    if parent == session_id {
                        return Err(Error::Invalid);
                    }
                } else if *parent_tail != 0 {
                    return Err(Error::Invalid);
                }
            }
            Self::List => {}
            Self::Snapshot {
                session_id,
                after,
                limit,
            } => {
                identity(session_id)?;
                if *after == u64::MAX || *limit == 0 || *limit as usize > MAX_EVENTS {
                    return Err(Error::Limit);
                }
            }
            Self::OpenWriter {
                session_id,
                expected_epoch,
            } => {
                identity(session_id)?;
                if *expected_epoch == u64::MAX {
                    return Err(Error::Limit);
                }
            }
            Self::Append {
                session_id,
                epoch,
                expected_tail,
                events,
            } => {
                identity(session_id)?;
                if *epoch == 0
                    || events.is_empty()
                    || events.len() > MAX_EVENTS
                    || expected_tail.checked_add(events.len() as u64).is_none()
                {
                    return Err(Error::Limit);
                }
                let mut ids = std::collections::BTreeSet::new();
                for event in events {
                    event.validate()?;
                    if !ids.insert(&event.event_id) {
                        return Err(Error::Invalid);
                    }
                }
            }
            Self::Checkpoint {
                session_id,
                epoch,
                state,
                ..
            } => {
                identity(session_id)?;
                if *epoch == 0 || state.len() > MAX_BODY_BYTES {
                    return Err(Error::Limit);
                }
            }
            Self::Archive {
                session_id, epoch, ..
            } => {
                identity(session_id)?;
                if *epoch == 0 {
                    return Err(Error::Limit);
                }
            }
            Self::Propose { session_id, intent } => {
                identity(session_id)?;
                intent.validate()?;
            }
            Self::Claim { operation_id, .. } | Self::Inspect { operation_id } => {
                identity(operation_id)?
            }
            Self::Report {
                operation_id,
                facts,
                ..
            } => {
                identity(operation_id)?;
                facts.validate()?;
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionInfo {
    pub session_id: String,
    pub parent: Option<String>,
    pub parent_tail: u64,
    pub epoch: u64,
    pub tail: u64,
    pub floor: u64,
    pub archived: bool,
    pub revision: u64,
    pub checkpoint_tail: u64,
    pub checkpoint_sha256: [u8; 32],
    pub checkpoint_sealed: bool,
    pub parent_checkpoint_sha256: Option<[u8; 32]>,
}
impl SessionInfo {
    pub fn validate(&self) -> Result<()> {
        identity(&self.session_id)?;
        if let Some(parent) = &self.parent {
            identity(parent)?;
            if parent == &self.session_id {
                return Err(Error::Invalid);
            }
        } else if self.parent_tail != 0 {
            return Err(Error::Invalid);
        }
        if self.parent.is_some() != self.parent_checkpoint_sha256.is_some() {
            return Err(Error::Invalid);
        }
        if self.revision == 0
            || self.floor == 0
            || self.floor > self.tail.saturating_add(1)
            || self.checkpoint_tail > self.tail
            || self.floor > 1
                && (!self.checkpoint_sealed || self.checkpoint_tail.saturating_add(1) < self.floor)
            || !self.checkpoint_sealed
                && (self.checkpoint_tail != 0 || self.checkpoint_sha256 != hash(&[]))
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredEvent {
    pub sequence: u64,
    pub event: Event,
}
#[derive(Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub info: SessionInfo,
    pub events: Vec<StoredEvent>,
    pub gap: bool,
    pub checkpoint: Vec<u8>,
}
impl SessionSnapshot {
    pub fn validate(&self) -> Result<()> {
        self.info.validate()?;
        if self.events.len() > MAX_EVENTS || self.checkpoint.len() > MAX_BODY_BYTES {
            return Err(Error::Limit);
        }
        if !self.info.checkpoint_sealed && !self.checkpoint.is_empty()
            || hash(&self.checkpoint) != self.info.checkpoint_sha256
        {
            return Err(Error::Correlation);
        }
        let mut last = 0u64;
        let mut ids = std::collections::BTreeSet::new();
        for stored in &self.events {
            stored.event.validate()?;
            if stored.sequence < self.info.floor
                || stored.sequence > self.info.tail
                || last != 0 && Some(stored.sequence) != last.checked_add(1)
                || !ids.insert(&stored.event.event_id)
            {
                return Err(Error::Correlation);
            }
            last = stored.sequence;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolPhase {
    Proposed,
    Approved,
    Revoked,
    DispatchUnknown,
    Reported,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolInfo {
    pub operation_id: String,
    pub session_id: String,
    pub intent_sha256: [u8; 32],
    pub phase: ToolPhase,
    pub facts: Option<ExecutionFacts>,
}
impl ToolInfo {
    pub fn validate(&self) -> Result<()> {
        identity(&self.operation_id)?;
        identity(&self.session_id)?;
        if let Some(facts) = &self.facts {
            facts.validate()?;
        }
        if (self.phase == ToolPhase::Reported) != self.facts.is_some() {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq)]
pub enum Outcome {
    Session(SessionInfo),
    Sessions(Vec<SessionInfo>),
    Snapshot(SessionSnapshot),
    Tool(ToolInfo),
    Claimed {
        info: ToolInfo,
        intent: Intent,
        claim: [u8; 32],
    },
    Rejected(Error),
}
#[derive(Clone, PartialEq, Eq)]
pub struct Request {
    request_id: String,
    action: Action,
    bytes: Vec<u8>,
    digest: [u8; 32],
}
#[derive(Clone, PartialEq, Eq)]
pub struct Reply {
    pub request_id: String,
    pub request_sha256: [u8; 32],
    pub outcome: Outcome,
}

fn exact_layout<'a>(
    root: impl IntoInternalStructReader<'a>,
    data: u16,
    pointers: u16,
) -> Result<()> {
    let root = root.into_internal_struct_reader();
    if root.get_data_section_size() != u32::from(data) * 64
        || root.get_pointer_section_size() != pointers
    {
        return Err(Error::Contract);
    }
    Ok(())
}
macro_rules! layout {
    ($root:expr,$ty:path) => {{
        let size = <$ty as HasStructSize>::STRUCT_SIZE;
        exact_layout($root, size.data, size.pointers)?;
    }};
}
fn text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    let value = value.map_err(invalid)?;
    if value.len() > MAX_TEXT_BYTES {
        return Err(Error::Limit);
    }
    let value = value.to_str().map_err(invalid)?;
    bounded_text(value)?;
    Ok(value.to_owned())
}
fn id_text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    let value = text(value)?;
    identity(&value)?;
    Ok(value)
}
fn data(value: capnp::Result<&[u8]>, max: usize) -> Result<Vec<u8>> {
    let value = value.map_err(invalid)?;
    if value.len() > max {
        return Err(Error::Limit);
    }
    Ok(value.to_vec())
}
fn hash32(value: capnp::Result<&[u8]>) -> Result<[u8; 32]> {
    value.map_err(invalid)?.try_into().map_err(invalid)
}
fn read(bytes: &[u8]) -> Result<capnp::message::Reader<serialize::OwnedSegments>> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    let mut remaining = bytes;
    let message = serialize::read_message(
        &mut remaining,
        ReaderOptions {
            traversal_limit_in_words: Some(MAX_FRAME_BYTES / 8 * 4),
            nesting_limit: 16,
        },
    )
    .map_err(invalid)?;
    if !remaining.is_empty() {
        return Err(Error::Invalid);
    }
    Ok(message)
}
fn finish(message: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let bytes = serialize::write_message_to_words(message);
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
fn contract(
    version: u16,
    revision: u16,
    schema: &[u8],
    core_version: u16,
    rt: &[u8],
    content: &[u8],
) -> Result<()> {
    if version != VERSION
        || revision != REVISION
        || schema != SCHEMA_DIGEST
        || core_version != runtime::PROTOCOL_VERSION
        || rt != runtime::runtime_digest()
        || content != runtime::content_digest()
    {
        return Err(Error::Contract);
    }
    Ok(())
}
fn set_event(mut out: wire::event::Builder<'_>, event: &Event) {
    out.set_event_id(event.event_id.as_str());
    out.set_body(&event.body);
}
fn get_event(root: wire::event::Reader<'_>) -> Result<Event> {
    layout!(root, wire::event::Builder<'_>);
    let value = Event {
        event_id: id_text(root.get_event_id())?,
        body: data(root.get_body(), MAX_BODY_BYTES)?,
    };
    value.validate()?;
    Ok(value)
}
fn set_intent(mut out: wire::intent::Builder<'_>, intent: &Intent) {
    out.set_operation_id(intent.operation_id.as_str());
    out.set_artifact_sha256(&intent.artifact_sha256);
    out.set_program(intent.program.as_str());
    out.set_cwd(intent.cwd.as_str());
    out.set_input(&intent.input);
    out.set_execution_domain(intent.execution_domain.as_str());
    out.set_max_runtime_ms(intent.max_runtime_ms);
    let mut args = out.reborrow().init_argv(intent.argv.len() as u32);
    for (i, arg) in intent.argv.iter().enumerate() {
        args.set(i as u32, arg.as_str());
    }
    let mut env = out.init_env(intent.env.len() as u32);
    for (i, value) in intent.env.iter().enumerate() {
        let mut item = env.reborrow().get(i as u32);
        item.set_name(value.name.as_str());
        item.set_value(value.value.as_str());
    }
}
fn get_intent(root: wire::intent::Reader<'_>) -> Result<Intent> {
    layout!(root, wire::intent::Builder<'_>);
    let args = root.get_argv().map_err(invalid)?;
    let env = root.get_env().map_err(invalid)?;
    if args.len() as usize > MAX_ARGUMENTS || env.len() as usize > MAX_ENVIRONMENT {
        return Err(Error::Limit);
    }
    let mut environment = Vec::with_capacity(env.len() as usize);
    for value in env {
        layout!(value, wire::environment::Builder<'_>);
        environment.push(Environment {
            name: text(value.get_name())?,
            value: text(value.get_value())?,
        });
    }
    let value = Intent {
        operation_id: id_text(root.get_operation_id())?,
        artifact_sha256: hash32(root.get_artifact_sha256())?,
        program: text(root.get_program())?,
        argv: args.iter().map(text).collect::<Result<Vec<_>>>()?,
        cwd: text(root.get_cwd())?,
        env: environment,
        input: data(root.get_input(), MAX_BODY_BYTES)?,
        execution_domain: id_text(root.get_execution_domain())?,
        max_runtime_ms: root.get_max_runtime_ms(),
    };
    value.validate()?;
    Ok(value)
}
fn set_facts(mut out: wire::execution_facts::Builder<'_>, facts: &ExecutionFacts) {
    out.set_has_exit_code(facts.exit_code.is_some());
    out.set_exit_code(facts.exit_code.unwrap_or(0));
    out.set_output_closed(facts.output_closed);
    out.set_stdout_sha256(&facts.stdout_sha256);
    out.set_stderr_sha256(&facts.stderr_sha256);
    out.set_stdout_bytes(facts.stdout_bytes);
    out.set_stderr_bytes(facts.stderr_bytes);
}
fn get_facts(root: wire::execution_facts::Reader<'_>) -> Result<ExecutionFacts> {
    layout!(root, wire::execution_facts::Builder<'_>);
    let value = ExecutionFacts {
        exit_code: root.get_has_exit_code().then_some(root.get_exit_code()),
        output_closed: root.get_output_closed(),
        stdout_sha256: hash32(root.get_stdout_sha256())?,
        stderr_sha256: hash32(root.get_stderr_sha256())?,
        stdout_bytes: root.get_stdout_bytes(),
        stderr_bytes: root.get_stderr_bytes(),
    };
    value.validate()?;
    Ok(value)
}
fn set_session(mut out: wire::session_info::Builder<'_>, info: &SessionInfo) {
    out.set_session_id(info.session_id.as_str());
    out.set_has_parent(info.parent.is_some());
    out.set_parent(info.parent.as_deref().unwrap_or(""));
    out.set_parent_tail(info.parent_tail);
    out.set_epoch(info.epoch);
    out.set_tail(info.tail);
    out.set_floor(info.floor);
    out.set_archived(info.archived);
    out.set_revision(info.revision);
    out.set_checkpoint_tail(info.checkpoint_tail);
    out.set_checkpoint_sha256(&info.checkpoint_sha256);
    out.set_checkpoint_sealed(info.checkpoint_sealed);
    out.set_parent_checkpoint_sha256(
        info.parent_checkpoint_sha256
            .as_ref()
            .map_or(&[][..], |h| h.as_slice()),
    );
}
fn get_session(root: wire::session_info::Reader<'_>) -> Result<SessionInfo> {
    layout!(root, wire::session_info::Builder<'_>);
    let value = SessionInfo {
        session_id: id_text(root.get_session_id())?,
        parent: if root.get_has_parent() {
            Some(id_text(root.get_parent())?)
        } else {
            None
        },
        parent_tail: root.get_parent_tail(),
        epoch: root.get_epoch(),
        tail: root.get_tail(),
        floor: root.get_floor(),
        archived: root.get_archived(),
        revision: root.get_revision(),
        checkpoint_tail: root.get_checkpoint_tail(),
        checkpoint_sha256: hash32(root.get_checkpoint_sha256())?,
        checkpoint_sealed: root.get_checkpoint_sealed(),
        parent_checkpoint_sha256: if root.get_has_parent() {
            Some(hash32(root.get_parent_checkpoint_sha256())?)
        } else {
            None
        },
    };
    value.validate()?;
    Ok(value)
}
fn phase_to_wire(value: ToolPhase) -> wire::ToolPhase {
    match value {
        ToolPhase::Proposed => wire::ToolPhase::Proposed,
        ToolPhase::Approved => wire::ToolPhase::Approved,
        ToolPhase::Revoked => wire::ToolPhase::Revoked,
        ToolPhase::DispatchUnknown => wire::ToolPhase::DispatchUnknown,
        ToolPhase::Reported => wire::ToolPhase::Reported,
    }
}
fn phase_from_wire(value: wire::ToolPhase) -> ToolPhase {
    match value {
        wire::ToolPhase::Proposed => ToolPhase::Proposed,
        wire::ToolPhase::Approved => ToolPhase::Approved,
        wire::ToolPhase::Revoked => ToolPhase::Revoked,
        wire::ToolPhase::DispatchUnknown => ToolPhase::DispatchUnknown,
        wire::ToolPhase::Reported => ToolPhase::Reported,
    }
}
fn set_tool(mut out: wire::tool_info::Builder<'_>, info: &ToolInfo) {
    out.set_operation_id(info.operation_id.as_str());
    out.set_session_id(info.session_id.as_str());
    out.set_intent_sha256(&info.intent_sha256);
    out.set_phase(phase_to_wire(info.phase));
    out.set_has_facts(info.facts.is_some());
    if let Some(facts) = &info.facts {
        set_facts(out.init_facts(), facts);
    }
}
fn get_tool(root: wire::tool_info::Reader<'_>) -> Result<ToolInfo> {
    layout!(root, wire::tool_info::Builder<'_>);
    let value = ToolInfo {
        operation_id: id_text(root.get_operation_id())?,
        session_id: id_text(root.get_session_id())?,
        intent_sha256: hash32(root.get_intent_sha256())?,
        phase: phase_from_wire(root.get_phase().map_err(invalid)?),
        facts: if root.get_has_facts() {
            Some(get_facts(root.get_facts().map_err(invalid)?)?)
        } else {
            None
        },
    };
    value.validate()?;
    Ok(value)
}
fn set_snapshot(mut out: wire::session_snapshot::Builder<'_>, snapshot: &SessionSnapshot) {
    out.set_gap(snapshot.gap);
    out.set_checkpoint(&snapshot.checkpoint);
    set_session(out.reborrow().init_info(), &snapshot.info);
    let mut events = out.init_events(snapshot.events.len() as u32);
    for (i, event) in snapshot.events.iter().enumerate() {
        let mut item = events.reborrow().get(i as u32);
        item.set_sequence(event.sequence);
        set_event(item.init_event(), &event.event);
    }
}
fn get_snapshot(root: wire::session_snapshot::Reader<'_>) -> Result<SessionSnapshot> {
    layout!(root, wire::session_snapshot::Builder<'_>);
    let events = root.get_events().map_err(invalid)?;
    if events.len() as usize > MAX_EVENTS {
        return Err(Error::Limit);
    }
    let mut stored = Vec::with_capacity(events.len() as usize);
    for item in events {
        layout!(item, wire::stored_event::Builder<'_>);
        stored.push(StoredEvent {
            sequence: item.get_sequence(),
            event: get_event(item.get_event().map_err(invalid)?)?,
        });
    }
    let value = SessionSnapshot {
        info: get_session(root.get_info().map_err(invalid)?)?,
        events: stored,
        gap: root.get_gap(),
        checkpoint: data(root.get_checkpoint(), MAX_BODY_BYTES)?,
    };
    value.validate()?;
    Ok(value)
}
fn failure_to_wire(value: Error) -> wire::Failure {
    match value {
        Error::Invalid => wire::Failure::Invalid,
        Error::Contract => wire::Failure::Contract,
        Error::Limit => wire::Failure::Limit,
        Error::Correlation => wire::Failure::Correlation,
        Error::Denied => wire::Failure::Denied,
        Error::Conflict => wire::Failure::Conflict,
        Error::NotFound => wire::Failure::NotFound,
        Error::CommitUnknown => wire::Failure::CommitUnknown,
        Error::Storage => wire::Failure::Storage,
    }
}
fn failure_from_wire(value: wire::Failure) -> Error {
    match value {
        wire::Failure::Invalid => Error::Invalid,
        wire::Failure::Contract => Error::Contract,
        wire::Failure::Limit => Error::Limit,
        wire::Failure::Correlation => Error::Correlation,
        wire::Failure::Denied => Error::Denied,
        wire::Failure::Conflict => Error::Conflict,
        wire::Failure::NotFound => Error::NotFound,
        wire::Failure::CommitUnknown => Error::CommitUnknown,
        wire::Failure::Storage => Error::Storage,
    }
}

impl Request {
    pub fn new(request_id: impl Into<String>, action: Action) -> Result<Self> {
        let request_id = request_id.into();
        identity(&request_id)?;
        action.validate()?;
        let mut message = Builder::new_default();
        {
            let mut root = message.init_root::<wire::request::Builder<'_>>();
            root.set_version(VERSION);
            root.set_revision(REVISION);
            root.set_schema_sha256(&SCHEMA_DIGEST);
            root.set_runtime_digest(&runtime::runtime_digest());
            root.set_content_digest(&runtime::content_digest());
            root.set_core_runtime_version(runtime::PROTOCOL_VERSION);
            root.set_request_id(request_id.as_str());
            match &action {
                Action::Create {
                    session_id,
                    parent,
                    parent_tail,
                } => {
                    let mut out = root.init_create();
                    out.set_session_id(session_id.as_str());
                    out.set_has_parent(parent.is_some());
                    out.set_parent(parent.as_deref().unwrap_or(""));
                    out.set_parent_tail(*parent_tail);
                }
                Action::List => root.set_list(()),
                Action::Snapshot {
                    session_id,
                    after,
                    limit,
                } => {
                    let mut out = root.init_snapshot();
                    out.set_session_id(session_id.as_str());
                    out.set_after(*after);
                    out.set_limit(*limit);
                }
                Action::OpenWriter {
                    session_id,
                    expected_epoch,
                } => {
                    let mut out = root.init_open_writer();
                    out.set_session_id(session_id.as_str());
                    out.set_expected_epoch(*expected_epoch);
                }
                Action::Append {
                    session_id,
                    epoch,
                    expected_tail,
                    events,
                } => {
                    let mut out = root.init_append();
                    out.set_session_id(session_id.as_str());
                    out.set_epoch(*epoch);
                    out.set_expected_tail(*expected_tail);
                    let mut list = out.init_events(events.len() as u32);
                    for (i, event) in events.iter().enumerate() {
                        set_event(list.reborrow().get(i as u32), event);
                    }
                }
                Action::Checkpoint {
                    session_id,
                    epoch,
                    expected_tail,
                    state,
                } => {
                    let mut out = root.init_checkpoint();
                    out.set_session_id(session_id.as_str());
                    out.set_epoch(*epoch);
                    out.set_expected_tail(*expected_tail);
                    out.set_state(state);
                }
                Action::Archive {
                    session_id,
                    epoch,
                    expected_tail,
                } => {
                    let mut out = root.init_archive();
                    out.set_session_id(session_id.as_str());
                    out.set_epoch(*epoch);
                    out.set_expected_tail(*expected_tail);
                }
                Action::Propose { session_id, intent } => {
                    let mut out = root.init_propose();
                    out.set_session_id(session_id.as_str());
                    set_intent(out.init_intent(), intent);
                }
                Action::Claim {
                    operation_id,
                    permit,
                } => {
                    let mut out = root.init_claim();
                    out.set_operation_id(operation_id.as_str());
                    out.set_permit(permit);
                }
                Action::Report {
                    operation_id,
                    claim,
                    facts,
                } => {
                    let mut out = root.init_report();
                    out.set_operation_id(operation_id.as_str());
                    out.set_claim(claim);
                    set_facts(out.init_facts(), facts);
                }
                Action::Inspect { operation_id } => {
                    root.init_inspect().set_operation_id(operation_id.as_str());
                }
            }
        }
        let bytes = finish(&message)?;
        let digest = hash(&bytes);
        Ok(Self {
            request_id,
            action,
            bytes,
            digest,
        })
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::request::Reader<'_>>()
            .map_err(invalid)?;
        layout!(root, wire::request::Builder<'_>);
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        contract(
            root.get_version(),
            root.get_revision(),
            root.get_schema_sha256().map_err(invalid)?,
            root.get_core_runtime_version(),
            root.get_runtime_digest().map_err(invalid)?,
            root.get_content_digest().map_err(invalid)?,
        )?;
        let request_id = id_text(root.get_request_id())?;
        let action = match root.which().map_err(invalid)? {
            wire::request::Create(value) => {
                let root = value.map_err(invalid)?;
                layout!(root, wire::create::Builder<'_>);
                Action::Create {
                    session_id: id_text(root.get_session_id())?,
                    parent: if root.get_has_parent() {
                        Some(id_text(root.get_parent())?)
                    } else {
                        None
                    },
                    parent_tail: root.get_parent_tail(),
                }
            }
            wire::request::List(()) => Action::List,
            wire::request::Snapshot(value) => {
                let root = value.map_err(invalid)?;
                layout!(root, wire::snapshot::Builder<'_>);
                Action::Snapshot {
                    session_id: id_text(root.get_session_id())?,
                    after: root.get_after(),
                    limit: root.get_limit(),
                }
            }
            wire::request::OpenWriter(value) => {
                let root = value.map_err(invalid)?;
                layout!(root, wire::open_writer::Builder<'_>);
                Action::OpenWriter {
                    session_id: id_text(root.get_session_id())?,
                    expected_epoch: root.get_expected_epoch(),
                }
            }
            wire::request::Append(value) => {
                let root = value.map_err(invalid)?;
                layout!(root, wire::append::Builder<'_>);
                let events = root.get_events().map_err(invalid)?;
                if events.len() as usize > MAX_EVENTS {
                    return Err(Error::Limit);
                }
                Action::Append {
                    session_id: id_text(root.get_session_id())?,
                    epoch: root.get_epoch(),
                    expected_tail: root.get_expected_tail(),
                    events: events.iter().map(get_event).collect::<Result<Vec<_>>>()?,
                }
            }
            wire::request::Checkpoint(value) => {
                let root = value.map_err(invalid)?;
                layout!(root, wire::checkpoint::Builder<'_>);
                Action::Checkpoint {
                    session_id: id_text(root.get_session_id())?,
                    epoch: root.get_epoch(),
                    expected_tail: root.get_expected_tail(),
                    state: data(root.get_state(), MAX_BODY_BYTES)?,
                }
            }
            wire::request::Archive(value) => {
                let root = value.map_err(invalid)?;
                layout!(root, wire::archive::Builder<'_>);
                Action::Archive {
                    session_id: id_text(root.get_session_id())?,
                    epoch: root.get_epoch(),
                    expected_tail: root.get_expected_tail(),
                }
            }
            wire::request::Propose(value) => {
                let root = value.map_err(invalid)?;
                layout!(root, wire::propose::Builder<'_>);
                Action::Propose {
                    session_id: id_text(root.get_session_id())?,
                    intent: get_intent(root.get_intent().map_err(invalid)?)?,
                }
            }
            wire::request::Claim(value) => {
                let root = value.map_err(invalid)?;
                layout!(root, wire::claim::Builder<'_>);
                Action::Claim {
                    operation_id: id_text(root.get_operation_id())?,
                    permit: hash32(root.get_permit())?,
                }
            }
            wire::request::Report(value) => {
                let root = value.map_err(invalid)?;
                layout!(root, wire::report::Builder<'_>);
                Action::Report {
                    operation_id: id_text(root.get_operation_id())?,
                    claim: hash32(root.get_claim())?,
                    facts: get_facts(root.get_facts().map_err(invalid)?)?,
                }
            }
            wire::request::Inspect(value) => {
                let root = value.map_err(invalid)?;
                layout!(root, wire::inspect::Builder<'_>);
                Action::Inspect {
                    operation_id: id_text(root.get_operation_id())?,
                }
            }
        };
        let request = Self::new(request_id, action)?;
        if request.bytes != bytes {
            return Err(Error::Contract);
        }
        Ok(request)
    }
    pub fn id(&self) -> &str {
        &self.request_id
    }
    pub fn raw(&self) -> &[u8] {
        &self.bytes
    }
    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn action(&self) -> &Action {
        &self.action
    }
}

impl Outcome {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Session(info) => info.validate(),
            Self::Sessions(infos) => {
                if infos.len() > MAX_SESSIONS {
                    return Err(Error::Limit);
                }
                let mut ids = std::collections::BTreeSet::new();
                for info in infos {
                    info.validate()?;
                    if !ids.insert(&info.session_id) {
                        return Err(Error::Invalid);
                    }
                }
                Ok(())
            }
            Self::Snapshot(snapshot) => snapshot.validate(),
            Self::Tool(info) => info.validate(),
            Self::Claimed { info, intent, .. } => {
                info.validate()?;
                intent.validate()?;
                if info.phase != ToolPhase::DispatchUnknown
                    || info.operation_id != intent.operation_id
                    || info.intent_sha256 != intent.digest()?
                {
                    return Err(Error::Correlation);
                }
                Ok(())
            }
            Self::Rejected(_) => Ok(()),
        }
    }
}
impl Reply {
    pub fn new(request: &Request, outcome: Outcome) -> Result<Self> {
        let reply = Self {
            request_id: request.request_id.clone(),
            request_sha256: request.digest,
            outcome,
        };
        reply.validate_for(request)?;
        Ok(reply)
    }
    pub fn validate_for(&self, request: &Request) -> Result<()> {
        identity(&self.request_id)?;
        self.outcome.validate()?;
        if self.request_id != request.request_id || self.request_sha256 != request.digest {
            return Err(Error::Correlation);
        }
        let accepted = match (&request.action, &self.outcome) {
            (_, Outcome::Rejected(_)) => true,
            (Action::List, Outcome::Sessions(_)) => true,
            (
                Action::Create {
                    session_id,
                    parent,
                    parent_tail,
                },
                Outcome::Session(info),
            ) => {
                info.session_id == *session_id
                    && info.parent == *parent
                    && info.parent_tail == *parent_tail
                    && info.epoch == 0
                    && info.tail == 0
                    && info.floor == 1
                    && !info.archived
                    && info.checkpoint_tail == 0
                    && if parent.is_some() {
                        info.checkpoint_sealed
                            && Some(info.checkpoint_sha256) == info.parent_checkpoint_sha256
                    } else {
                        !info.checkpoint_sealed && info.checkpoint_sha256 == hash(&[])
                    }
            }
            (
                Action::OpenWriter {
                    session_id,
                    expected_epoch,
                },
                Outcome::Session(info),
            ) => {
                info.session_id == *session_id && info.epoch == expected_epoch + 1 && !info.archived
            }
            (
                Action::Append {
                    session_id,
                    epoch,
                    expected_tail,
                    events,
                },
                Outcome::Session(info),
            ) => {
                info.session_id == *session_id
                    && info.epoch == *epoch
                    && info.tail == expected_tail + events.len() as u64
                    && !info.archived
            }
            (
                Action::Checkpoint {
                    session_id,
                    epoch,
                    expected_tail,
                    state,
                },
                Outcome::Session(info),
            ) => {
                info.session_id == *session_id
                    && info.epoch == *epoch
                    && info.tail == *expected_tail
                    && info.checkpoint_tail == *expected_tail
                    && info.checkpoint_sha256 == hash(state)
                    && info.checkpoint_sealed
                    && !info.archived
            }
            (
                Action::Archive {
                    session_id,
                    epoch,
                    expected_tail,
                },
                Outcome::Session(info),
            ) => {
                info.session_id == *session_id
                    && info.epoch == *epoch
                    && info.tail == *expected_tail
                    && info.archived
            }
            (
                Action::Snapshot {
                    session_id,
                    after,
                    limit,
                },
                Outcome::Snapshot(snapshot),
            ) => {
                let start = (after + 1).max(snapshot.info.floor);
                let available = snapshot
                    .info
                    .tail
                    .saturating_sub(start)
                    .saturating_add(u64::from(start <= snapshot.info.tail));
                snapshot.info.session_id == *session_id
                    && snapshot.gap == (after + 1 < snapshot.info.floor)
                    && snapshot.events.len() as u64 == available.min(u64::from(*limit))
                    && snapshot
                        .events
                        .first()
                        .is_none_or(|event| event.sequence == start)
            }
            (Action::Propose { session_id, intent }, Outcome::Tool(info)) => {
                info.operation_id == intent.operation_id
                    && info.session_id == *session_id
                    && info.intent_sha256 == intent.digest()?
            }
            (Action::Claim { operation_id, .. }, Outcome::Claimed { info, .. }) => {
                info.operation_id == *operation_id
            }
            (
                Action::Report {
                    operation_id,
                    facts,
                    ..
                },
                Outcome::Tool(info),
            ) => {
                info.operation_id == *operation_id
                    && info.phase == ToolPhase::Reported
                    && info.facts.as_ref() == Some(facts)
            }
            (Action::Inspect { operation_id }, Outcome::Tool(info)) => {
                info.operation_id == *operation_id
            }
            _ => false,
        };
        if !accepted {
            return Err(Error::Correlation);
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        identity(&self.request_id)?;
        self.outcome.validate()?;
        let mut message = Builder::new_default();
        {
            let mut root = message.init_root::<wire::reply::Builder<'_>>();
            root.set_version(VERSION);
            root.set_revision(REVISION);
            root.set_schema_sha256(&SCHEMA_DIGEST);
            root.set_runtime_digest(&runtime::runtime_digest());
            root.set_content_digest(&runtime::content_digest());
            root.set_core_runtime_version(runtime::PROTOCOL_VERSION);
            root.set_request_id(self.request_id.as_str());
            root.set_request_sha256(&self.request_sha256);
            match &self.outcome {
                Outcome::Session(info) => set_session(root.init_session(), info),
                Outcome::Sessions(infos) => {
                    let mut list = root.init_sessions(infos.len() as u32);
                    for (i, info) in infos.iter().enumerate() {
                        set_session(list.reborrow().get(i as u32), info);
                    }
                }
                Outcome::Snapshot(snapshot) => set_snapshot(root.init_snapshot(), snapshot),
                Outcome::Tool(info) => set_tool(root.init_tool(), info),
                Outcome::Claimed {
                    info,
                    intent,
                    claim,
                } => {
                    let mut out = root.init_claimed();
                    out.set_claim(claim);
                    set_tool(out.reborrow().init_info(), info);
                    set_intent(out.init_intent(), intent);
                }
                Outcome::Rejected(failure) => root.set_rejected(failure_to_wire(*failure)),
            }
        }
        finish(&message)
    }
    pub fn decode_for(request: &Request, bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::reply::Reader<'_>>()
            .map_err(invalid)?;
        layout!(root, wire::reply::Builder<'_>);
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        contract(
            root.get_version(),
            root.get_revision(),
            root.get_schema_sha256().map_err(invalid)?,
            root.get_core_runtime_version(),
            root.get_runtime_digest().map_err(invalid)?,
            root.get_content_digest().map_err(invalid)?,
        )?;
        let outcome = match root.which().map_err(invalid)? {
            wire::reply::Session(value) => Outcome::Session(get_session(value.map_err(invalid)?)?),
            wire::reply::Sessions(value) => {
                let infos = value.map_err(invalid)?;
                if infos.len() as usize > MAX_SESSIONS {
                    return Err(Error::Limit);
                }
                Outcome::Sessions(infos.iter().map(get_session).collect::<Result<Vec<_>>>()?)
            }
            wire::reply::Snapshot(value) => {
                Outcome::Snapshot(get_snapshot(value.map_err(invalid)?)?)
            }
            wire::reply::Tool(value) => Outcome::Tool(get_tool(value.map_err(invalid)?)?),
            wire::reply::Claimed(value) => {
                let root = value.map_err(invalid)?;
                layout!(root, wire::claimed::Builder<'_>);
                Outcome::Claimed {
                    info: get_tool(root.get_info().map_err(invalid)?)?,
                    intent: get_intent(root.get_intent().map_err(invalid)?)?,
                    claim: hash32(root.get_claim())?,
                }
            }
            wire::reply::Rejected(value) => {
                Outcome::Rejected(failure_from_wire(value.map_err(invalid)?))
            }
        };
        let reply = Self {
            request_id: id_text(root.get_request_id())?,
            request_sha256: hash32(root.get_request_sha256())?,
            outcome,
        };
        reply.validate_for(request)?;
        if reply.encode()? != bytes {
            return Err(Error::Contract);
        }
        Ok(reply)
    }
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Event")
            .field("event_id", &self.event_id)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}
impl std::fmt::Debug for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Environment")
            .field("name", &self.name)
            .field("value_bytes", &self.value.len())
            .finish()
    }
}
impl std::fmt::Debug for Intent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Intent")
            .field("operation_id", &self.operation_id)
            .field("execution_domain", &self.execution_domain)
            .field("argument_count", &self.argv.len())
            .field("environment_count", &self.env.len())
            .field("input_bytes", &self.input.len())
            .field("max_runtime_ms", &self.max_runtime_ms)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create { session_id, .. } => f
                .debug_struct("Create")
                .field("session_id", session_id)
                .finish_non_exhaustive(),
            Self::List => f.write_str("List"),
            Self::Snapshot {
                session_id,
                after,
                limit,
            } => f
                .debug_struct("Snapshot")
                .field("session_id", session_id)
                .field("after", after)
                .field("limit", limit)
                .finish(),
            Self::OpenWriter {
                session_id,
                expected_epoch,
            } => f
                .debug_struct("OpenWriter")
                .field("session_id", session_id)
                .field("expected_epoch", expected_epoch)
                .finish(),
            Self::Append {
                session_id, events, ..
            } => f
                .debug_struct("Append")
                .field("session_id", session_id)
                .field("event_count", &events.len())
                .finish_non_exhaustive(),
            Self::Checkpoint {
                session_id, state, ..
            } => f
                .debug_struct("Checkpoint")
                .field("session_id", session_id)
                .field("state_bytes", &state.len())
                .finish_non_exhaustive(),
            Self::Archive { session_id, .. } => f
                .debug_struct("Archive")
                .field("session_id", session_id)
                .finish_non_exhaustive(),
            Self::Propose { session_id, intent } => f
                .debug_struct("Propose")
                .field("session_id", session_id)
                .field("intent", intent)
                .finish(),
            Self::Claim { operation_id, .. } => f
                .debug_struct("Claim")
                .field("operation_id", operation_id)
                .finish_non_exhaustive(),
            Self::Report { operation_id, .. } => f
                .debug_struct("Report")
                .field("operation_id", operation_id)
                .finish_non_exhaustive(),
            Self::Inspect { operation_id } => f
                .debug_struct("Inspect")
                .field("operation_id", operation_id)
                .finish(),
        }
    }
}
impl std::fmt::Debug for SessionSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionSnapshot")
            .field("info", &self.info)
            .field("event_count", &self.events.len())
            .field("gap", &self.gap)
            .field("checkpoint_bytes", &self.checkpoint.len())
            .finish()
    }
}
impl std::fmt::Debug for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Session(info) => f.debug_tuple("Session").field(info).finish(),
            Self::Sessions(infos) => f
                .debug_struct("Sessions")
                .field("count", &infos.len())
                .finish(),
            Self::Snapshot(snapshot) => std::fmt::Debug::fmt(snapshot, f),
            Self::Tool(info) => f.debug_tuple("Tool").field(info).finish(),
            Self::Claimed { info, intent, .. } => f
                .debug_struct("Claimed")
                .field("info", info)
                .field("intent", intent)
                .finish_non_exhaustive(),
            Self::Rejected(error) => f.debug_tuple("Rejected").field(error).finish(),
        }
    }
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("request_id", &self.request_id)
            .field("action", &self.action)
            .field("frame_bytes", &self.bytes.len())
            .finish()
    }
}
impl std::fmt::Debug for Reply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reply")
            .field("request_id", &self.request_id)
            .field("outcome", &self.outcome)
            .finish_non_exhaustive()
    }
}
