//! Trusted-host service result history, never a dispatch grant or proof of a
//! complete application transaction. Only a newly committed strict IO claim
//! permits this helper to return Execute. Unknown outcomes are never resent.
use crate::io_execution::{Error, Result, storage};
use morrow_core::{
    io_evidence::{Kind, Material},
    io_intent::{ObservationSource, Phase, Record},
    service::{self, Request, Response},
    service_record::{Policy, RequestRecord},
    store::Store,
};
use std::sync::{Arc, Mutex};

/// Host-fixed retention policy and trusted UTC-millisecond clock. Clones share
/// the clock; callbacks must be pure, bounded and must not re-enter this journal.
#[derive(Clone)]
pub struct ServiceJournal {
    policy: Policy,
    clock: Arc<Mutex<JournalClock>>,
}
struct JournalClock {
    sample: Box<dyn FnMut() -> u64 + Send>,
    high_water: u64,
}
impl ServiceJournal {
    pub fn new(policy: Policy, clock: impl FnMut() -> u64 + Send + 'static) -> Result<Self> {
        policy.validate().map_err(storage)?;
        Ok(Self {
            policy,
            clock: Arc::new(Mutex::new(JournalClock {
                sample: Box::new(clock),
                high_water: 0,
            })),
        })
    }
    /// Restore only the historical namespace and retention policy, bound to a
    /// freshly issued actual service grant. This does not resolve credential or
    /// approval references, restore authority, or open a listener. The worker
    /// continues to enforce live authorization. Configuration revision changes
    /// do not automatically revoke already constructed journals or routes.
    pub fn from_config(
        config: &morrow_core::service_config::Config,
        grant: &crate::service_io::ServiceGrant,
        clock: impl FnMut() -> u64 + Send + 'static,
    ) -> Result<Self> {
        grant.validate_config(config)?;
        Self::new(config.policy().map_err(storage)?, clock)
    }
    pub fn policy(&self) -> &Policy {
        &self.policy
    }
    fn now(&self) -> Result<u64> {
        let mut clock = self.clock.lock().map_err(|_| Error::Clock)?;
        let now = (clock.sample)();
        if now == 0 || now < clock.high_water {
            return Err(Error::Clock);
        }
        clock.high_water = now;
        Ok(now)
    }
    fn fresh(&self, request: &RequestRecord) -> Result<()> {
        Validity::new(self, request).check()
    }
}
/// Retained by the job through Ready/read. Historical retention is not renewed
/// by a replay, a new process or a late successful guest completion.
#[derive(Clone)]
pub(crate) struct Validity {
    journal: ServiceJournal,
    created_ms: u64,
    expires_ms: u64,
}
impl Validity {
    fn new(journal: &ServiceJournal, request: &RequestRecord) -> Self {
        Self {
            journal: journal.clone(),
            created_ms: request.created_ms(),
            expires_ms: request.expires_ms(),
        }
    }
    /// Also shorten the monotonic deadline checked at Runner execution boundaries.
    /// This is not a realtime CPU interrupt; fuel remains the computation bound.
    /// UTC is checked again at imports/transport/delivery to catch forward jumps.
    pub(crate) fn arm(&self, cancel: &crate::Cancellation) -> Result<()> {
        let now = self.journal.now()?;
        if now < self.created_ms {
            return Err(Error::Clock);
        }
        let remaining = self
            .expires_ms
            .checked_sub(now)
            .filter(|n| *n > 0)
            .ok_or(Error::Expired)?;
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_millis(remaining))
            .ok_or(Error::Clock)?;
        cancel.limit_deadline(deadline);
        Ok(())
    }
    pub(crate) fn check(&self) -> Result<()> {
        let now = self.journal.now()?;
        if now < self.created_ms {
            return Err(Error::Clock);
        }
        if now >= self.expires_ms {
            return Err(Error::Expired);
        }
        Ok(())
    }
}
// A transient result, never a queue element; retain the exact owned Store record.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Begin {
    Execute {
        record: Record,
        validity: Validity,
    },
    Replay {
        reply: service::Reply,
        completion: Vec<u8>,
        validity: Validity,
    },
    Unknown,
    Expired,
}

/// Read-only recovery state. None of these variants authorizes execution or
/// reconciliation; nonterminal states retain the original retention deadline.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Query {
    Missing,
    Prepared {
        validity: Validity,
    },
    Unknown {
        validity: Validity,
    },
    Cancelled {
        validity: Validity,
    },
    Expired,
    Observed {
        reply: service::Reply,
        completion: Vec<u8>,
        validity: Validity,
    },
}

