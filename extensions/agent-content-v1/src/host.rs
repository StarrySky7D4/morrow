//! Trusted adapter for an existing HostRuntime and its original Connection.
//! Wire nominations and proposals grant no permission. Approval and execution
//! are separate host-only calls; this adapter never opens or creates a Store.
use crate::{Action, ContentRef, Error, Outcome, Reply, Request};
use morrow_core::{
    dispatch::{Connection, ConnectionBinding, HostBinding, HostRuntime},
    lifecycle::{ContentAuthorization, GrantKind, InstancePhase},
    response::{Failure, Outcome as CoreOutcome, Response},
    runtime::{Command, ReadContent},
};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_PROPOSALS: usize = crate::MAX_PROPOSALS;

/// Session-local facts; neither Approved nor DispatchUnknown proves a commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalPhase {
    Proposed,
    Approved,
    DispatchUnknown,
    LocallyCommitted,
}

struct Proposal {
    digest: [u8; 32],
    reference: ContentRef,
    command: Vec<u8>,
    read: ContentAuthorization,
    edit: ContentAuthorization,
    phase: ProposalPhase,
}

/// Owns only bounded nominations and fixed proposals, not a runtime or grant.
pub struct ContentHost {
    host: HostBinding,
    connection: ConnectionBinding,
    cards: BTreeSet<String>,
    proposals: BTreeMap<String, Proposal>,
}

impl std::fmt::Debug for ContentHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ContentHost")
            .field("nominations", &self.cards.len())
            .field("proposals", &self.proposals.len())
            .finish_non_exhaustive()
    }
}

impl ContentHost {
    /// Trusted nominations limit discovery. Each nominated object still needs
    /// its own current grant and the original package capability ceiling.
    pub fn new(
        runtime: &HostRuntime,
        connection: &Connection,
        allowed_cards: Vec<String>,
    ) -> morrow_core::Result<Self> {
        if allowed_cards.is_empty() || allowed_cards.len() > crate::MAX_QUERY_CARDS {
            return Err(morrow_core::Error::Limit);
        }
        if runtime.connection_phase(connection)? != InstancePhase::Ready {
            return Err(morrow_core::Error::Invalid("content connection not ready"));
        }
        let mut cards = BTreeSet::new();
        for card in allowed_cards {
            // Validate through the unchanged public Core command codec.
            Command::ReadSummary {
                request_id: "nomination".into(),
                card_id: card.clone(),
            }
            .validate()?;
            if !cards.insert(card) {
                return Err(morrow_core::Error::Invalid("duplicate content nomination"));
            }
        }
        Ok(Self {
            host: runtime.binding(),
            connection: connection.binding(),
            cards,
            proposals: BTreeMap::new(),
        })
    }

    pub fn phase(&self, operation: &str) -> Option<ProposalPhase> {
        self.proposals.get(operation).map(|proposal| proposal.phase)
    }

    fn bound(&self, runtime: &HostRuntime, connection: &Connection) -> morrow_core::Result<()> {
        if runtime.binding() != self.host || connection.binding() != self.connection {
            return Err(morrow_core::Error::Invalid("foreign content binding"));
        }
        if runtime.connection_phase(connection)? != InstancePhase::Ready {
            return Err(morrow_core::Error::Invalid("content connection not ready"));
        }
        Ok(())
    }

    fn nominated(&self, card: &str) -> morrow_core::Result<()> {
        if self.cards.contains(card) {
            Ok(())
        } else {
            Err(morrow_core::Error::Invalid(
                "content outside nominated scope",
            ))
        }
    }

