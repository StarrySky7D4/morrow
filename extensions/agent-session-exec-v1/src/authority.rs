//! Explicit host admission for this profile. Data never reconstructs live authority.
use crate::{Error, Result, identity};
use morrow_core::{
    dispatch::{Connection, ConnectionBinding, HostBinding, HostRuntime},
    lifecycle::InstancePhase,
};
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
}
impl Capabilities {
    fn subset(self, other: Self) -> bool {
        (!self.session_read || other.session_read)
            && (!self.session_write || other.session_write)
            && (!self.propose || other.propose)
            && (!self.execute || other.execute)
    }
    fn permits(self, right: Right) -> bool {
        match right {
            Right::SessionRead => self.session_read,
            Right::SessionWrite => self.session_write,
            Right::Propose => self.propose,
            Right::Execute => self.execute,
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Right {
    SessionRead,
    SessionWrite,
    Propose,
    Execute,
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
    lease: morrow_core::store::ServiceAuthorityLease,
}
pub(crate) struct Authority {
    identity: Arc<AuthorityIdentity>,
    admissions: Mutex<Vec<Admission>>,
    fence: Mutex<()>,
}
/// Adapter for the existing HostRuntime and its single Store; creates neither.
pub struct SessionExecHost {
    pub(crate) authority: Authority,
}
impl SessionExecHost {
    pub fn new(runtime: &mut HostRuntime) -> Result<Self> {
        let lease = runtime
            .store_local_mut()
            .pin_service_authority()
            .map_err(core_error)?;
        Ok(Self {
            authority: Authority {
                identity: Arc::new(AuthorityIdentity {
                    host: runtime.binding(),
                    nonce: random()?,
                    lease,
                }),
                admissions: Mutex::new(Vec::new()),
                fence: Mutex::new(()),
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
        self.bound(runtime, connection)?;
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
        Ok(())
    }
    pub(crate) fn issuer_nonce(&self) -> [u8; 32] {
        self.authority.identity.nonce
    }
    pub(crate) fn execution_fence(&self) -> Result<std::sync::MutexGuard<'_, ()>> {
        self.authority.fence.lock().map_err(|_| Error::Storage)
    }
    fn bound(&self, runtime: &HostRuntime, connection: &Connection) -> Result<()> {
        runtime
            .store_local()
            .validate_service_authority(&self.authority.identity.lease)
            .map_err(|_| Error::Denied)?;
        if runtime.binding() != self.authority.identity.host
            || runtime
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
        if !Arc::ptr_eq(&self.authority.identity, &admission.issuer)
            || admission.connection != connection.binding()
            || !admission.approved.permits(right)
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
        runtime
            .store_local()
            .validate_service_authority(&self.authority.identity.lease)
            .map_err(|_| Error::Denied)?;
        if runtime.binding() != self.authority.identity.host {
            return Err(Error::Denied);
        }
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