/// Inspect the original request and evidence without preparing, reserving,
/// claiming, executing or reconciling anything. Requiring the complete original
/// request also preserves the host-supplied content scope in the identity check.
pub(crate) fn query(
    store: &Store,
    journal: &ServiceJournal,
    key: &str,
    request: &Request,
    package: [u8; 32],
    mut authorize: impl FnMut() -> morrow_core::Result<()>,
) -> Result<Query> {
    authorize().map_err(|_| Error::Denied)?;
    let candidate =
        RequestRecord::encode(journal.policy(), key, request, journal.now()?).map_err(storage)?;
    let command = candidate.command(package).map_err(storage)?;
    let result = if let Some(stored) = store
        .lookup_io_intent(&command.subject, &command.operation_id)
        .map_err(storage)?
    {
        let retained = original(store, &stored)?;
        retained
            .matches(journal.policy(), key, request)
            .map_err(storage)?;
        stored
            .matches_command(&retained.command(package).map_err(storage)?)
            .map_err(storage)?;
        let validity = Validity::new(journal, &retained);
        match validity.check() {
            Err(Error::Expired) => Query::Expired,
            Err(error) => return Err(error),
            Ok(()) => match stored.phase() {
                Phase::Prepared => Query::Prepared { validity },
                Phase::OutcomeUnknown => Query::Unknown { validity },
                Phase::CancelledBeforeDispatch => Query::Cancelled { validity },
                Phase::InvalidPhase => return Err(Error::Integrity),
                Phase::Observed => {
                    let response = store
                        .io_material(&command.subject, &command.operation_id, Kind::Response)
                        .map_err(storage)?
                        .ok_or(Error::EvidenceUnavailable)?;
                    if stored.data().observation_sha256 != response.payload_sha256() {
                        return Err(Error::Integrity);
                    }
                    let completion = response.payload().to_vec();
                    let reply =
                        Response::decode(retained.request(), &completion).map_err(storage)?;
                    Query::Observed {
                        reply,
                        completion,
                        validity,
                    }
                }
            },
        }
    } else {
        Query::Missing
    };
    // Missing, expired and unfinished history are still private information.
    // Every successful outcome must pass the final live authorization check.
    authorize().map_err(|_| Error::Denied)?;
    let validity = match &result {
        Query::Prepared { validity }
        | Query::Unknown { validity }
        | Query::Cancelled { validity }
        | Query::Observed { validity, .. } => Some(validity),
        Query::Missing | Query::Expired => None,
    };
    if let Some(validity) = validity {
        match validity.check() {
            Err(Error::Expired) => return Ok(Query::Expired),
            Err(error) => return Err(error),
            Ok(()) => {}
        }
    }
    Ok(result)
}

// Preserve the precise host guard failure rather than flattening it to a Store
// error. The Store always runs this callback before committing its transaction.
fn guarded<T>(
    guard: &mut impl FnMut() -> Result<()>,
    write: impl FnOnce(&mut dyn FnMut() -> morrow_core::Result<()>) -> morrow_core::Result<T>,
) -> Result<T> {
    let mut rejection = None;
    let result = write(&mut || {
        guard().map_err(|error| {
            rejection = Some(error);
            morrow_core::Error::Invalid("inactive service history admission")
        })
    });
    result.map_err(|error| rejection.unwrap_or_else(|| storage(error)))
}

fn original(store: &Store, record: &Record) -> Result<RequestRecord> {
    let command = record.command();
    let material = store
        .io_material(&command.subject, &command.operation_id, Kind::Request)
        .map_err(storage)?
        .ok_or(Error::EvidenceUnavailable)?;
    if material.payload_sha256() != command.request_sha256
        || material.payload().len() as u64 != command.request_bytes
    {
        return Err(Error::Integrity);
    }
    let request = RequestRecord::decode(material.payload()).map_err(storage)?;
    record
        .matches_command(&request.command(command.package_sha256).map_err(storage)?)
        .map_err(storage)?;
    Ok(request)
}

