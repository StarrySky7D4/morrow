//! Trusted connection binding. No instance, grant, clock, or storage path is accepted on wire.
use crate::{
    Error, Result,
    lifecycle::{Grant, GrantKind, HostPolicy, Instance},
    response::{Outcome, Response, failure},
    runtime::{Command, MAX_MESSAGE_BYTES},
    store::Store,
};
use std::collections::{BTreeMap, BTreeSet};
/// Opaque host-owned endpoint; never serialize or let an untrusted caller select another endpoint.
pub struct Connection {
    instance: Instance,
    package_digest: Option<[u8; 32]>,
    ceiling: Option<BTreeSet<GrantKind>>,
    grants: BTreeMap<(GrantKind, String, Option<String>), Grant>,
}
impl Connection {
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
    pub fn store_local(&self) -> &Store {
        &self.store
    }
    pub fn store_local_mut(&mut self) -> &mut Store {
        &mut self.store
    }
    /// Hold the runtime exclusively through response creation. Transport owns the connection.
    /// Clock is a trusted monotonic host function; errors before command decoding have no response id.
    pub fn dispatch(
        &mut self,
        connection: &Connection,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> Result<Vec<u8>> {
        if input.len() > MAX_MESSAGE_BYTES {
            return Err(Error::Limit);
        }
        let fixed = input.to_vec();
        let command = Command::decode(&fixed)?;
        let kind = match command {
            Command::Rename(_) => GrantKind::Rename,
            Command::ReadAttachment(_) => GrantKind::ReadAttachment,
            Command::ReadSummary { .. } => GrantKind::ReadSummary,
            Command::QueryOperation { .. } => GrantKind::QueryOperation,
        };
        let attachment = match &command {
            Command::ReadAttachment(v) => Some(v.attachment_id.clone()),
            _ => None,
        };
        let result = (|| {
            connection.permits_kind(kind)?;
            self.policy.phase(connection.instance)?;
            let grant = *connection
                .grants
                .get(&(kind, command.card_id().to_owned(), attachment))
                .ok_or(Error::Invalid("missing grant"))?;
            match &command {
                Command::Rename(request) => {
                    let permit = self
                        .policy
                        .begin(connection.instance, grant, request, clock())?;
                    self.policy
                        .commit_rename(permit, &mut self.store, &mut clock)
                        .map(Outcome::Renamed)
                }
                Command::ReadAttachment(request) => self
                    .policy
                    .read_attachment(connection.instance, grant, &self.store, request, &mut clock)
                    .map(Outcome::AttachmentChunk),
                Command::QueryOperation {
                    card_id,
                    operation_id,
                    ..
                } => self
                    .policy
                    .query_operation(
                        connection.instance,
                        grant,
                        &self.store,
                        (card_id, operation_id),
                        &mut clock,
                    )
                    .map(|result| Outcome::OperationResult {
                        card_id: card_id.clone(),
                        operation_id: operation_id.clone(),
                        result,
                    }),
                Command::ReadSummary { card_id, .. } => self
                    .policy
                    .read_summary(connection.instance, grant, &self.store, card_id, &mut clock)
                    .map(Outcome::Summary),
            }
        })();
        let response = Response {
            request_id: command.request_id().into(),
            outcome: result.unwrap_or_else(|error| Outcome::Rejected(failure(error))),
        };
        match response.encode() {
            Ok(bytes) => Ok(bytes),
            Err(Error::Limit) => Response {
                request_id: command.request_id().into(),
                outcome: Outcome::Rejected(crate::response::Failure::Limit),
            }
            .encode(),
            Err(error) => Err(error),
        }
    }
}
