//! Trusted connection binding. No instance, grant, clock, or storage path is accepted on wire.
use crate::{
    Error, Result,
    lifecycle::{Grant, GrantKind, HostPolicy, Instance},
    response::{Outcome, Response, failure},
    runtime::{Command, MAX_MESSAGE_BYTES},
    store::Store,
};
use std::collections::BTreeMap;
/// Opaque host-owned endpoint; never serialize or let an untrusted caller select another endpoint.
pub struct Connection {
    instance: Instance,
    grants: BTreeMap<(GrantKind, String), Grant>,
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
            grants: BTreeMap::new(),
        })
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
        self.policy.phase(connection.instance)?;
        let key = (kind, card.to_owned());
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
            .remove(&(kind, card.into()))
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
            Command::ReadSummary { .. } => GrantKind::ReadSummary,
        };
        let result = (|| {
            self.policy.phase(connection.instance)?;
            let grant = *connection
                .grants
                .get(&(kind, command.card_id().to_owned()))
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
