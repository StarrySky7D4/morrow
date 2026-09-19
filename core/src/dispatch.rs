//! Trusted connection binding. No instance, grant, clock, or storage path is accepted on wire.
use crate::{
    Error, Result,
    lifecycle::{Grant, GrantKind, HostPolicy, Instance},
    response::{Outcome, Response, failure},
    runtime::{Command, MAX_MESSAGE_BYTES},
    store::Store,
};
use std::collections::{BTreeMap, BTreeSet};
/// Opaque in-process identity for trusted adapters. No guest-selected identity or numeric handle.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ConnectionBinding(Instance);
impl std::fmt::Debug for ConnectionBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConnectionBinding(..)")
    }
}
/// Process-local host identity for trusted resource brokers, never a wire capability.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct HostBinding(u64);
impl std::fmt::Debug for HostBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostBinding(..)")
    }
}
/// Opaque host-owned endpoint; never serialize or let an untrusted caller select another endpoint.
pub struct Connection {
    instance: Instance,
    package_digest: Option<[u8; 32]>,
    ceiling: Option<BTreeSet<GrantKind>>,
    grants: BTreeMap<(GrantKind, String, Option<String>), Grant>,
}
impl Connection {
    pub fn binding(&self) -> ConnectionBinding {
        ConnectionBinding(self.instance)
    }
    pub fn package_digest(&self) -> Option<[u8; 32]> {
        self.package_digest
    }
    fn permits_kind(&self, kind: GrantKind) -> Result<()> {
        if self.ceiling.as_ref().is_some_and(|v| !v.contains(&kind)) {
            return Err(Error::Invalid("undeclared package capability"));
        }
        Ok(())
    }
}
pub struct HostRuntime {
    policy: HostPolicy,
    store: Store,
}
impl HostRuntime {
    pub fn binding(&self) -> HostBinding {
        HostBinding(self.policy.identity())
    }
    pub fn new(store: Store) -> Result<Self> {
        Ok(Self {
            policy: HostPolicy::new()?,
            store,
        })
    }
    pub fn connect(&mut self) -> Result<Connection> {
        let instance = self.policy.activate()?;
        self.policy.ready(instance)?;
        Ok(Connection {
            instance,
            package_digest: None,
            ceiling: None,
            grants: BTreeMap::new(),
        })
    }
    /// Bind a fresh instance to immutable package identity and declared capability ceiling.
    /// Package validity is not author trust; caller prepares/approves code before connecting.
    pub fn connect_package(
        &mut self,
        package: &crate::plugin_package::Package,
    ) -> Result<Connection> {
        let mut connection = self.connect()?;
        connection.package_digest = Some(package.digest());
        connection.ceiling = Some(package.capabilities().clone());
        Ok(connection)
    }
    /// Bind the explicit approved subset to the actual connection, before any grant exists.
    /// Declarations remain ceilings; this trusted entry does not create object authorization.
    pub fn connect_package_approved(
        &mut self,
        package: &crate::plugin_package::Package,
        approved: &BTreeSet<GrantKind>,
    ) -> Result<Connection> {
        if !approved.is_subset(package.capabilities()) {
            return Err(Error::Invalid("approval exceeds declaration"));
        }
        let mut connection = self.connect()?;
        connection.package_digest = Some(package.digest());
        connection.ceiling = Some(approved.clone());
        Ok(connection)
    }
    /// Administrative control plane, never a dispatch command.
    pub fn grant(
        &mut self,
        connection: &mut Connection,
        kind: GrantKind,
        card: &str,
        expires: u64,
        now: u64,
    ) -> Result<()> {
        connection.permits_kind(kind)?;
        self.policy.phase(connection.instance)?;
        let key = (kind, card.to_owned(), None);
        if let Some(old) = connection.grants.remove(&key) {
            self.policy.revoke(old)?;
        }
        let grant = self
            .policy
            .grant(connection.instance, kind, card, expires, now)?;
        connection.grants.insert(key, grant);
        Ok(())
    }
    pub fn revoke(
        &mut self,
        connection: &mut Connection,
        kind: GrantKind,
        card: &str,
    ) -> Result<()> {
        self.policy.phase(connection.instance)?;
        let grant = connection
            .grants
            .remove(&(kind, card.into(), None))
            .ok_or(Error::Invalid("missing grant"))?;
        self.policy.revoke(grant)
    }
    pub fn grant_attachment(
        &mut self,
        connection: &mut Connection,
        card: &str,
        attachment: &str,
        expires: u64,
        now: u64,
    ) -> Result<()> {
        connection.permits_kind(GrantKind::ReadAttachment)?;
        self.policy.phase(connection.instance)?;
        let key = (
            GrantKind::ReadAttachment,
            card.into(),
            Some(attachment.into()),
        );
        if let Some(old) = connection.grants.remove(&key) {
            self.policy.revoke(old)?;
        }
        let grant =
            self.policy
                .grant_attachment(connection.instance, card, attachment, expires, now)?;
        connection.grants.insert(key, grant);
        Ok(())
    }
    pub fn revoke_attachment(
        &mut self,
        connection: &mut Connection,
        card: &str,
        attachment: &str,
    ) -> Result<()> {
        self.policy.phase(connection.instance)?;
        let grant = connection
            .grants
            .remove(&(
                GrantKind::ReadAttachment,
                card.into(),
                Some(attachment.into()),
            ))
            .ok_or(Error::Invalid("missing grant"))?;
        self.policy.revoke(grant)
    }
    pub fn disconnect(&mut self, connection: &Connection) -> Result<()> {
        self.policy.safety_stop(connection.instance)?;
        self.policy.stop(connection.instance)?;
        self.policy.retire(connection.instance)
    }
    /// Trusted one-way stop signal; commit/read boundaries observe it without sharing the Store.
    pub fn revocation(&self, connection: &Connection) -> Result<crate::lifecycle::Revocation> {
        self.policy.revocation(connection.instance)
    }
    /// Trusted status check; a foreign or retired connection cannot start new guest work.
    pub fn connection_phase(
        &self,
        connection: &Connection,
    ) -> Result<crate::lifecycle::InstancePhase> {
        self.policy.phase(connection.instance)
    }
    /// Trusted resource owners can check a previously bound instance without recreating a connection.
    pub fn binding_phase(
        &self,
        binding: ConnectionBinding,
    ) -> Result<crate::lifecycle::InstancePhase> {
        self.policy.phase(binding.0)
    }
    pub fn store_local(&self) -> &Store {
        &self.store
    }
    pub fn store_local_mut(&mut self) -> &mut Store {
        &mut self.store
    }
    fn content_grant(&self, connection: &Connection, kind: GrantKind, card: &str) -> Result<Grant> {
        connection.permits_kind(kind)?;
        self.policy.phase(connection.instance)?;
        connection
            .grants
            .get(&(kind, card.into(), None))
            .copied()
            .ok_or(Error::Invalid("missing grant"))
    }
    /// Safe host-side proposal entry. This does not expose administrative grants.
    pub fn edit_content(
        &mut self,
        connection: &Connection,
        change: &crate::content_change::ContentChange,
        clock: impl FnMut() -> u64,
    ) -> Result<crate::transaction::Receipt> {
        self.edit_content_guarded(connection, change, clock, |_| Ok(()))
    }
    /// Trusted extra constraints supplement the package ceiling, connection and content
    /// grant. They run at every Store authorization boundary, not only at admission.
    pub fn edit_content_guarded(
        &mut self,
        connection: &Connection,
        change: &crate::content_change::ContentChange,
        clock: impl FnMut() -> u64,
        guard: impl FnMut(u64) -> Result<()>,
    ) -> Result<crate::transaction::Receipt> {
        self.edit_content_guarded_with_evidence(connection, change, &[], clock, guard)
    }
    pub fn edit_content_with_evidence(
        &mut self,
        connection: &Connection,
        change: &crate::content_change::ContentChange,
        evidence: &[crate::task_evidence::Evidence],
        clock: impl FnMut() -> u64,
    ) -> Result<crate::transaction::Receipt> {
        self.edit_content_guarded_with_evidence(connection, change, evidence, clock, |_| Ok(()))
    }
    /// Historical material is saved in the same transaction as content; grants and optional
    /// live proof checks remain mandatory at every original authorization boundary.
    pub fn edit_content_guarded_with_evidence(
        &mut self,
        connection: &Connection,
        change: &crate::content_change::ContentChange,
        evidence: &[crate::task_evidence::Evidence],
        clock: impl FnMut() -> u64,
        guard: impl FnMut(u64) -> Result<()>,
    ) -> Result<crate::transaction::Receipt> {
        let grant = self.content_grant(connection, GrantKind::EditContent, &change.card_id)?;
        self.policy.edit_content_guarded_with_evidence(
            connection.instance,
            grant,
            &mut self.store,
            change,
            evidence,
            clock,
            guard,
        )
    }
    pub fn create_content(
        &mut self,
        connection: &Connection,
        operation: &str,
        card: &crate::content::CardRecord,
        clock: impl FnMut() -> u64,
    ) -> Result<crate::transaction::Receipt> {
        self.create_content_with_evidence(connection, operation, card, &[], clock)
    }
    pub fn create_content_with_evidence(
        &mut self,
        connection: &Connection,
        operation: &str,
        card: &crate::content::CardRecord,
        evidence: &[crate::task_evidence::Evidence],
        clock: impl FnMut() -> u64,
    ) -> Result<crate::transaction::Receipt> {
        let grant = self.content_grant(connection, GrantKind::CreateContent, &card.summary().id)?;
        self.policy.create_content_with_evidence(
            connection.instance,
            grant,
            &mut self.store,
            operation,
            card,
            evidence,
            clock,
        )
    }
    pub fn read_content(
        &mut self,
        connection: &Connection,
        card: &str,
        clock: impl FnMut() -> u64,
    ) -> Result<crate::content::CardRecord> {
        let grant = self.content_grant(connection, GrantKind::ReadContent, card)?;
        self.policy
            .read_content(connection.instance, grant, &self.store, card, clock)
    }
    /// Apply current object authorization to bytes from this Store's exact frozen scan.
    /// Foreign Store/snapshot/connection identities reject before consuming the host clock.
    pub fn read_snapshot_content(
        &mut self,
        connection: &Connection,
        snapshot: &crate::store::CardReadSnapshot,
        card: &crate::store::FrozenCard,
        clock: impl FnMut() -> u64,
    ) -> Result<crate::content::CardRecord> {
        self.store.validate_snapshot_card(snapshot, card)?;
        let grant = self.content_grant(connection, GrantKind::ReadContent, card.id())?;
        self.policy
            .read_frozen_content(connection.instance, grant, card.card(), clock)
    }
    /// Capture only an existing exact object grant for later host-side delivery checks.
    /// This never grants access and is tied to the original connection and grant lifetime.
    pub fn content_authorization(
        &self,
        connection: &Connection,
        kind: GrantKind,
        card: &str,
        attachment: Option<&str>,
        now: u64,
    ) -> Result<crate::lifecycle::ContentAuthorization> {
        crate::identity(card)?;
        if let Some(id) = attachment {
            crate::identity(id)?;
        }
        if (kind == GrantKind::ReadAttachment) != attachment.is_some() {
            return Err(Error::Invalid("attachment scope"));
        }
        connection.permits_kind(kind)?;
        self.policy.phase(connection.instance)?;
        let grant = *connection
            .grants
            .get(&(kind, card.into(), attachment.map(str::to_owned)))
            .ok_or(Error::Invalid("missing grant"))?;
        self.policy
            .content_authorization(connection.instance, grant, (kind, card, attachment), now)
    }
    /// Hold the runtime exclusively through response creation. Transport owns the connection.
    /// Clock is a trusted monotonic host function; errors before command decoding have no response id.
    pub fn dispatch(
        &mut self,
        connection: &Connection,
        input: &[u8],
        clock: impl FnMut() -> u64,
    ) -> Result<Vec<u8>> {
        self.dispatch_inner(connection, input, clock, |_, _| Ok(()), false)
    }
    /// Extra host constraints intersect the original grant at every read/commit boundary
    /// and after encoding a successful response. A late denial cannot undo a committed write.
    pub fn dispatch_guarded(
        &mut self,
        connection: &Connection,
        input: &[u8],
        clock: impl FnMut() -> u64,
        guard: impl FnMut(&Command, u64) -> Result<()>,
    ) -> Result<Vec<u8>> {
        self.dispatch_inner(connection, input, clock, guard, true)
    }
    // Legacy dispatch retains its original clock/response boundaries. Only the new
    // explicitly guarded entry samples the additional post-encoding delivery check.
    fn dispatch_inner(
        &mut self,
        connection: &Connection,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
        mut guard: impl FnMut(&Command, u64) -> Result<()>,
        final_delivery_check: bool,
    ) -> Result<Vec<u8>> {
        if input.len() > MAX_MESSAGE_BYTES {
            return Err(Error::Limit);
        }
        let fixed = input.to_vec();
        let command = Command::decode(&fixed)?;
        let kind = match command {
            Command::CreateContent(_) => GrantKind::CreateContent,
            Command::EditContent(_) => GrantKind::EditContent,
            Command::ReadContent(_) => GrantKind::ReadContent,
            Command::Rename(_) => GrantKind::Rename,
            Command::ReadAttachment(_) => GrantKind::ReadAttachment,
            Command::ReadSummary { .. } => GrantKind::ReadSummary,
            Command::QueryOperation { .. } => GrantKind::QueryOperation,
        };
        let attachment = match &command {
            Command::ReadAttachment(v) => Some(v.attachment_id.clone()),
            _ => None,
        };
        let mut selected_grant = None;
        let result = (|| {
            connection.permits_kind(kind)?;
            self.policy.phase(connection.instance)?;
            let grant = *connection
                .grants
                .get(&(kind, command.card_id().to_owned(), attachment.clone()))
                .ok_or(Error::Invalid("missing grant"))?;
            selected_grant = Some(grant);
            match &command {
                Command::CreateContent(v) => {
                    let card = crate::content::CardRecord::new(
                        &v.card_id,
                        &v.type_id,
                        v.format_version,
                        &v.title,
                        v.body.clone(),
                    )?;
                    self.policy
                        .create_content_with_evidence_guarded(
                            connection.instance,
                            grant,
                            &mut self.store,
                            &v.operation_id,
                            &card,
                            &[],
                            &mut clock,
                            |now| guard(&command, now),
                        )
                        .map(Outcome::ContentCommitted)
                }
                Command::EditContent(v) => self
                    .edit_content_guarded(connection, v, &mut clock, |now| guard(&command, now))
                    .map(Outcome::ContentCommitted),
                Command::ReadContent(v) => {
                    use sha2::{Digest, Sha256};
                    let card = self.policy.read_content_guarded(
                        connection.instance,
                        grant,
                        &self.store,
                        &v.card_id,
                        &mut clock,
                        |now| guard(&command, now),
                    )?;
                    if card.summary().revision != v.expected_revision {
                        return Err(Error::RevisionConflict);
                    }
                    let body = card.body();
                    if v.offset > body.len() as u64 {
                        return Err(Error::Limit);
                    }
                    let start = v.offset as usize;
                    let part = crate::runtime::ContentChunk {
                        card_id: v.card_id.clone(),
                        revision: v.expected_revision,
                        offset: v.offset,
                        total_length: body.len() as u64,
                        body_sha256: Sha256::digest(&body).into(),
                        bytes: body[start..(start + v.length as usize).min(body.len())].to_vec(),
                    };
                    // Hashing/copying can be substantial; recheck authorization immediately before delivery.
                    self.policy.read_content_guarded(
                        connection.instance,
                        grant,
                        &self.store,
                        &v.card_id,
                        &mut clock,
                        |now| guard(&command, now),
                    )?;
                    Ok(Outcome::ContentChunk(part))
                }
                Command::Rename(request) => {
                    let now = clock();
                    guard(&command, now)?;
                    let permit = self
                        .policy
                        .begin(connection.instance, grant, request, now)?;
                    self.policy
                        .commit_rename_guarded(permit, &mut self.store, &mut clock, |now| {
                            guard(&command, now)
                        })
                        .map(Outcome::Renamed)
                }
                Command::ReadAttachment(request) => self
                    .policy
                    .read_attachment_guarded(
                        connection.instance,
                        grant,
                        &self.store,
                        request,
                        &mut clock,
                        |now| guard(&command, now),
                    )
                    .map(Outcome::AttachmentChunk),
                Command::QueryOperation {
                    card_id,
                    operation_id,
                    ..
                } => self
                    .policy
                    .query_operation_guarded(
                        connection.instance,
                        grant,
                        &self.store,
                        (card_id, operation_id),
                        &mut clock,
                        |now| guard(&command, now),
                    )
                    .map(|result| Outcome::OperationResult {
                        card_id: card_id.clone(),
                        operation_id: operation_id.clone(),
                        result,
                    }),
                Command::ReadSummary { card_id, .. } => self
                    .policy
                    .read_summary_guarded(
                        connection.instance,
                        grant,
                        &self.store,
                        card_id,
                        &mut clock,
                        |now| guard(&command, now),
                    )
                    .map(Outcome::Summary),
            }
        })();
        let succeeded = result.is_ok();
        let response = Response {
            request_id: command.request_id().into(),
            outcome: result.unwrap_or_else(|error| Outcome::Rejected(failure(error))),
        };
        match response.encode() {
            Ok(bytes) => {
                if succeeded && final_delivery_check {
                    let grant = selected_grant.ok_or(Error::Invalid("missing grant"))?;
                    if let Err(error) = self.policy.check_scope_guarded(
                        connection.instance,
                        grant,
                        (kind, command.card_id(), attachment.as_deref()),
                        clock(),
                        |now| guard(&command, now),
                    ) {
                        return Response {
                            request_id: command.request_id().into(),
                            outcome: Outcome::Rejected(failure(error)),
                        }
                        .encode();
                    }
                }
                Ok(bytes)
            }
            Err(Error::Limit) => Response {
                request_id: command.request_id().into(),
                outcome: Outcome::Rejected(crate::response::Failure::Limit),
            }
            .encode(),
            Err(error) => Err(error),
        }
    }
}