    /// No grant, approval, execution or content export can be requested here.
    /// Every Core result retains its original byte codec and authorization.
    pub fn dispatch(
        &mut self,
        runtime: &mut HostRuntime,
        connection: &Connection,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> crate::Result<Vec<u8>> {
        let request = Request::decode(input)?;
        let result = self
            .bound(runtime, connection)
            .and_then(|()| self.request(runtime, connection, &request, &mut clock));
        let (outcome, authorizations) = match result {
            Ok(value) => value,
            Err(error) => (
                Outcome::Rejected {
                    failure: failure(error),
                },
                Vec::new(),
            ),
        };
        let reply = match Reply::new(&request, outcome) {
            Ok(reply) => reply,
            Err(Error::Limit) => {
                return Reply::new(
                    &request,
                    Outcome::Rejected {
                        failure: Failure::Limit,
                    },
                )?
                .encode();
            }
            Err(error) => return Err(error),
        };
        let output = match reply.encode() {
            Ok(output) => output,
            Err(Error::Limit) => {
                return Reply::new(
                    &request,
                    Outcome::Rejected {
                        failure: Failure::Limit,
                    },
                )?
                .encode();
            }
            Err(error) => return Err(error),
        };
        // An earlier query element or an encoded body cannot escape a late
        // revocation/expiry while subsequent elements and the outer frame build.
        if !authorizations.is_empty() {
            let now = clock();
            for authorization in authorizations {
                if let Err(error) = authorization.check(now) {
                    return Reply::new(
                        &request,
                        Outcome::Rejected {
                            failure: failure(error),
                        },
                    )?
                    .encode();
                }
            }
        }
        Ok(output)
    }

    fn request(
        &mut self,
        runtime: &mut HostRuntime,
        connection: &Connection,
        request: &Request,
        clock: &mut impl FnMut() -> u64,
    ) -> morrow_core::Result<(Outcome, Vec<ContentAuthorization>)> {
        match request.action() {
            Action::Query { cards } => {
                for card in cards {
                    self.nominated(card)?;
                }
                let mut responses = Vec::with_capacity(cards.len());
                let mut authorizations = Vec::with_capacity(cards.len());
                for card in cards {
                    let command = Command::ReadSummary {
                        request_id: request.request_id().into(),
                        card_id: card.clone(),
                    };
                    let authorization = runtime.content_authorization(
                        connection,
                        GrantKind::ReadSummary,
                        card,
                        None,
                        clock(),
                    );
                    let bytes = match authorization {
                        Ok(authorization) => {
                            let bytes = runtime.dispatch_guarded(
                                connection,
                                &command.encode()?,
                                &mut *clock,
                                |_, now| authorization.check(now),
                            )?;
                            if matches!(Response::decode(&bytes)?.outcome, CoreOutcome::Summary(_))
                            {
                                authorizations.push(authorization);
                            }
                            bytes
                        }
                        Err(error) => Response {
                            request_id: request.request_id().into(),
                            outcome: CoreOutcome::Rejected(failure(error)),
                        }
                        .encode()?,
                    };
                    responses.push(bytes);
                }
                Ok((Outcome::Query { responses }, authorizations))
            }
            Action::ReadRef {
                reference,
                offset,
                length,
            } => {
                self.nominated(&reference.card_id)?;
                let read = runtime.content_authorization(
                    connection,
                    GrantKind::ReadContent,
                    &reference.card_id,
                    None,
                    clock(),
                )?;
                let response = read_reference(
                    runtime,
                    connection,
                    request.request_id(),
                    reference,
                    *offset,
                    *length,
                    clock,
                    &read,
                    None,
                )?;
                Ok((Outcome::ReadRef { response }, vec![read]))
            }
            Action::InspectOperation {
                card_id,
                operation_id,
            } => {
                self.nominated(card_id)?;
                let authorization = runtime.content_authorization(
                    connection,
                    GrantKind::QueryOperation,
                    card_id,
                    None,
                    clock(),
                )?;
                let command = Command::QueryOperation {
                    request_id: request.request_id().into(),
                    card_id: card_id.clone(),
                    operation_id: operation_id.clone(),
                };
                let response =
                    runtime.dispatch_guarded(connection, &command.encode()?, clock, |_, now| {
                        authorization.check(now)
                    })?;
                Ok((Outcome::Operation { response }, vec![authorization]))
            }
            Action::ProposeMutation { reference, command } => {
                self.nominated(&reference.card_id)?;
                let Command::EditContent(change) = Command::decode(command)? else {
                    return Err(morrow_core::Error::Invalid(
                        "only bounded edit proposal supported",
                    ));
                };
                let operation = change.operation_id;
                if let Some(proposal) = self.proposals.get(&operation) {
                    if proposal.digest != request.digest() {
                        return Err(morrow_core::Error::OperationConflict);
                    }
                    if !matches!(
                        proposal.phase,
                        ProposalPhase::Proposed | ProposalPhase::Approved
                    ) {
                        return Err(morrow_core::Error::CommitUnknown);
                    }
                    let now = clock();
                    proposal.read.check(now)?;
                    proposal.edit.check(now)?;
                    return Ok((
                        Outcome::Proposed {
                            operation_id: operation,
                            proposal_sha256: proposal.digest,
                        },
                        vec![proposal.read.clone(), proposal.edit.clone()],
                    ));
                }
                if self.proposals.len() >= MAX_PROPOSALS {
                    return Err(morrow_core::Error::Limit);
                }
                let now = clock();
                let read = runtime.content_authorization(
                    connection,
                    GrantKind::ReadContent,
                    &reference.card_id,
                    None,
                    now,
                )?;
                let edit = runtime.content_authorization(
                    connection,
                    GrantKind::EditContent,
                    &reference.card_id,
                    None,
                    now,
                )?;
                check_reference(
                    runtime,
                    connection,
                    request.request_id(),
                    reference,
                    clock,
                    &read,
                    &edit,
                )?;
                self.proposals.insert(
                    operation.clone(),
                    Proposal {
                        digest: request.digest(),
                        reference: reference.clone(),
                        command: command.clone(),
                        read: read.clone(),
                        edit: edit.clone(),
                        phase: ProposalPhase::Proposed,
                    },
                );
                Ok((
                    Outcome::Proposed {
                        operation_id: operation,
                        proposal_sha256: request.digest(),
                    },
                    vec![read, edit],
                ))
            }
        }
    }

    /// Trusted review of the entire original proposal, not a wire approval bit.
    /// Original object grants are restriction probes; approval never renews them.
    pub fn approve(
        &mut self,
        runtime: &HostRuntime,
        connection: &Connection,
        operation: &str,
        proposal_sha256: [u8; 32],
        now: u64,
    ) -> morrow_core::Result<()> {
        self.bound(runtime, connection)?;
        let proposal = self
            .proposals
            .get_mut(operation)
            .ok_or(morrow_core::Error::Invalid("missing proposal"))?;
        if proposal.digest != proposal_sha256 {
            return Err(morrow_core::Error::OperationConflict);
        }
        if proposal.phase != ProposalPhase::Proposed {
            return Err(morrow_core::Error::Invalid(
                "proposal already approved or consumed",
            ));
        }
        proposal.read.check(now)?;
        proposal.edit.check(now)?;
        proposal.phase = ProposalPhase::Approved;
        Ok(())
    }

    /// Host-only, single attempt. Consumption occurs before original Core
    /// dispatch; loss or denial after that boundary retains DispatchUnknown.
    /// InspectOperation can observe history; it cannot grant another execution.
    pub fn execute_approved(
        &mut self,
        runtime: &mut HostRuntime,
        connection: &Connection,
        operation: &str,
        mut clock: impl FnMut() -> u64,
    ) -> morrow_core::Result<Response> {
        self.bound(runtime, connection)?;
        let proposal = self
            .proposals
            .get(operation)
            .ok_or(morrow_core::Error::Invalid("missing proposal"))?;
        if proposal.phase != ProposalPhase::Approved {
            return Err(morrow_core::Error::Invalid(
                "proposal not approved or consumed",
            ));
        }
        let now = clock();
        proposal.read.check(now)?;
        proposal.edit.check(now)?;
        let read = proposal.read.clone();
        let edit = proposal.edit.clone();
        let reference = proposal.reference.clone();
        let command = proposal.command.clone();
        check_reference(
            runtime, connection, operation, &reference, &mut clock, &read, &edit,
        )?;
        self.proposals
            .get_mut(operation)
            .expect("fixed proposal retained")
            .phase = ProposalPhase::DispatchUnknown;
        let bytes = runtime
            .dispatch_guarded(connection, &command, &mut clock, |_, now| {
                read.check(now)?;
                edit.check(now)
            })
            .map_err(|_| morrow_core::Error::CommitUnknown)?;
        let response = Response::decode(&bytes).map_err(|_| morrow_core::Error::CommitUnknown)?;
        let CoreOutcome::ContentCommitted(receipt) = &response.outcome else {
            return Err(morrow_core::Error::CommitUnknown);
        };
        if response.request_id != operation
            || receipt.operation_id != operation
            || receipt.card_id != reference.card_id
            || reference.revision.checked_add(1) != Some(receipt.revision)
        {
            return Err(morrow_core::Error::CommitUnknown);
        }
        let now = clock();
        read.check(now)
            .map_err(|_| morrow_core::Error::CommitUnknown)?;
        edit.check(now)
            .map_err(|_| morrow_core::Error::CommitUnknown)?;
        self.proposals
            .get_mut(operation)
            .expect("fixed proposal retained")
            .phase = ProposalPhase::LocallyCommitted;
        Ok(response)
    }
}

fn failure(error: morrow_core::Error) -> Failure {
    use morrow_core::Error as E;
    match error {
        E::NotFound => Failure::NotFound,
        E::RevisionConflict => Failure::RevisionConflict,
        E::OperationConflict => Failure::OperationConflict,
        E::StorageFull | E::EventCapacity => Failure::Capacity,
        E::StorageBusy => Failure::Busy,
        E::CommitUnknown => Failure::CommitUnknown,
        E::Limit => Failure::Limit,
        E::Invalid(_) => Failure::Denied,
        _ => Failure::Storage,
    }
}

fn check_reference(
    runtime: &mut HostRuntime,
    connection: &Connection,
    request_id: &str,
    reference: &ContentRef,
    clock: &mut impl FnMut() -> u64,
    read: &ContentAuthorization,
    edit: &ContentAuthorization,
) -> morrow_core::Result<()> {
    read_reference(
        runtime,
        connection,
        request_id,
        reference,
        0,
        1,
        clock,
        read,
        Some(edit),
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_reference(
    runtime: &mut HostRuntime,
    connection: &Connection,
    request_id: &str,
    reference: &ContentRef,
    offset: u64,
    length: u32,
    clock: &mut impl FnMut() -> u64,
    read: &ContentAuthorization,
    edit: Option<&ContentAuthorization>,
) -> morrow_core::Result<Vec<u8>> {
    let command = Command::ReadContent(ReadContent {
        request_id: request_id.into(),
        card_id: reference.card_id.clone(),
        expected_revision: reference.revision,
        offset,
        length,
    });
    let bytes = runtime.dispatch_guarded(connection, &command.encode()?, clock, |_, now| {
        read.check(now)?;
        if let Some(edit) = edit {
            edit.check(now)?;
        }
        Ok(())
    })?;
    let response = Response::decode(&bytes)?;
    match response.outcome {
        CoreOutcome::ContentChunk(part) => {
            if part.card_id != reference.card_id || part.revision != reference.revision {
                return Err(morrow_core::Error::RevisionConflict);
            }
            if part.total_length != reference.total_length
                || part.body_sha256 != reference.body_sha256
            {
                return Err(morrow_core::Error::Integrity);
            }
            Ok(bytes)
        }
        CoreOutcome::Rejected(failure) => Err(match failure {
            Failure::RevisionConflict => morrow_core::Error::RevisionConflict,
            Failure::NotFound => morrow_core::Error::NotFound,
            Failure::Limit => morrow_core::Error::Limit,
            Failure::OperationConflict => morrow_core::Error::OperationConflict,
            Failure::Capacity => morrow_core::Error::StorageFull,
            Failure::Busy => morrow_core::Error::StorageBusy,
            Failure::Storage => morrow_core::Error::Storage,
            Failure::CommitUnknown => morrow_core::Error::CommitUnknown,
            Failure::Denied => morrow_core::Error::Invalid("content read denied"),
        }),
        _ => Err(morrow_core::Error::Integrity),
    }
}
