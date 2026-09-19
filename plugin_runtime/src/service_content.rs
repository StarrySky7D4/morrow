//! Host-issued, exact content scopes for one registered service and principal.
//! Scope digests identify request semantics; they neither grant authority nor
//! restore revoked, replaced, expired or disconnected content permissions.
use crate::{
    io_binding::{self, Error},
    manager::ManagedInstance,
    service_io::ServiceGrant,
};
use morrow_core::{
    dispatch::HostRuntime,
    lifecycle::{ContentAuthorization, GrantKind},
    runtime::Command,
    service::Request,
};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub const MAX_CONTENT_SCOPES: usize = 128;
/// Reserved host metadata. The transport strips caller-supplied occurrences and
/// injects exactly one lowercase hexadecimal digest of the effective scopes.
pub const SCOPE_HEADER: &str = "morrow-content-scope";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentScope {
    pub kind: GrantKind,
    pub card_id: String,
    /// Required only for ReadAttachment; there are no attachment wildcards.
    pub attachment_id: Option<String>,
}
fn kind_tag(kind: GrantKind) -> u8 {
    match kind {
        GrantKind::Rename => 1,
        GrantKind::ReadSummary => 2,
        GrantKind::QueryOperation => 3,
        GrantKind::ReadAttachment => 4,
        GrantKind::CreateContent => 5,
        GrantKind::EditContent => 6,
        GrantKind::ReadContent => 7,
    }
}
fn compare(a: &ContentScope, b: &ContentScope) -> std::cmp::Ordering {
    (kind_tag(a.kind), &a.card_id, &a.attachment_id).cmp(&(
        kind_tag(b.kind),
        &b.card_id,
        &b.attachment_id,
    ))
}
fn identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
}
fn validate_scopes(scopes: &[ContentScope]) -> io_binding::Result<()> {
    if scopes.len() > MAX_CONTENT_SCOPES {
        return Err(Error::Limit);
    }
    for scope in scopes {
        if !identity(&scope.card_id)
            || match (scope.kind, scope.attachment_id.as_deref()) {
                (GrantKind::ReadAttachment, Some(id)) => !identity(id),
                (GrantKind::ReadAttachment, None) | (_, Some(_)) => true,
                (_, None) => false,
            }
        {
            return Err(Error::Denied);
        }
    }
    Ok(())
}
fn canonical(mut scopes: Vec<ContentScope>) -> io_binding::Result<Vec<ContentScope>> {
    validate_scopes(&scopes)?;
    scopes.sort_by(compare);
    if scopes.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(Error::Denied);
    }
    Ok(scopes)
}
fn core_error(error: morrow_core::Error) -> Error {
    match error {
        morrow_core::Error::Limit => Error::Limit,
        _ => Error::Denied,
    }
}
fn digest(scopes: &[ContentScope]) -> [u8; 32] {
    fn text(hash: &mut Sha256, value: &str) {
        hash.update((value.len() as u32).to_le_bytes());
        hash.update(value.as_bytes());
    }
    let mut hash = Sha256::new();
    hash.update(b"Morrow/service-content-scopes/v1\0");
    hash.update((scopes.len() as u32).to_le_bytes());
    for scope in scopes {
        hash.update([kind_tag(scope.kind)]);
        text(&mut hash, &scope.card_id);
        match &scope.attachment_id {
            Some(attachment) => {
                hash.update([1]);
                text(&mut hash, attachment);
            }
            None => hash.update([0]),
        }
    }
    hash.finalize().into()
}
/// Compute only the stable scope identity, without granting access or touching
/// a host. Input ordering is normalized; duplicates and malformed scopes fail.
pub fn scope_digest(scopes: &[ContentScope]) -> io_binding::Result<[u8; 32]> {
    validate_scopes(scopes)?;
    Ok(digest(&canonical(scopes.to_vec())?))
}
fn hex(bytes: [u8; 32]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(64);
    for byte in bytes {
        write!(&mut out, "{byte:02x}").expect("String formatting");
    }
    out
}
struct PolicyState {
    grant: ServiceGrant,
    scopes: Vec<ContentScope>,
    probes: Vec<ContentAuthorization>,
    revoked: AtomicBool,
}
/// Fixed service maximum backed by probes of the original content grants.
/// Cloning shares revocation and cannot renew or replace any original grant.
#[derive(Clone)]
pub struct ServiceContentPolicy {
    state: Arc<PolicyState>,
}
impl ServiceContentPolicy {
    pub fn issue(
        host: &HostRuntime,
        instance: &ManagedInstance,
        grant: &ServiceGrant,
        scopes: Vec<ContentScope>,
        now: u64,
    ) -> io_binding::Result<Self> {
        let scopes = canonical(scopes)?;
        grant.validate_instance(host, instance)?;
        let mut probes = Vec::with_capacity(scopes.len());
        for scope in &scopes {
            probes.push(
                host.content_authorization(
                    instance.connection(),
                    scope.kind,
                    &scope.card_id,
                    scope.attachment_id.as_deref(),
                    now,
                )
                .map_err(core_error)?,
            );
        }
        grant.check(now)?;
        for probe in &probes {
            probe.check(now).map_err(core_error)?;
        }
        Ok(Self {
            state: Arc::new(PolicyState {
                grant: grant.clone(),
                scopes,
                probes,
                revoked: AtomicBool::new(false),
            }),
        })
    }
    pub fn revoke(&self) {
        self.state.revoked.store(true, Ordering::Release);
    }
    /// Compare actual issuance identity, not matching package IDs or scope text.
    /// Binding the same service resource to its listener does not replace it.
    pub fn validate_grant(&self, grant: &ServiceGrant) -> io_binding::Result<()> {
        if self.state.revoked.load(Ordering::Acquire) || !self.state.grant.same_service(grant) {
            return Err(Error::Denied);
        }
        Ok(())
    }
    /// The trusted transport supplies the authenticated principal and a current
    /// principal-scope callback. Requested scopes must be an exact subset; excess
    /// requests are rejected rather than silently intersected or broadened.
    pub fn authorize(
        &self,
        principal: &str,
        scopes: Vec<ContentScope>,
        live: impl Fn() -> bool + Send + Sync + 'static,
    ) -> io_binding::Result<ServiceContentAccess> {
        if !identity(principal) || self.state.revoked.load(Ordering::Acquire) {
            return Err(Error::Denied);
        }
        let scopes = canonical(scopes)?;
        let indices = scopes
            .iter()
            .map(|scope| {
                self.state
                    .scopes
                    .binary_search_by(|value| compare(value, scope))
                    .map_err(|_| Error::Denied)
            })
            .collect::<io_binding::Result<Vec<_>>>()?;
        let access = ServiceContentAccess {
            policy: self.clone(),
            principal: principal.into(),
            digest: digest(&scopes),
            scopes,
            indices,
            live: Arc::new(live),
        };
        access.check()?;
        Ok(access)
    }
}
/// One effective principal scope. It retains the exact original authorization
/// probes through queueing, execution, historical replay and final delivery.
#[derive(Clone)]
pub struct ServiceContentAccess {
    policy: ServiceContentPolicy,
    principal: String,
    scopes: Vec<ContentScope>,
    indices: Vec<usize>,
    digest: [u8; 32],
    live: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl ServiceContentAccess {
    pub fn scope_digest(&self) -> [u8; 32] {
        self.digest
    }
    /// Without sampling time, check the host's live principal and policy flag.
    /// Time-sensitive consumers must also call check_at with the host clock.
    pub fn check(&self) -> io_binding::Result<()> {
        if self.policy.state.revoked.load(Ordering::Acquire)
            || !(self.live)()
            || self.policy.state.revoked.load(Ordering::Acquire)
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub fn check_at(&self, now: u64) -> io_binding::Result<()> {
        self.check()?;
        self.policy.state.grant.check(now)?;
        for &index in &self.indices {
            self.policy.state.probes[index]
                .check(now)
                .map_err(core_error)?;
        }
        self.check()
    }
    pub(crate) fn validate_binding(
        &self,
        binding: &crate::io_binding::IoBinding,
    ) -> io_binding::Result<()> {
        self.policy.state.grant.validate_binding(binding)?;
        self.check()
    }
    pub fn validate_grant(&self, grant: &ServiceGrant) -> io_binding::Result<()> {
        self.policy.validate_grant(grant)?;
        self.check()
    }
    pub fn validate_request(&self, request: &Request) -> io_binding::Result<()> {
        let invocation = request.invocation();
        if invocation.principal != self.principal
            || invocation.service != self.policy.state.grant.service()
            || invocation.handler != self.policy.state.grant.handler()
        {
            return Err(Error::Denied);
        }
        let mut headers = invocation
            .headers
            .iter()
            .filter(|header| header.name.eq_ignore_ascii_case(SCOPE_HEADER));
        let header = headers.next().ok_or(Error::Denied)?;
        if headers.next().is_some() || header.value != hex(self.digest).as_bytes() {
            return Err(Error::Denied);
        }
        self.check()
    }
    /// Validate the concrete command and exact object/attachment scope before
    /// touching the host clock. No scope grants another command kind implicitly.
    pub fn check_command(&self, command: &Command, now: u64) -> io_binding::Result<()> {
        command.validate().map_err(core_error)?;
        let kind = match command {
            Command::CreateContent(_) => GrantKind::CreateContent,
            Command::EditContent(_) => GrantKind::EditContent,
            Command::ReadContent(_) => GrantKind::ReadContent,
            Command::Rename(_) => GrantKind::Rename,
            Command::ReadAttachment(_) => GrantKind::ReadAttachment,
            Command::ReadSummary { .. } => GrantKind::ReadSummary,
            Command::QueryOperation { .. } => GrantKind::QueryOperation,
        };
        let scope = ContentScope {
            kind,
            card_id: command.card_id().into(),
            attachment_id: match command {
                Command::ReadAttachment(value) => Some(value.attachment_id.clone()),
                _ => None,
            },
        };
        if self
            .scopes
            .binary_search_by(|value| compare(value, &scope))
            .is_err()
        {
            return Err(Error::Denied);
        }
        self.check_at(now)
    }
}
