//! Read-only, bounded observation of one original StateSlot. No task adoption,
//! grant, execution claim, join, cleanup, owner replacement or session attach.
use crate::{agent_task_discovery_capnp as wire, io_tasks::TaskKey};
use capnp::{message::{Builder, ReaderOptions}, serialize};
use sha2::{Digest, Sha256};

pub const MAGIC: &[u8; 8] = b"MROWAD31";
pub const VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 16 * 1024;
pub fn digest() -> [u8; 32] {
    Sha256::digest(include_bytes!("../schemas/agent_task_discovery.capnp")).into()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error { Invalid, Contract, Limit }
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "private Agent observation {self:?}")
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;
fn parsed<T>(value: capnp::Result<T>) -> Result<T> { value.map_err(|_| Error::Contract) }
fn fixed(value: capnp::Result<&[u8]>) -> Result<[u8; 32]> {
    parsed(value)?.try_into().map_err(|_| Error::Invalid)
}
fn reader(input: &[u8]) -> Result<capnp::message::Reader<serialize::BufferSegments<&[u8]>>> {
    if input.len() > MAX_FRAME_BYTES { return Err(Error::Limit); }
    if input.len() < 16 || !input.starts_with(MAGIC) { return Err(Error::Contract); }
    let mut rest = &input[8..];
    let value = parsed(serialize::read_message_from_flat_slice(&mut rest, ReaderOptions {
        traversal_limit_in_words: Some(MAX_FRAME_BYTES / 8 * 4), nesting_limit: 8,
    }))?;
    if !rest.is_empty() { return Err(Error::Contract); }
    Ok(value)
}
fn frame(message: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let mut output = MAGIC.to_vec();
    output.extend(serialize::write_message_to_words(message));
    if output.len() > MAX_FRAME_BYTES { return Err(Error::Limit); }
    Ok(output)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    None,
    /// Exact AgentWorker remains retained. It may already be stopping/finished;
    /// this pure observation never joins it or proves OS/native execution.
    RunningAgent,
    /// Same-key Agent exit/failure record retained, not permission to restart.
    RetainedAgentExit,
    OtherTask,
    Unavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation { pub kind: Kind, pub task: Option<TaskKey> }
impl Observation {
    fn without_key(kind: Kind) -> Self { Self { kind, task: None } }
    fn validate(self) -> Result<()> {
        let keyed = matches!(self.kind, Kind::RunningAgent | Kind::RetainedAgentExit);
        if keyed != self.task.is_some() || self.task.is_some_and(|key| key.as_bytes() == &[0; 32]) {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

/// Immutable facts sampled only by StateSlot. No worker operation is invoked.
/// The bools mean retained Executor::Agent and same-key retained Agent exit.
pub(crate) fn classify(
    unavailable: bool, owner_present: bool, task: Option<(TaskKey, bool, bool)>,
) -> Observation {
    if unavailable { return Observation::without_key(Kind::Unavailable); }
    let Some((key, retained_worker, retained_exit)) = task else {
        return Observation::without_key(if owner_present { Kind::None } else { Kind::Unavailable });
    };
    let conflicting_owner = if owner_present { retained_worker } else { retained_exit };
    if key.as_bytes() == &[0; 32] || conflicting_owner {
        return Observation::without_key(Kind::Unavailable);
    }
    if retained_worker { return Observation { kind: Kind::RunningAgent, task: Some(key) }; }
    if retained_exit { return Observation { kind: Kind::RetainedAgentExit, task: Some(key) }; }
    Observation::without_key(Kind::OtherTask)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request { pub id: [u8; 32] }
impl Request {
    pub fn encode(self) -> Result<Vec<u8>> {
        if self.id == [0; 32] { return Err(Error::Invalid); }
        let mut message = Builder::new_default();
        let mut r = message.init_root::<wire::request::Builder<'_>>();
        r.set_version(VERSION); r.set_digest(&digest()); r.set_id(&self.id);
        frame(&message)
    }
    pub fn decode(input: &[u8]) -> Result<Self> {
        let message = reader(input)?;
        let r = parsed(message.get_root::<wire::request::Reader<'_>>())?;
        if r.get_version() != VERSION || fixed(r.get_digest())? != digest() { return Err(Error::Contract); }
        let request = Self { id: fixed(r.get_id())? };
        // One fixed shape: reject hidden fields, alternate/trailing encodings.
        if request.encode()? != input { return Err(Error::Contract); }
        Ok(request)
    }
}
fn wire_kind(kind: Kind) -> wire::Kind {
    match kind {
        Kind::None => wire::Kind::None, Kind::RunningAgent => wire::Kind::RunningAgent,
        Kind::RetainedAgentExit => wire::Kind::RetainedAgentExit,
        Kind::OtherTask => wire::Kind::OtherTask, Kind::Unavailable => wire::Kind::Unavailable,
    }
}
fn read_kind(kind: std::result::Result<wire::Kind, capnp::NotInSchema>) -> Result<Kind> {
    Ok(match kind.map_err(|_| Error::Contract)? {
        wire::Kind::None => Kind::None, wire::Kind::RunningAgent => Kind::RunningAgent,
        wire::Kind::RetainedAgentExit => Kind::RetainedAgentExit,
        wire::Kind::OtherTask => Kind::OtherTask, wire::Kind::Unavailable => Kind::Unavailable,
    })
}
impl Observation {
    pub fn encode_for(self, request: Request, input: &[u8]) -> Result<Vec<u8>> {
        if Request::decode(input)? != request { return Err(Error::Contract); }
        self.validate()?;
        let mut message = Builder::new_default();
        let mut r = message.init_root::<wire::reply::Builder<'_>>();
        r.set_version(VERSION); r.set_digest(&digest()); r.set_id(&request.id);
        r.set_request_sha256(&Sha256::digest(input)); r.set_kind(wire_kind(self.kind));
        r.set_task(self.task.as_ref().map_or(&[][..], |key| &key.as_bytes()[..]));
        frame(&message)
    }
    pub fn decode_for(request: Request, input: &[u8], response: &[u8]) -> Result<Self> {
        if Request::decode(input)? != request { return Err(Error::Contract); }
        let message = reader(response)?;
        let r = parsed(message.get_root::<wire::reply::Reader<'_>>())?;
        if r.get_version() != VERSION || fixed(r.get_digest())? != digest()
            || fixed(r.get_id())? != request.id || fixed(r.get_request_sha256())? != <[u8; 32]>::from(Sha256::digest(input)) {
            return Err(Error::Contract);
        }
        let raw = parsed(r.get_task())?;
        let task = if raw.is_empty() { None } else { Some(TaskKey::from_bytes(raw).map_err(|_| Error::Invalid)?) };
        let observation = Self { kind: read_kind(r.get_kind())?, task };
        observation.validate()?;
        if observation.encode_for(request, input)? != response { return Err(Error::Contract); }
        Ok(observation)
    }
}
impl crate::Workbench {
    /// Only immutable original slot inspection. No Store access or task action.
    pub fn observe_agent_task(&self) -> Observation { self.state.observe_agent_task() }
}
pub fn respond(host: &crate::Workbench, input: &[u8]) -> crate::Result<Vec<u8>> {
    let gate = host.product_gate(); gate.check()?;
    let request = Request::decode(input)?;
    let response = host.observe_agent_task().encode_for(request, input)?;
    gate.check()?;
    Ok(response)
}
/// Read-only observation delivery does not mutate PS30 Unknown or its controller.
/// Errors close the existing caller's ordinary transport; no retry or owner replacement.
pub fn deliver_native_reply(
    output: &mut impl std::io::Write, response: &[u8], gate: &crate::product_gate::ProductGate,
) -> crate::Result<()> {
    if response.len() > MAX_FRAME_BYTES { return Err(Error::Limit.into()); }
    gate.check()?;
    output.write_all(&(response.len() as u32).to_le_bytes())?;
    gate.check()?; output.write_all(response)?; gate.check()?;
    output.flush()?; gate.check()?;
    Ok(())
}

#[cfg(test)]
#[path = "agent_task_discovery_tests.rs"]
mod tests;