pub(crate) fn begin(
    store: &mut Store,
    journal: &ServiceJournal,
    key: &str,
    request: &Request,
    package: [u8; 32],
    mut authorize: impl FnMut() -> morrow_core::Result<()>,
) -> Result<Begin> {
    match begin_inner(store, journal, key, request, package, &mut authorize) {
        Err(Error::Expired) => Ok(Begin::Expired),
        result => result,
    }
}
fn begin_inner(
    store: &mut Store,
    journal: &ServiceJournal,
    key: &str,
    request: &Request,
    package: [u8; 32],
    authorize: &mut impl FnMut() -> morrow_core::Result<()>,
) -> Result<Begin> {
    authorize().map_err(|_| Error::Denied)?;
    let candidate =
        RequestRecord::encode(journal.policy(), key, request, journal.now()?).map_err(storage)?;
    let command = candidate.command(package).map_err(storage)?;
    let existing = store
        .lookup_io_intent(&command.subject, &command.operation_id)
        .map_err(storage)?;
    let (request_record, prepared, admitted) = if let Some(stored) = existing {
        let retained = original(store, &stored)?;
        retained
            .matches(journal.policy(), key, request)
            .map_err(storage)?;
        stored
            .matches_command(&retained.command(package).map_err(storage)?)
            .map_err(storage)?;
        journal.fresh(&retained)?;
        match stored.phase() {
            Phase::Observed => {
                let response = store
                    .io_material(&command.subject, &command.operation_id, Kind::Response)
                    .map_err(storage)?
                    .ok_or(Error::EvidenceUnavailable)?;
                if stored.data().observation_sha256 != response.payload_sha256() {
                    return Err(Error::Integrity);
                }
                let completion = response.payload().to_vec();
                let reply = Response::decode(retained.request(), &completion).map_err(storage)?;
                authorize().map_err(|_| Error::Denied)?;
                journal.fresh(&retained)?;
                return Ok(Begin::Replay {
                    reply,
                    completion,
                    validity: Validity::new(journal, &retained),
                });
            }
            Phase::OutcomeUnknown => return Ok(Begin::Unknown),
            Phase::CancelledBeforeDispatch => return Err(Error::Cancelled),
            Phase::InvalidPhase => return Err(Error::Integrity),
            Phase::Prepared => (retained, stored, false),
        }
    } else {
        let proposed = Record::prepared(command).map_err(storage)?;
        let mut guard = || {
            authorize().map_err(|_| Error::Denied)?;
            journal.fresh(&candidate)
        };
        let command = proposed.command();
        let material = Material::encode(
            Kind::Request,
            &command.operation_id,
            &command.subject,
            command.request_sha256,
            candidate.container(),
        )
        .map_err(storage)?;
        let stored = guarded(&mut guard, |guard| {
            store.prepare_service_request_local_authorized(&proposed, &material, guard)
        })?;
        (candidate, stored, true)
    };
    // Preserve the exact original command/creation time on Prepared recovery.
    let command = prepared.command();
    let mut guard = || {
        authorize().map_err(|_| Error::Denied)?;
        journal.fresh(&request_record)
    };
    if !admitted {
        guarded(&mut guard, |guard| {
            store.reserve_io_intent_followup(command, guard)
        })?;
        guarded(&mut guard, |guard| {
            store.reserve_io_materials(command, guard)
        })?;
        let material = Material::encode(
            Kind::Request,
            &command.operation_id,
            &command.subject,
            command.request_sha256,
            request_record.container(),
        )
        .map_err(storage)?;
        guarded(&mut guard, |guard| {
            store.store_io_material(&command.subject, Kind::Request, &material, guard)
        })?;
    }
    guard()?;
    let boundary = prepared.propose_dispatch_boundary().map_err(storage)?;
    // This API is deliberately non-idempotent: an identical committed claim is
    // still a losing claim. A lost commit receipt can never return Execute.
    let claimed = guarded(&mut guard, |guard| {
        store.claim_io_dispatch_local_authorized(&boundary, guard)
    })?;
    Ok(Begin::Execute {
        record: claimed,
        validity: Validity::new(journal, &request_record),
    })
}

/// Retain an actual typed completion, including after live delivery was revoked.
/// This is historical recording only and never a permission to send or retry.
pub(crate) fn finish(store: &mut Store, boundary: &Record, completion: &[u8]) -> Result<()> {
    if boundary.phase() != Phase::OutcomeUnknown {
        return Err(Error::Conflict);
    }
    let request = original(store, boundary)?;
    Response::decode(request.request(), completion).map_err(storage)?;
    let command = boundary.command();
    let response = Material::encode(
        Kind::Response,
        &command.operation_id,
        &command.subject,
        command.request_sha256,
        completion,
    )
    .map_err(storage)?;
    let observed = boundary
        .propose_observation(
            response.payload_sha256(),
            ObservationSource::OriginalResponse,
        )
        .map_err(storage)?;
    let stored = store
        .lookup_io_intent(&command.subject, &command.operation_id)
        .map_err(storage)?
        .ok_or(Error::NotFound)?;
    if stored.raw() != boundary.raw() && stored.raw() != observed.raw() {
        return Err(Error::Conflict);
    }
    store
        .store_io_material(&command.subject, Kind::Response, &response, || Ok(()))
        .map_err(storage)?;
    store
        .append_io_intent_local_authorized(&observed, || Ok(()))
        .map_err(storage)?;
    Ok(())
}

#[cfg(test)]
#[path = "service_history_tests.rs"]
mod tests;
