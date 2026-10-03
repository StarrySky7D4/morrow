//! Narrow, read-only HTTP history projection. Persisted facts never grant a new
//! dispatch, replay or access to original headers/body. Every read shares one
//! pinned transaction, and foreign subjects are hidden before decoding history.
use super::{Store, io_evidence, io_intent, sql};
use crate::{
    Error, Result,
    io::{self, Action, OperationOutcome, Request, Response, Status},
    io_evidence::Kind,
    io_intent::{Command, ObservationSource, Phase, Record},
    plugin_package::io::IoCapability,
};
use rusqlite::params;

impl Store {
    /// Returns only the original bounded HTTP status under exact command pins.
    /// This does not claim, reserve, reconcile, append or authorize an operation.
    pub fn lookup_http_operation_status(&self, expected: &Command) -> Result<Option<OperationOutcome>> {
        // Reuse command validation without opening or changing a Store transaction.
        Record::prepared(expected.clone())?;
        if expected.capability != IoCapability::HttpRequest
            || expected.protocol_sha256 != io::schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        if expected.request_bytes > io::MAX_FRAME_BYTES as u64
            || expected.response_limit > io::MAX_FRAME_BYTES as u64
        {
            return Err(Error::Limit);
        }
        let tx = sql(self.connection.unchecked_transaction())?;
        let version: i64 = sql(tx.query_row("PRAGMA user_version", [], |row| row.get(0)))?;
        if version < 15 {
            return Ok(None);
        }
        // Do not ask whether a foreign or unowned malformed operation exists.
        // Revision-one subject ownership is established before history payloads.
        let owned: bool = sql(tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM io_intents i JOIN operations o ON o.id=i.event_id WHERE i.operation_id=?1 AND i.revision=1 AND o.card_id=?2)",
            params![expected.operation_id, expected.subject], |row| row.get(0),
        ))?;
        if !owned {
            return Ok(None);
        }
        // The shared history validator guards chains with LIMIT4. Reject excess
        // revisions in metadata first, so this path loads at most three bodies.
        let excess: bool = sql(tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM io_intents WHERE operation_id=?1 AND revision NOT BETWEEN 1 AND 3)",
            [&expected.operation_id], |row| row.get(0),
        ))?;
        if excess {
            return Err(Error::EvidenceUnavailable);
        }
        let history = io_intent::history(&tx, &expected.operation_id)
            .map_err(|_| Error::EvidenceUnavailable)?;
        for record in &history {
            record.matches_command(expected)?;
        }
        let latest = history.last().ok_or(Error::EvidenceUnavailable)?;
        let status = match latest.phase() {
            Phase::Prepared => Status::Pending,
            Phase::OutcomeUnknown => Status::OutcomeUnknown,
            Phase::CancelledBeforeDispatch => Status::Cancelled,
            Phase::Observed => {
                if latest.data().observation_source != ObservationSource::OriginalResponse as i32 {
                    return Err(Error::EvidenceUnavailable);
                }
                // Include the bounded material protobuf metadata overhead, while
                // independently requiring payloads within the original IO frame cap.
                let material_limit = io::MAX_FRAME_BYTES + 4096;
                let request = io_evidence::load_material_bounded(
                    &tx, &expected.operation_id, Kind::Request, &history, material_limit,
                ).map_err(|_| Error::EvidenceUnavailable)?.ok_or(Error::EvidenceUnavailable)?;
                let response = io_evidence::load_material_bounded(
                    &tx, &expected.operation_id, Kind::Response, &history, material_limit,
                ).map_err(|_| Error::EvidenceUnavailable)?.ok_or(Error::EvidenceUnavailable)?;
                if request.payload().len() > io::MAX_FRAME_BYTES
                    || response.payload().len() > io::MAX_FRAME_BYTES
                    || latest.data().observation_sha256.as_slice() != response.payload_sha256().as_slice()
                {
                    return Err(Error::EvidenceUnavailable);
                }
                let original = Request::decode(request.payload()).map_err(|_| Error::EvidenceUnavailable)?;
                let Action::SubmitHttp(submission) = original.action() else {
                    return Err(Error::EvidenceUnavailable);
                };
                if submission.operation_id.as_slice() != expected.operation_id.as_bytes() {
                    return Err(Error::EvidenceUnavailable);
                }
                let outcome = Response::decode_http(&original, response.payload())
                    .map_err(|_| Error::EvidenceUnavailable)?;
                return Ok(Some(OperationOutcome {
                    status: outcome.status,
                    http_status: outcome.http_status,
                }));
            }
            Phase::InvalidPhase => return Err(Error::EvidenceUnavailable),
        };
        Ok(Some(OperationOutcome { status, http_status: 0 }))
    }
}
