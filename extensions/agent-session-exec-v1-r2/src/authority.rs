//! Explicit host admission for this profile. Data never reconstructs live authority.
use crate::{Error, Result, hash, identity};
use morrow_core::{
    agent_ledger::Record,
    dispatch::{Connection, ConnectionBinding, HostBinding, HostRuntime},
    lifecycle::InstancePhase,
};
use prost::Message;
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub session_read: bool,
    pub session_write: bool,
    pub propose: bool,
    pub execute: bool,
    pub retire: bool,
}
impl Capabilities {
    fn subset(self, other: Self) -> bool {
        (!self.session_read || other.session_read)
            && (!self.session_write || other.session_write)
            && (!self.propose || other.propose)
            && (!self.execute || other.execute)
            && (!self.retire || other.retire)
    }
    fn permits(self, right: Right) -> bool {
        match right {
            Right::SessionRead => self.session_read,
            Right::SessionWrite => self.session_write,
            Right::Propose => self.propose,
            Right::Execute => self.execute,
            Right::Retire => self.retire,
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Right {
    SessionRead,
    SessionWrite,
    Propose,
    Execute,
    Retire,
}

struct Live {
    nonce: [u8; 32],
    revoked: AtomicBool,
    last_clock: AtomicU64,
}
/// Opaque, revocable original connection admission. Never serialized or restored.
#[derive(Clone)]
pub struct Admission {
    issuer: Arc<AuthorityIdentity>,
    connection: ConnectionBinding,
    approved: Capabilities,
    sessions: BTreeSet<String>,
    domain: String,
    created: u64,
    expires: u64,
    generation: u64,
    live: Arc<Live>,
}
impl Admission {
    pub(crate) fn nonce(&self) -> [u8; 32] {
        self.live.nonce
    }
    pub(crate) fn execution_domain(&self) -> &str {
        &self.domain
    }
    pub(crate) fn expires(&self) -> u64 {
        self.expires
    }
    pub(crate) fn sessions(&self) -> &BTreeSet<String> {
        &self.sessions
    }
}
struct AuthorityIdentity {
    host: HostBinding,
    nonce: [u8; 32],
    lease: morrow_core::store::AgentLedgerOwnerLease,
    generation: u64,
}
pub(crate) struct Authority {
    identity: Arc<AuthorityIdentity>,
    admissions: Mutex<Vec<Admission>>,
    fence: Mutex<()>,
    last_clock: AtomicU64,
}
/// Adapter for the existing HostRuntime and its single Store; creates neither.
pub struct SessionExecHost {
    pub(crate) authority: Authority,
}
impl SessionExecHost {
    pub fn new(runtime: &mut HostRuntime) -> Result<Self> {
        let lease = runtime
            .store_local_mut()
            .pin_agent_ledger_owner()
            .map_err(core_error)?;
        let generation = initialize_metadata(runtime, &lease)?;
        Ok(Self {
            authority: Authority {
                identity: Arc::new(AuthorityIdentity {
                    host: runtime.binding(),
                    nonce: random()?,
                    lease,
                    generation,
                }),
                admissions: Mutex::new(Vec::new()),
                fence: Mutex::new(()),
                last_clock: AtomicU64::new(0),
            },
        })
    }
    /// Trusted import control plane: declarations are ceilings, approval is a subset.
    /// The host must obtain the declaration from its reviewed package/import metadata.
    /// No wire request can call this method, select its scope, or approve itself.
    #[allow(clippy::too_many_arguments)]
    pub fn admit(
        &self,
        runtime: &HostRuntime,
        connection: &Connection,
        declared: Capabilities,
        approved: Capabilities,
        sessions: Vec<String>,
        execution_domain: String,
        expires: u64,
        now: u64,
    ) -> Result<Admission> {
        let _fence = self.execution_fence()?;
        self.bound(runtime, connection)?;
        self.advance_clock(now)?;
        if !approved.subset(declared) {
            return Err(Error::Denied);
        }
        if sessions.is_empty() || sessions.len() > 16 || expires <= now || expires - now > 3_600_000
        {
            return Err(Error::Limit);
        }
        identity(&execution_domain)?;
        let mut scope = BTreeSet::new();
        for session in sessions {
            identity(&session)?;
            if !scope.insert(session) {
                return Err(Error::Invalid);
            }
        }
        let admission = Admission {
            issuer: self.authority.identity.clone(),
            connection: connection.binding(),
            approved,
            sessions: scope,
            domain: execution_domain,
            created: now,
            expires,
            generation: self.authority.identity.generation,
            live: Arc::new(Live {
                nonce: random()?,
                revoked: AtomicBool::new(false),
                last_clock: AtomicU64::new(now),
            }),
        };
        let mut admissions = self
            .authority
            .admissions
            .lock()
            .map_err(|_| Error::Storage)?;
        admissions.retain(|admission| {
            let dead = admission.live.revoked.load(Ordering::SeqCst)
                || now >= admission.expires
                || runtime.binding_phase(admission.connection).ok() != Some(InstancePhase::Ready);
            if dead {
                admission.live.revoked.store(true, Ordering::SeqCst);
            }
            !dead
        });
        if admissions.len() >= 128 {
            return Err(Error::Limit);
        }
        admissions.push(admission.clone());
        Ok(admission)
    }
    pub fn revoke(&self, admission: &Admission) -> Result<()> {
        let _fence = self.execution_fence()?;
        if !Arc::ptr_eq(&self.authority.identity, &admission.issuer) {
            return Err(Error::Denied);
        }
        admission.live.revoked.store(true, Ordering::SeqCst);
        self.authority
            .admissions
            .lock()
            .map_err(|_| Error::Storage)?
            .retain(|candidate| !candidate.live.revoked.load(Ordering::SeqCst));
        Ok(())
    }
    pub(crate) fn issuer_nonce(&self) -> [u8; 32] {
        self.authority.identity.nonce
    }
    pub(crate) fn execution_fence(&self) -> Result<std::sync::MutexGuard<'_, ()>> {
        self.authority.fence.lock().map_err(|_| Error::Storage)
    }
    fn bound(&self, runtime: &HostRuntime, connection: &Connection) -> Result<()> {
        self.ensure_owner(runtime)?;
        if runtime
            .connection_phase(connection)
            .map_err(|_| Error::Denied)?
            != InstancePhase::Ready
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub(crate) fn check(
        &self,
        runtime: &HostRuntime,
        connection: &Connection,
        admission: &Admission,
        right: Right,
        session: Option<&str>,
        now: u64,
    ) -> Result<()> {
        self.bound(runtime, connection)?;
        self.advance_clock(now)?;
        if !Arc::ptr_eq(&self.authority.identity, &admission.issuer)
            || admission.connection != connection.binding()
            || !admission.approved.permits(right)
            || admission.generation != self.authority.identity.generation
            || admission.live.revoked.load(Ordering::SeqCst)
            || session.is_some_and(|s| !admission.sessions.contains(s))
            || now < admission.created
            || now >= admission.expires
        {
            return Err(Error::Denied);
        }
        admission
            .live
            .last_clock
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                if now >= old { Some(now) } else { None }
            })
            .map_err(|_| Error::Denied)?;
        Ok(())
    }
    pub(crate) fn check_nonce(
        &self,
        runtime: &HostRuntime,
        nonce: [u8; 32],
        right: Right,
        session: &str,
        now: u64,
    ) -> Result<()> {
        self.ensure_owner(runtime)?;
        self.advance_clock(now)?;
        let admissions = self
            .authority
            .admissions
            .lock()
            .map_err(|_| Error::Storage)?;
        let admission = admissions
            .iter()
            .find(|a| a.nonce() == nonce)
            .ok_or(Error::Denied)?;
        if runtime
            .binding_phase(admission.connection)
            .map_err(|_| Error::Denied)?
            != InstancePhase::Ready
            || admission.live.revoked.load(Ordering::SeqCst)
            || !admission.approved.permits(right)
            || admission.generation != self.authority.identity.generation
            || !admission.sessions.contains(session)
            || now < admission.created
            || now >= admission.expires
        {
            return Err(Error::Denied);
        }
        admission
            .live
            .last_clock
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                if now >= old { Some(now) } else { None }
            })
            .map_err(|_| Error::Denied)?;
        Ok(())
    }
    fn advance_clock(&self, now: u64) -> Result<()> {
        self.authority
            .last_clock
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                if now >= old { Some(now) } else { None }
            })
            .map_err(|_| Error::Denied)?;
        Ok(())
    }
    pub(crate) fn ensure_owner(&self, runtime: &HostRuntime) -> Result<()> {
        runtime
            .store_local()
            .validate_agent_ledger_owner(&self.authority.identity.lease)
            .map_err(|_| Error::Denied)?;
        if runtime.binding() != self.authority.identity.host {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub(crate) fn ledger_owner(&self) -> &morrow_core::store::AgentLedgerOwnerLease {
        &self.authority.identity.lease
    }
    pub fn generation(&self, runtime: &HostRuntime) -> Result<u64> {
        self.ensure_owner(runtime)?;
        Ok(self.authority.identity.generation)
    }
    pub(crate) fn check_generation(&self, runtime: &HostRuntime, generation: u64) -> Result<()> {
        if self.generation(runtime)? != generation {
            return Err(Error::Conflict);
        }
        Ok(())
    }
    pub(crate) fn publish_profile_record(
        &self,
        runtime: &mut HostRuntime,
        record: &Record,
        expected: u64,
    ) -> Result<()> {
        self.ensure_owner(runtime)?;
        let kind = business_kind(&record.domain())?;
        let (metadata_record, mut metadata) = load_metadata(runtime)?;
        self.check_generation(runtime, metadata.generation)?;
        let position = metadata
            .identities
            .iter()
            .position(|v| v.kind == kind && v.id == record.id());
        if expected == 0 {
            if position.is_some() {
                return Err(Error::Conflict);
            }
            if metadata.identities.len() >= MAX_PROFILE_IDENTITIES {
                return Err(Error::Limit);
            }
            metadata.identities.push(ProfileIdentity {
                kind,
                id: record.id().into(),
                retired: false,
                digest: hash(record.container()).to_vec(),
                phase: 0,
                revision: record.revision(),
            });
            let next = metadata_record
                .revision()
                .checked_add(1)
                .ok_or(Error::Limit)?;
            let updated = Record::new(
                metadata_domain(),
                METADATA_ID,
                next,
                metadata.encode_reserved()?,
            )
            .map_err(core_error)?;
            runtime
                .store_local_mut()
                .batch_agent_ledger_local(
                    self.ledger_owner(),
                    &[
                        morrow_core::store::AgentLedgerMutation::Put {
                            record: updated,
                            expected_revision: metadata_record.revision(),
                        },
                        morrow_core::store::AgentLedgerMutation::Put {
                            record: record.clone(),
                            expected_revision: expected,
                        },
                    ],
                )
                .map_err(core_error)
        } else {
            if position.is_none_or(|position| metadata.identities[position].retired) {
                return Err(Error::Conflict);
            }
            let item = &mut metadata.identities[position.ok_or(Error::Conflict)?];
            if item.revision != expected {
                return Err(Error::Conflict);
            }
            item.digest = hash(record.container()).to_vec();
            item.revision = record.revision();
            let updated = Record::new(
                metadata_domain(),
                METADATA_ID,
                metadata_record
                    .revision()
                    .checked_add(1)
                    .ok_or(Error::Limit)?,
                metadata.encode_reserved()?,
            )
            .map_err(core_error)?;
            runtime
                .store_local_mut()
                .batch_agent_ledger_local(
                    self.ledger_owner(),
                    &[
                        morrow_core::store::AgentLedgerMutation::Put {
                            record: updated,
                            expected_revision: metadata_record.revision(),
                        },
                        morrow_core::store::AgentLedgerMutation::Put {
                            record: record.clone(),
                            expected_revision: expected,
                        },
                    ],
                )
                .map_err(core_error)
        }
    }
    pub(crate) fn retire_profile_record(
        &self,
        runtime: &mut HostRuntime,
        expected: &Record,
    ) -> Result<()> {
        if business_kind(&expected.domain())? != 1 {
            return Err(Error::Denied);
        }
        self.retire_profile_record_with_phase(runtime, expected, 1)
    }
    pub(crate) fn retire_profile_record_with_phase(
        &self,
        runtime: &mut HostRuntime,
        expected: &Record,
        historical_phase: u32,
    ) -> Result<()> {
        self.ensure_owner(runtime)?;
        let kind = business_kind(&expected.domain())?;
        if (kind == 1 && historical_phase != 1) || (kind == 2 && !matches!(historical_phase, 2..=4))
        {
            return Err(Error::Invalid);
        }
        let (metadata_record, mut metadata) = load_metadata(runtime)?;
        self.check_generation(runtime, metadata.generation)?;
        let identity = metadata
            .identities
            .iter_mut()
            .find(|v| v.kind == kind && v.id == expected.id())
            .ok_or(Error::NotFound)?;
        if identity.retired
            || identity.revision != expected.revision()
            || identity.digest.as_slice() != hash(expected.container())
        {
            return Err(Error::Conflict);
        }
        identity.retired = true;
        identity.digest = hash(expected.container()).to_vec();
        identity.phase = historical_phase;
        let updated = Record::new(
            metadata_domain(),
            METADATA_ID,
            metadata_record
                .revision()
                .checked_add(1)
                .ok_or(Error::Limit)?,
            metadata.encode_reserved()?,
        )
        .map_err(core_error)?;
        runtime
            .store_local_mut()
            .batch_agent_ledger_local(
                self.ledger_owner(),
                &[
                    morrow_core::store::AgentLedgerMutation::Put {
                        record: updated,
                        expected_revision: metadata_record.revision(),
                    },
                    morrow_core::store::AgentLedgerMutation::Delete {
                        expected: expected.clone(),
                    },
                ],
            )
            .map_err(core_error)
    }
    pub(crate) fn retired_identity(
        &self,
        runtime: &HostRuntime,
        domain: &[u8; 32],
        id: &str,
    ) -> Result<Option<[u8; 32]>> {
        Ok(self
            .retired_identity_summary(runtime, domain, id)?
            .map(|(digest, _)| digest))
    }
    pub(crate) fn retired_identity_summary(
        &self,
        runtime: &HostRuntime,
        domain: &[u8; 32],
        id: &str,
    ) -> Result<Option<([u8; 32], u32)>> {
        self.ensure_owner(runtime)?;
        identity(id)?;
        let kind = business_kind(domain)?;
        let (_, metadata) = load_metadata(runtime)?;
        self.check_generation(runtime, metadata.generation)?;
        metadata
            .identities
            .iter()
            .find(|v| v.kind == kind && v.id == id && v.retired)
            .map(|v| {
                v.digest
                    .as_slice()
                    .try_into()
                    .map(|digest| (digest, v.phase))
                    .map_err(|_| Error::Storage)
            })
            .transpose()
    }
    /// Finish an empty generation. Old owners and admissions are invalidated;
    /// construct a new host and obtain fresh trusted admissions for the result.
    pub fn rollover_generation(&self, runtime: &mut HostRuntime, now: u64) -> Result<u64> {
        let _fence = self.execution_fence()?;
        self.ensure_owner(runtime)?;
        self.advance_clock(now)?;
        let (metadata_record, mut metadata) = load_metadata(runtime)?;
        self.check_generation(runtime, metadata.generation)?;
        if metadata.identities.iter().any(|v| !v.retired) {
            return Err(Error::Conflict);
        }
        for domain in [session_domain(), tool_domain()] {
            if !runtime
                .store_local()
                .agent_ledger_ids_local(&domain)
                .map_err(core_error)?
                .is_empty()
            {
                return Err(Error::Conflict);
            }
        }
        metadata.generation = metadata.generation.checked_add(1).ok_or(Error::Limit)?;
        metadata.identities.clear();
        let updated = Record::new(
            metadata_domain(),
            METADATA_ID,
            metadata_record
                .revision()
                .checked_add(1)
                .ok_or(Error::Limit)?,
            metadata.encode_reserved()?,
        )
        .map_err(core_error)?;
        let result = runtime
            .store_local_mut()
            .batch_agent_ledger_local(
                self.ledger_owner(),
                &[morrow_core::store::AgentLedgerMutation::Put {
                    record: updated,
                    expected_revision: metadata_record.revision(),
                }],
            )
            .map_err(core_error);
        if result.is_ok() || result == Err(Error::CommitUnknown) {
            // Closing the owner is mandatory even if admission bookkeeping is
            // poisoned: a committed or uncertain generation cannot stay live.
            let invalidated = runtime
                .store_local()
                .invalidate_agent_ledger_owner(self.ledger_owner());
            for admission in self
                .authority
                .admissions
                .lock()
                .map_err(|_| Error::CommitUnknown)?
                .iter()
            {
                admission.live.revoked.store(true, Ordering::SeqCst);
            }
            if invalidated.is_err() {
                return Err(Error::CommitUnknown);
            }
        }
        result?;
        Ok(metadata.generation)
    }
}

pub const MAX_PROFILE_IDENTITIES: usize = 127;
pub const PROFILE_METADATA_BYTES: usize = 64 * 1024;
const METADATA_ID: &str = "profile";
fn metadata_domain() -> [u8; 32] {
    hash(b"morrow/agent-session-exec-v1/profile/metadata/2")
}
fn session_domain() -> [u8; 32] {
    hash(b"morrow/agent-session-exec-v1/session/state/2")
}
fn tool_domain() -> [u8; 32] {
    hash(b"agent-session-exec-v1/tool/2")
}
fn business_kind(domain: &[u8; 32]) -> Result<u32> {
    if *domain == session_domain() {
        Ok(1)
    } else if *domain == tool_domain() {
        Ok(2)
    } else {
        Err(Error::Denied)
    }
}
#[derive(Clone, PartialEq, Message)]
struct ProfileMetadata {
    #[prost(uint32, tag = "1")]
    version: u32,
    #[prost(uint64, tag = "2")]
    generation: u64,
    #[prost(message, repeated, tag = "3")]
    identities: Vec<ProfileIdentity>,
    #[prost(bytes = "vec", tag = "4")]
    padding: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct ProfileIdentity {
    #[prost(uint32, tag = "1")]
    kind: u32,
    #[prost(string, tag = "2")]
    id: String,
    #[prost(bool, tag = "3")]
    retired: bool,
    #[prost(bytes = "vec", tag = "4")]
    digest: Vec<u8>,
    #[prost(uint32, tag = "5")]
    phase: u32,
    #[prost(uint64, tag = "6")]
    revision: u64,
}
impl ProfileMetadata {
    fn validate(&self) -> Result<()> {
        if self.version != 2
            || self.generation == 0
            || self.encoded_len() != PROFILE_METADATA_BYTES
            || self.identities.len() > MAX_PROFILE_IDENTITIES
            || self.padding.iter().any(|&b| b != 0)
        {
            return Err(Error::Storage);
        }
        let mut identities = BTreeSet::new();
        for item in &self.identities {
            identity(&item.id).map_err(|_| Error::Storage)?;
            if !matches!(item.kind, 1 | 2)
                || item.digest.len() != 32
                || item.revision == 0
                || item.revision > i64::MAX as u64
                || !identities.insert((item.kind, &item.id))
            {
                return Err(Error::Storage);
            }
            if (!item.retired && item.phase != 0)
                || (item.retired
                    && ((item.kind == 1 && item.phase != 1)
                        || (item.kind == 2 && !matches!(item.phase, 2..=4))))
            {
                return Err(Error::Storage);
            }
        }
        Ok(())
    }
    fn encode_reserved(&mut self) -> Result<Vec<u8>> {
        self.padding.clear();
        for _ in 0..8 {
            let length = self.encoded_len();
            if length == PROFILE_METADATA_BYTES {
                self.validate()?;
                return Ok(self.encode_to_vec());
            }
            let padding = if length < PROFILE_METADATA_BYTES {
                self.padding
                    .len()
                    .checked_add(PROFILE_METADATA_BYTES - length)
            } else {
                self.padding
                    .len()
                    .checked_sub(length - PROFILE_METADATA_BYTES)
            }
            .ok_or(Error::Limit)?;
            self.padding.resize(padding, 0);
        }
        Err(Error::Storage)
    }
}
fn load_metadata(runtime: &HostRuntime) -> Result<(Record, ProfileMetadata)> {
    let record = runtime
        .store_local()
        .load_agent_ledger_local(&metadata_domain(), METADATA_ID)
        .map_err(core_error)?
        .ok_or(Error::NotFound)?;
    if record.payload().len() != PROFILE_METADATA_BYTES {
        return Err(Error::Storage);
    }
    let metadata = ProfileMetadata::decode(record.payload()).map_err(|_| Error::Storage)?;
    if metadata.encode_to_vec() != record.payload() {
        return Err(Error::Storage);
    }
    metadata.validate()?;
    Ok((record, metadata))
}
fn initialize_metadata(
    runtime: &mut HostRuntime,
    lease: &morrow_core::store::AgentLedgerOwnerLease,
) -> Result<u64> {
    match load_metadata(runtime) {
        Ok((_, metadata)) => {
            validate_profile_closure(runtime, &metadata)?;
            Ok(metadata.generation)
        }
        Err(Error::NotFound) => {
            for domain in [metadata_domain(), session_domain(), tool_domain()] {
                if !runtime
                    .store_local()
                    .agent_ledger_ids_local(&domain)
                    .map_err(core_error)?
                    .is_empty()
                {
                    return Err(Error::Storage);
                }
            }
            let mut metadata = ProfileMetadata {
                version: 2,
                generation: 1,
                identities: vec![],
                padding: vec![],
            };
            let record = Record::new(
                metadata_domain(),
                METADATA_ID,
                1,
                metadata.encode_reserved()?,
            )
            .map_err(core_error)?;
            runtime
                .store_local_mut()
                .batch_agent_ledger_local(
                    lease,
                    &[morrow_core::store::AgentLedgerMutation::Put {
                        record,
                        expected_revision: 0,
                    }],
                )
                .map_err(core_error)?;
            Ok(1)
        }
        Err(error) => Err(error),
    }
}

fn validate_profile_closure(runtime: &HostRuntime, metadata: &ProfileMetadata) -> Result<()> {
    if runtime
        .store_local()
        .agent_ledger_ids_local(&metadata_domain())
        .map_err(|_| Error::Storage)?
        != [METADATA_ID]
    {
        return Err(Error::Storage);
    }
    let mut actual = BTreeSet::new();
    for (kind, domain) in [(1, session_domain()), (2, tool_domain())] {
        for id in runtime
            .store_local()
            .agent_ledger_ids_local(&domain)
            .map_err(|_| Error::Storage)?
        {
            let item = metadata
                .identities
                .iter()
                .find(|v| v.kind == kind && v.id == id)
                .ok_or(Error::Storage)?;
            if item.retired || !actual.insert((kind, id.clone())) {
                return Err(Error::Storage);
            }
            let row = runtime
                .store_local()
                .load_agent_ledger_local(&domain, &id)
                .map_err(|_| Error::Storage)?
                .ok_or(Error::Storage)?;
            if row.revision() != item.revision || item.digest.as_slice() != hash(row.container()) {
                return Err(Error::Storage);
            }
            match kind {
                1 => crate::session::validate_profile_session_record(
                    runtime,
                    &row,
                    metadata.generation,
                )?,
                2 => crate::safe_exec::validate_profile_tool_record(
                    runtime,
                    &row,
                    metadata.generation,
                )?,
                _ => return Err(Error::Storage),
            }
        }
    }
    if metadata
        .identities
        .iter()
        .any(|v| !v.retired && !actual.contains(&(v.kind, v.id.clone())))
    {
        return Err(Error::Storage);
    }
    Ok(())
}

pub(crate) fn random() -> Result<[u8; 32]> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| Error::Storage)?;
    if bytes == [0; 32] {
        return Err(Error::Storage);
    }
    Ok(bytes)
}
pub(crate) fn core_error(error: morrow_core::Error) -> Error {
    match error {
        morrow_core::Error::RevisionConflict | morrow_core::Error::OperationConflict => {
            Error::Conflict
        }
        morrow_core::Error::NotFound => Error::NotFound,
        morrow_core::Error::Limit | morrow_core::Error::EventCapacity => Error::Limit,
        morrow_core::Error::CommitUnknown => Error::CommitUnknown,
        morrow_core::Error::UnsupportedVersion => Error::Contract,
        _ => Error::Storage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, HostRuntime, Connection, SessionExecHost) {
        let temp = tempfile::tempdir().unwrap();
        let store = morrow_core::store::Store::open(
            &temp.path().join("single-core.sqlite"),
            Default::default(),
        )
        .unwrap();
        let mut runtime = HostRuntime::new(store).unwrap();
        let connection = runtime.connect().unwrap();
        let host = SessionExecHost::new(&mut runtime).unwrap();
        (temp, runtime, connection, host)
    }
    fn admission(
        host: &SessionExecHost,
        runtime: &HostRuntime,
        connection: &Connection,
        expires: u64,
        now: u64,
    ) -> Result<Admission> {
        let caps = Capabilities {
            session_read: true,
            ..Capabilities::default()
        };
        host.admit(
            runtime,
            connection,
            caps,
            caps,
            vec!["session".into()],
            "domain".into(),
            expires,
            now,
        )
    }
    fn writable_fixture() -> (
        tempfile::TempDir,
        HostRuntime,
        Connection,
        SessionExecHost,
        Admission,
    ) {
        let (temp, mut runtime, connection, host) = fixture();
        let caps = Capabilities {
            session_read: true,
            session_write: true,
            propose: true,
            execute: true,
            retire: true,
        };
        let grant = host
            .admit(
                &runtime,
                &connection,
                caps,
                caps,
                vec!["session".into()],
                "test-domain".into(),
                100,
                1,
            )
            .unwrap();
        let request = crate::Request::new(
            "create",
            crate::Action::Create {
                session_id: "session".into(),
                parent: None,
                parent_tail: 0,
            },
        )
        .unwrap();
        let bytes = host
            .dispatch(&mut runtime, &connection, &grant, request.raw(), || 1)
            .unwrap();
        assert!(matches!(
            crate::Reply::decode_for(&request, &bytes).unwrap().outcome,
            crate::Outcome::Session(_)
        ));
        (temp, runtime, connection, host, grant)
    }

    #[test]
    fn trusted_host_replacement_closes_old_admissions_and_restores_only_history() {
        let (_temp, mut runtime, connection, host, grant) = writable_fixture();
        let writer = crate::Request::new(
            "writer",
            crate::Action::OpenWriter {
                session_id: "session".into(),
                expected_epoch: 0,
            },
        )
        .unwrap();
        host.dispatch(&mut runtime, &connection, &grant, writer.raw(), || 1)
            .unwrap();
        let replacement = SessionExecHost::new(&mut runtime).unwrap();
        assert_eq!(
            host.check(
                &runtime,
                &connection,
                &grant,
                Right::SessionWrite,
                Some("session"),
                1
            ),
            Err(Error::Denied)
        );
        let oldappend = crate::Request::new(
            "old-append",
            crate::Action::Append {
                session_id: "session".into(),
                epoch: 1,
                expected_tail: 0,
                events: vec![crate::Event {
                    event_id: "old".into(),
                    body: b"blocked".to_vec(),
                }],
            },
        )
        .unwrap();
        let bytes = host
            .dispatch(&mut runtime, &connection, &grant, oldappend.raw(), || 1)
            .unwrap();
        assert_eq!(
            crate::Reply::decode_for(&oldappend, &bytes)
                .unwrap()
                .outcome,
            crate::Outcome::Rejected(Error::Denied)
        );
        let reader = admission(&replacement, &runtime, &connection, 100, 1).unwrap();
        let snapshot = crate::Request::new(
            "restored-read",
            crate::Action::Snapshot {
                session_id: "session".into(),
                after: 0,
                limit: 1,
            },
        )
        .unwrap();
        let bytes = replacement
            .dispatch(&mut runtime, &connection, &reader, snapshot.raw(), || 1)
            .unwrap();
        assert!(
            matches!(crate::Reply::decode_for(&snapshot,&bytes).unwrap().outcome,crate::Outcome::Snapshot(s) if s.info.epoch==1 && s.info.tail==0)
        );
        assert_eq!(
            replacement.check(
                &runtime,
                &connection,
                &grant,
                Right::SessionRead,
                Some("session"),
                1
            ),
            Err(Error::Denied)
        );
    }

    #[test]
    fn startup_requires_exact_registry_rows_current_containers_and_generation() {
        use morrow_core::store::AgentLedgerMutation as Mutation;
        for case in 0..6 {
            let (_temp, mut runtime, connection, host, grant) = writable_fixture();
            let original = runtime
                .store_local()
                .load_agent_ledger_local(&session_domain(), "session")
                .unwrap()
                .unwrap();
            match case {
                0 => runtime
                    .store_local_mut()
                    .batch_agent_ledger_local(
                        host.ledger_owner(),
                        &[Mutation::Delete { expected: original }],
                    )
                    .unwrap(),
                1 => runtime
                    .store_local_mut()
                    .batch_agent_ledger_local(
                        host.ledger_owner(),
                        &[Mutation::Put {
                            record: Record::new(
                                session_domain(),
                                "unregistered",
                                1,
                                b"unregistered".to_vec(),
                            )
                            .unwrap(),
                            expected_revision: 0,
                        }],
                    )
                    .unwrap(),
                2 => {
                    host.retire_profile_record(&mut runtime, &original).unwrap();
                    runtime
                        .store_local_mut()
                        .batch_agent_ledger_local(
                            host.ledger_owner(),
                            &[Mutation::Put {
                                record: original,
                                expected_revision: 0,
                            }],
                        )
                        .unwrap();
                }
                3 => runtime
                    .store_local_mut()
                    .batch_agent_ledger_local(
                        host.ledger_owner(),
                        &[Mutation::Put {
                            record: Record::new(metadata_domain(), "extra", 1, b"extra".to_vec())
                                .unwrap(),
                            expected_revision: 0,
                        }],
                    )
                    .unwrap(),
                4 => {
                    let writer = crate::Request::new(
                        "writer",
                        crate::Action::OpenWriter {
                            session_id: "session".into(),
                            expected_epoch: 0,
                        },
                    )
                    .unwrap();
                    host.dispatch(&mut runtime, &connection, &grant, writer.raw(), || 1)
                        .unwrap();
                    let current = runtime
                        .store_local()
                        .load_agent_ledger_local(&session_domain(), "session")
                        .unwrap()
                        .unwrap();
                    runtime
                        .store_local_mut()
                        .batch_agent_ledger_local(
                            host.ledger_owner(),
                            &[Mutation::Delete { expected: current }],
                        )
                        .unwrap();
                    runtime
                        .store_local_mut()
                        .batch_agent_ledger_local(
                            host.ledger_owner(),
                            &[Mutation::Put {
                                record: original,
                                expected_revision: 0,
                            }],
                        )
                        .unwrap();
                }
                5 => {
                    let (record, mut metadata) = load_metadata(&runtime).unwrap();
                    metadata.generation = 2;
                    let replacement = Record::new(
                        metadata_domain(),
                        METADATA_ID,
                        record.revision() + 1,
                        metadata.encode_reserved().unwrap(),
                    )
                    .unwrap();
                    runtime
                        .store_local_mut()
                        .batch_agent_ledger_local(
                            host.ledger_owner(),
                            &[Mutation::Put {
                                record: replacement,
                                expected_revision: record.revision(),
                            }],
                        )
                        .unwrap();
                }
                _ => unreachable!(),
            }
            assert_eq!(
                SessionExecHost::new(&mut runtime).err(),
                Some(Error::Storage),
                "startup corruption case {case}"
            );
            assert!(host.ensure_owner(&runtime).is_err());
        }
    }
    #[test]
    fn revoked_admission_churn_recovers_capacity_and_never_revives_old_arcs() {
        let (_temp, runtime, connection, host) = fixture();
        let first = admission(&host, &runtime, &connection, 10, 0).unwrap();
        host.revoke(&first).unwrap();
        for now in 1..300 {
            let grant = admission(&host, &runtime, &connection, now + 10, now).unwrap();
            host.revoke(&grant).unwrap();
        }
        assert!(host.authority.admissions.lock().unwrap().is_empty());
        assert_eq!(
            host.check(
                &runtime,
                &connection,
                &first,
                Right::SessionRead,
                Some("session"),
                300
            ),
            Err(Error::Denied)
        );
        assert_eq!(
            admission(&host, &runtime, &connection, 20, 1).err(),
            Some(Error::Denied)
        );
    }
    #[test]
    fn expiry_and_disconnect_permanently_revoke_collected_admissions() {
        let (_temp, mut runtime, connection, host) = fixture();
        let grants: Vec<_> = (0..128)
            .map(|_| admission(&host, &runtime, &connection, 10, 0).unwrap())
            .collect();
        assert_eq!(
            admission(&host, &runtime, &connection, 20, 9).err(),
            Some(Error::Limit)
        );
        let fresh = admission(&host, &runtime, &connection, 20, 10).unwrap();
        assert!(
            grants
                .iter()
                .all(|grant| grant.live.revoked.load(Ordering::SeqCst))
        );
        host.check(
            &runtime,
            &connection,
            &fresh,
            Right::SessionRead,
            Some("session"),
            11,
        )
        .unwrap();
        assert_eq!(
            host.check(
                &runtime,
                &connection,
                &grants[0],
                Right::SessionRead,
                Some("session"),
                1
            ),
            Err(Error::Denied)
        );
        let other = runtime.connect().unwrap();
        let disconnected = admission(&host, &runtime, &other, 30, 11).unwrap();
        runtime.disconnect(&other).unwrap();
        admission(&host, &runtime, &connection, 30, 12).unwrap();
        assert!(disconnected.live.revoked.load(Ordering::SeqCst));
    }
    #[test]
    fn unrelated_configuration_revoke_does_not_kill_the_profile_owner() {
        let (_temp, runtime, connection, host) = fixture();
        let grant = admission(&host, &runtime, &connection, 100, 0).unwrap();
        runtime
            .store_local()
            .service_authority_control()
            .revoke_resource(
                &morrow_core::store::ServiceAuthorityResource::Configuration("unrelated".into()),
            );
        host.check(
            &runtime,
            &connection,
            &grant,
            Right::SessionRead,
            Some("session"),
            1,
        )
        .unwrap();
        runtime
            .store_local()
            .service_authority_control()
            .revoke_all();
        assert_eq!(
            host.check(
                &runtime,
                &connection,
                &grant,
                Right::SessionRead,
                Some("session"),
                2
            ),
            Err(Error::Denied)
        );
    }
    #[test]
    fn every_reserved_identity_can_retire_and_rollover_never_restores_old_owners() {
        let (_temp, mut runtime, connection, host) = fixture();
        let old = admission(&host, &runtime, &connection, 1000, 0).unwrap();
        let records: Vec<_> = (0..MAX_PROFILE_IDENTITIES)
            .map(|index| {
                Record::new(
                    tool_domain(),
                    format!("tool-{index}"),
                    1,
                    format!("private-body-{index}").into_bytes(),
                )
                .unwrap()
            })
            .collect();
        for record in &records {
            host.publish_profile_record(&mut runtime, record, 0)
                .unwrap();
        }
        let extra = Record::new(tool_domain(), "extra", 1, b"extra".to_vec()).unwrap();
        assert_eq!(
            host.publish_profile_record(&mut runtime, &extra, 0),
            Err(Error::Limit)
        );
        assert_eq!(
            host.rollover_generation(&mut runtime, 1),
            Err(Error::Conflict)
        );
        for record in &records {
            host.retire_profile_record_with_phase(&mut runtime, record, 3)
                .unwrap();
        }
        let (_, metadata) = load_metadata(&runtime).unwrap();
        assert_eq!(metadata.identities.len(), MAX_PROFILE_IDENTITIES);
        assert!(
            metadata
                .identities
                .iter()
                .all(|identity| identity.retired && identity.phase == 3)
        );
        assert!(
            runtime
                .store_local()
                .agent_ledger_ids_local(&tool_domain())
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            host.publish_profile_record(&mut runtime, &records[0], 0),
            Err(Error::Conflict)
        );
        assert_eq!(host.rollover_generation(&mut runtime, 2).unwrap(), 2);
        assert!(host.ensure_owner(&runtime).is_err());
        assert_eq!(
            host.check(
                &runtime,
                &connection,
                &old,
                Right::SessionRead,
                Some("session"),
                3
            ),
            Err(Error::Denied)
        );
        let fresh = SessionExecHost::new(&mut runtime).unwrap();
        assert_eq!(fresh.generation(&runtime).unwrap(), 2);
        assert_eq!(fresh.check_generation(&runtime, 1), Err(Error::Conflict));
        fresh
            .publish_profile_record(&mut runtime, &records[0], 0)
            .unwrap();
        runtime.store_local().integrity_check().unwrap();
    }
}
