//! Atomic admission of one file mutation's Prepared history and protected input.
//! Uses the same IO history/material SQL and accounting as independent callers.
use super::{Store, boundary, io_evidence, io_intent, sql};
use crate::{
    Error, Result,
    file_effect::{CreateOutcome, DeleteOutcome, ReplaceOutcome},
    file_mutation::{Disposition, RequestRecord},
    io_evidence::{Kind, Material},
    io_intent::{ObservationSource, Phase, Record},
    plugin_package::io::IoCapability,
};
use rusqlite::TransactionBehavior;
impl Store {
    /// Atomically retain Prepared, follow-up capacity, both material reservations,
    /// and the exact protected RequestRecord. This does not claim dispatch.
    /// Err or unwinding before commit rolls all writes back through Transaction
    /// ownership. CommitUnknown requires a read of original history, never resend.
    pub fn prepare_file_mutation_local_authorized(
        &mut self,
        prepared: &Record,
        material: &Material,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        let command = prepared.command();
        if prepared.phase() != Phase::Prepared
            || !matches!(
                command.capability,
                IoCapability::FileCreate | IoCapability::FileReplace | IoCapability::FileDelete
            )
            || command.protocol_sha256 != crate::file_mutation::schema_digest()
            || material.kind() != Kind::Request
            || material.operation_id() != command.operation_id
            || material.subject() != command.subject
            || material.request_sha256() != command.request_sha256
            || material.payload_sha256() != command.request_sha256
            || material.payload().len() as u64 != command.request_bytes
        {
            return Err(Error::Invalid("file mutation preparation metadata"));
        }
        let request = RequestRecord::decode(material.payload())?;
        prepared.matches_command(&request.command()?)?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        let version: i64 = sql(tx.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
        if !(21..=super::SCHEMA_VERSION).contains(&version) {
            return Err(Error::UnsupportedVersion);
        }
        let existing = io_intent::history(&tx, &command.operation_id)?;
        for stored in &existing {
            stored.matches_command(command)?;
        }
        verify_history(&tx, &existing)?;
        let (stored, _) =
            io_intent::append_in_tx(&tx, self.budget, prepared, &mut authorize, false)?;
        io_intent::reserve_followup_in_tx(&tx, self.budget, command, &mut authorize)?;
        io_evidence::reserve_materials_in_tx(&tx, self.budget, command, &mut authorize)?;
        io_evidence::store_material_in_tx(
            &tx,
            &command.subject,
            Kind::Request,
            material,
            &mut authorize,
        )?;
        // No effectful callback may run inside this synchronous authorization.
        // It is checked again after every piece has been admitted, before commit.
        authorize()?;
        boundary("file-mutation-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("file-mutation-after-commit");
        Ok(stored)
    }

    /// Claim one Create attempt only after exact content and its audit receipt
    /// are durable. This records uncertainty before any temporary/destination
    /// file is created. It does not itself grant OS authority or execute a write.
    /// Duplicate/uncertain claims must be reconciled, never automatically replayed.
    pub fn claim_file_create_local_authorized(
        &mut self,
        candidate: &Record,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        if candidate.phase() != Phase::OutcomeUnknown
            || candidate.command().capability != IoCapability::FileCreate
            || candidate.command().protocol_sha256 != crate::file_mutation::schema_digest()
        {
            return Err(Error::Invalid("file create command"));
        }
        authorize()?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        check_version(&tx)?;
        let history = io_intent::history(&tx, &candidate.command().operation_id)?;
        verify_history(&tx, &history)?;
        let latest = history.last().ok_or(Error::NotFound)?;
        if latest.phase() != Phase::Prepared {
            return Err(Error::RevisionConflict);
        }
        let material = io_evidence::load_material(
            &tx,
            &candidate.command().operation_id,
            Kind::Request,
            &history,
        )?
        .ok_or(Error::Integrity)?;
        let request = RequestRecord::decode(material.payload())?;
        candidate.matches_command(&request.command()?)?;
        if request.request().disposition != Disposition::Create
            || request.request().target.relative_path.is_none()
        {
            return Err(Error::Invalid("file create destination"));
        }
        super::file_content::require_for_dispatch(&tx, latest, &request)?;
        io_evidence::require_pending_file_response(&tx, &history)?;
        let (stored, changed) =
            io_intent::append_in_tx(&tx, self.budget, candidate, &mut authorize, true)?;
        if !changed {
            return Err(Error::RevisionConflict);
        }
        // Recheck the newly appended history, including receipt-before-claim
        // sequence, inside this same transaction before any caller gets a claim.
        super::file_content::require_for_dispatch(&tx, &stored, &request)?;
        authorize()?;
        boundary("file-create-claim-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("file-create-claim-after-commit");
        Ok(stored)
    }

    /// Atomically retain an exact Create outcome and terminal observation.
    /// This records historical evidence only and never repeats the OS effect.
    pub fn observe_file_create_local_authorized(
        &mut self,
        candidate: &Record,
        response: &Material,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        validate_create_command(candidate, Phase::Observed)?;
        if response.kind() != Kind::Response
            || response.operation_id() != candidate.command().operation_id
            || response.subject() != candidate.command().subject
            || response.request_sha256() != candidate.command().request_sha256
            || response.digest().as_slice() != candidate.data().observation_sha256
            || !matches!(
                ObservationSource::try_from(candidate.data().observation_source),
                Ok(ObservationSource::OriginalResponse | ObservationSource::Reconciliation)
            )
        {
            return Err(Error::Invalid("file create observation metadata"));
        }
        // Check live authority before reading protected request/response bytes.
        authorize()?;
        let outcome = CreateOutcome::decode(response.payload())?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        check_version(&tx)?;
        let history = io_intent::history(&tx, &candidate.command().operation_id)?;
        verify_history(&tx, &history)?;
        if history.last().map(Record::phase) != Some(Phase::OutcomeUnknown) {
            return Err(Error::RevisionConflict);
        }
        let request = load_create_request(&tx, &history)?;
        candidate.matches_command(&request.command()?)?;
        super::file_content::require_for_dispatch(
            &tx,
            history.last().ok_or(Error::Integrity)?,
            &request,
        )?;
        create_outcome_matches(&outcome, &request, candidate.command().request_sha256)?;
        io_evidence::store_material_in_tx(
            &tx,
            &candidate.command().subject,
            Kind::Response,
            response,
            &mut authorize,
        )?;
        let (stored, changed) =
            io_intent::append_in_tx(&tx, self.budget, candidate, &mut authorize, false)?;
        if !changed {
            return Err(Error::RevisionConflict);
        }
        verify_history(
            &tx,
            &io_intent::history(&tx, &candidate.command().operation_id)?,
        )?;
        authorize()?;
        boundary("file-create-observe-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("file-create-observe-after-commit");
        Ok(stored)
    }

    /// Claim one Replace attempt only after exact content and its audit receipt
    /// are durable. This records uncertainty before any temporary/destination
    /// file is replaced. It does not itself grant OS authority or execute a write.
    /// Only a trusted host that has confirmed conditional replacement support
    /// may claim; Core cannot verify the live target identity or OS guarantee.
    /// Duplicate/uncertain claims must be reconciled, never automatically replayed.
    pub fn claim_file_replace_local_authorized(
        &mut self,
        candidate: &Record,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        if candidate.phase() != Phase::OutcomeUnknown
            || candidate.command().capability != IoCapability::FileReplace
            || candidate.command().protocol_sha256 != crate::file_mutation::schema_digest()
        {
            return Err(Error::Invalid("file replace command"));
        }
        authorize()?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        check_version(&tx)?;
        let history = io_intent::history(&tx, &candidate.command().operation_id)?;
        verify_history(&tx, &history)?;
        let latest = history.last().ok_or(Error::NotFound)?;
        if latest.phase() != Phase::Prepared {
            return Err(Error::RevisionConflict);
        }
        let material = io_evidence::load_material(
            &tx,
            &candidate.command().operation_id,
            Kind::Request,
            &history,
        )?
        .ok_or(Error::Integrity)?;
        let request = RequestRecord::decode(material.payload())?;
        candidate.matches_command(&request.command()?)?;
        if request.request().disposition != Disposition::Replace
            || request.request().expected_identity.is_none()
        {
            return Err(Error::Invalid("file replace expected identity"));
        }
        super::file_content::require_for_dispatch(&tx, latest, &request)?;
        io_evidence::require_pending_file_response(&tx, &history)?;
        let (stored, changed) =
            io_intent::append_in_tx(&tx, self.budget, candidate, &mut authorize, true)?;
        if !changed {
            return Err(Error::RevisionConflict);
        }
        // Recheck the newly appended history, including receipt-before-claim
        // sequence, inside this same transaction before any caller gets a claim.
        super::file_content::require_for_dispatch(&tx, &stored, &request)?;
        authorize()?;
        boundary("file-replace-claim-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("file-replace-claim-after-commit");
        Ok(stored)
    }

    /// Atomically retain an exact Replace outcome and terminal observation.
    /// This records historical evidence only and never repeats the OS effect.
    pub fn observe_file_replace_local_authorized(
        &mut self,
        candidate: &Record,
        response: &Material,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        validate_replace_command(candidate, Phase::Observed)?;
        if response.kind() != Kind::Response
            || response.operation_id() != candidate.command().operation_id
            || response.subject() != candidate.command().subject
            || response.request_sha256() != candidate.command().request_sha256
            || response.digest().as_slice() != candidate.data().observation_sha256
            || !matches!(
                ObservationSource::try_from(candidate.data().observation_source),
                Ok(ObservationSource::OriginalResponse | ObservationSource::Reconciliation)
            )
        {
            return Err(Error::Invalid("file replace observation metadata"));
        }
        // Check live authority before reading protected request/response bytes.
        authorize()?;
        let outcome = ReplaceOutcome::decode(response.payload())?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        check_version(&tx)?;
        let history = io_intent::history(&tx, &candidate.command().operation_id)?;
        verify_history(&tx, &history)?;
        if history.last().map(Record::phase) != Some(Phase::OutcomeUnknown) {
            return Err(Error::RevisionConflict);
        }
        let request = load_replace_request(&tx, &history)?;
        candidate.matches_command(&request.command()?)?;
        super::file_content::require_for_dispatch(
            &tx,
            history.last().ok_or(Error::Integrity)?,
            &request,
        )?;
        replace_outcome_matches(&outcome, &request, candidate.command().request_sha256)?;
        io_evidence::store_material_in_tx(
            &tx,
            &candidate.command().subject,
            Kind::Response,
            response,
            &mut authorize,
        )?;
        let (stored, changed) =
            io_intent::append_in_tx(&tx, self.budget, candidate, &mut authorize, false)?;
        if !changed {
            return Err(Error::RevisionConflict);
        }
        verify_history(
            &tx,
            &io_intent::history(&tx, &candidate.command().operation_id)?,
        )?;
        authorize()?;
        boundary("file-replace-observe-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("file-replace-observe-after-commit");
        Ok(stored)
    }

    /// Persist the one-way dispatch boundary before the platform attempts a
    /// delete. A repeated claim is a conflict, including after a lost commit
    /// response; callers must inspect history instead of retrying the effect.
    pub fn claim_file_delete_local_authorized(
        &mut self,
        candidate: &Record,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        validate_delete_command(candidate, Phase::OutcomeUnknown)?;
        // Check live authority before reading retained protected request bytes.
        authorize()?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        check_version(&tx)?;
        let history = io_intent::history(&tx, &candidate.command().operation_id)?;
        verify_history(&tx, &history)?;
        if history.last().map(Record::phase) != Some(Phase::Prepared) {
            return Err(Error::RevisionConflict);
        }
        let request = load_delete_request(&tx, &history)?;
        candidate.matches_command(&request.command()?)?;
        io_evidence::require_pending_file_response(&tx, &history)?;
        let (stored, changed) =
            io_intent::append_in_tx(&tx, self.budget, candidate, &mut authorize, true)?;
        if !changed {
            return Err(Error::RevisionConflict);
        }
        authorize()?;
        boundary("file-delete-claim-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("file-delete-claim-after-commit");
        Ok(stored)
    }

    /// Atomically retain a bounded, exact delete outcome and the terminal
    /// observation. This does not authorize another filesystem attempt.
    pub fn observe_file_delete_local_authorized(
        &mut self,
        candidate: &Record,
        response: &Material,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        validate_delete_command(candidate, Phase::Observed)?;
        if response.kind() != Kind::Response
            || response.operation_id() != candidate.command().operation_id
            || response.subject() != candidate.command().subject
            || response.request_sha256() != candidate.command().request_sha256
            || response.digest().as_slice() != candidate.data().observation_sha256
            || !matches!(
                ObservationSource::try_from(candidate.data().observation_source),
                Ok(ObservationSource::OriginalResponse | ObservationSource::Reconciliation)
            )
        {
            return Err(Error::Invalid("file delete observation metadata"));
        }
        // Check live authority before reading retained protected request bytes.
        authorize()?;
        let outcome = DeleteOutcome::decode(response.payload())?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        check_version(&tx)?;
        let history = io_intent::history(&tx, &candidate.command().operation_id)?;
        verify_history(&tx, &history)?;
        if history.last().map(Record::phase) != Some(Phase::OutcomeUnknown) {
            return Err(Error::RevisionConflict);
        }
        let request = load_delete_request(&tx, &history)?;
        candidate.matches_command(&request.command()?)?;
        outcome_matches(&outcome, &request, candidate.command().request_sha256)?;
        io_evidence::store_material_in_tx(
            &tx,
            &candidate.command().subject,
            Kind::Response,
            response,
            &mut authorize,
        )?;
        let (stored, changed) =
            io_intent::append_in_tx(&tx, self.budget, candidate, &mut authorize, false)?;
        if !changed {
            return Err(Error::RevisionConflict);
        }
        authorize()?;
        boundary("file-delete-observe-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("file-delete-observe-after-commit");
        Ok(stored)
    }
}

fn check_version(c: &rusqlite::Connection) -> Result<()> {
    let version: i64 = sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
    if !(21..=super::SCHEMA_VERSION).contains(&version) {
        return Err(Error::UnsupportedVersion);
    }
    Ok(())
}

fn validate_create_command(candidate: &Record, phase: Phase) -> Result<()> {
    if candidate.phase() != phase
        || candidate.command().capability != IoCapability::FileCreate
        || candidate.command().protocol_sha256 != crate::file_mutation::schema_digest()
    {
        return Err(Error::Invalid("file create command"));
    }
    Ok(())
}

fn load_create_request(c: &rusqlite::Connection, history: &[Record]) -> Result<RequestRecord> {
    let first = history.first().ok_or(Error::NotFound)?;
    let material =
        io_evidence::load_material(c, &first.command().operation_id, Kind::Request, history)?
            .ok_or(Error::Integrity)?;
    let request = RequestRecord::decode(material.payload())?;
    first
        .matches_command(&request.command()?)
        .map_err(|_| Error::Integrity)?;
    if request.request().disposition != Disposition::Create
        || request.request().target.relative_path.is_none()
    {
        return Err(Error::Invalid("not a file create request"));
    }
    Ok(request)
}

fn create_outcome_matches(
    outcome: &CreateOutcome,
    request: &RequestRecord,
    request_sha256: [u8; 32],
) -> Result<()> {
    let value = request.request();
    if outcome.operation_id() != value.operation_id
        || outcome.subject() != value.subject
        || outcome.request_sha256() != request_sha256
        || outcome.target_reference() != value.target.reference
        || Some(outcome.content_sha256()) != value.content_sha256
        || outcome.content_length() != value.content_length
    {
        return Err(Error::OperationConflict);
    }
    Ok(())
}

fn validate_replace_command(candidate: &Record, phase: Phase) -> Result<()> {
    if candidate.phase() != phase
        || candidate.command().capability != IoCapability::FileReplace
        || candidate.command().protocol_sha256 != crate::file_mutation::schema_digest()
    {
        return Err(Error::Invalid("file replace command"));
    }
    Ok(())
}

fn load_replace_request(c: &rusqlite::Connection, history: &[Record]) -> Result<RequestRecord> {
    let first = history.first().ok_or(Error::NotFound)?;
    let material =
        io_evidence::load_material(c, &first.command().operation_id, Kind::Request, history)?
            .ok_or(Error::Integrity)?;
    let request = RequestRecord::decode(material.payload())?;
    first
        .matches_command(&request.command()?)
        .map_err(|_| Error::Integrity)?;
    if request.request().disposition != Disposition::Replace
        || request.request().expected_identity.is_none()
    {
        return Err(Error::Invalid("not a file replace request"));
    }
    Ok(request)
}

fn replace_outcome_matches(
    outcome: &ReplaceOutcome,
    request: &RequestRecord,
    request_sha256: [u8; 32],
) -> Result<()> {
    let value = request.request();
    if outcome.operation_id() != value.operation_id
        || outcome.subject() != value.subject
        || outcome.request_sha256() != request_sha256
        || outcome.target_reference() != value.target.reference
        || Some(outcome.expected_identity()) != value.expected_identity
        || Some(outcome.content_sha256()) != value.content_sha256
        || outcome.content_length() != value.content_length
    {
        return Err(Error::OperationConflict);
    }
    Ok(())
}

fn validate_delete_command(candidate: &Record, phase: Phase) -> Result<()> {
    if candidate.phase() != phase
        || candidate.command().capability != IoCapability::FileDelete
        || candidate.command().protocol_sha256 != crate::file_mutation::schema_digest()
    {
        return Err(Error::Invalid("file delete command"));
    }
    Ok(())
}

fn load_delete_request(c: &rusqlite::Connection, history: &[Record]) -> Result<RequestRecord> {
    let first = history.first().ok_or(Error::NotFound)?;
    let material =
        io_evidence::load_material(c, &first.command().operation_id, Kind::Request, history)?
            .ok_or(Error::Integrity)?;
    let request = RequestRecord::decode(material.payload())?;
    first
        .matches_command(&request.command()?)
        .map_err(|_| Error::Integrity)?;
    if request.request().disposition != Disposition::Delete {
        return Err(Error::Invalid("not a file delete request"));
    }
    ensure_no_delete_content(c, &first.command().operation_id)?;
    Ok(request)
}

fn ensure_no_delete_content(c: &rusqlite::Connection, operation: &str) -> Result<()> {
    let version: i64 = sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
    if version >= 22 {
        let present: bool = sql(c.query_row(
            "SELECT EXISTS(SELECT 1 FROM file_mutation_content WHERE operation_id=?1)",
            [operation],
            |r| r.get(0),
        ))?;
        if present {
            return Err(Error::Integrity);
        }
    }
    if version >= 23 {
        let present: bool = sql(c.query_row(
            "SELECT EXISTS(SELECT 1 FROM file_content_receipts WHERE operation_id=?1)",
            [operation],
            |r| r.get(0),
        ))?;
        if present {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}

fn outcome_matches(
    outcome: &DeleteOutcome,
    request: &RequestRecord,
    request_sha256: [u8; 32],
) -> Result<()> {
    let value = request.request();
    if outcome.operation_id() != value.operation_id
        || outcome.subject() != value.subject
        || outcome.request_sha256() != request_sha256
        || outcome.target_reference() != value.target.reference
        || outcome.expected_identity() != value.expected_identity.ok_or(Error::Integrity)?
    {
        return Err(Error::OperationConflict);
    }
    Ok(())
}

/// File plans reuse v21 IO material tables. The dedicated protocol digest is the
/// feature gate: older readers reject it instead of silently accepting a plan.
/// Called only after atomic admission, never from intermediate SQL helpers.
pub(super) fn verify_history(c: &rusqlite::Connection, history: &[Record]) -> Result<()> {
    let Some(first) = history.first() else {
        return Ok(());
    };
    if first.command().protocol_sha256 != crate::file_mutation::schema_digest() {
        return Ok(());
    }
    check_version(c)?;
    let material =
        io_evidence::load_material(c, &first.command().operation_id, Kind::Request, history)?
            .ok_or(Error::Integrity)?;
    let request = RequestRecord::decode(material.payload())?;
    first
        .matches_command(&request.command()?)
        .map_err(|_| Error::Integrity)?;
    let latest = history.last().ok_or(Error::Integrity)?;
    if request.request().disposition == Disposition::Create {
        if history.iter().any(|v| {
            !matches!(
                v.phase(),
                Phase::Prepared
                    | Phase::CancelledBeforeDispatch
                    | Phase::OutcomeUnknown
                    | Phase::Observed
            )
        }) {
            return Err(Error::UnsupportedVersion);
        }
        if matches!(latest.phase(), Phase::OutcomeUnknown | Phase::Observed) {
            if request.request().target.relative_path.is_none() {
                return Err(Error::Integrity);
            }
            // Even simultaneous loss of bytes AND receipt must fail closed once
            // dispatch is recorded; an empty pair is only valid before dispatch.
            super::file_content::require_for_dispatch(c, latest, &request)
                .map_err(|_| Error::Integrity)?;
        }
        if latest.phase() == Phase::OutcomeUnknown {
            io_evidence::require_pending_file_response(c, history)?;
        }
        if latest.phase() == Phase::Observed {
            let response = io_evidence::load_material(
                c,
                &first.command().operation_id,
                Kind::Response,
                history,
            )?
            .ok_or(Error::Integrity)?;
            let outcome =
                CreateOutcome::decode(response.payload()).map_err(|_| Error::Integrity)?;
            create_outcome_matches(&outcome, &request, first.command().request_sha256)
                .map_err(|_| Error::Integrity)?;
            if latest.data().observation_sha256 != response.digest().as_slice()
                || !matches!(
                    ObservationSource::try_from(latest.data().observation_source),
                    Ok(ObservationSource::OriginalResponse | ObservationSource::Reconciliation)
                )
            {
                return Err(Error::Integrity);
            }
        }
        return Ok(());
    }
    if request.request().disposition == Disposition::Replace {
        if history.iter().any(|v| {
            !matches!(
                v.phase(),
                Phase::Prepared
                    | Phase::CancelledBeforeDispatch
                    | Phase::OutcomeUnknown
                    | Phase::Observed
            )
        }) {
            return Err(Error::UnsupportedVersion);
        }
        if matches!(latest.phase(), Phase::OutcomeUnknown | Phase::Observed) {
            if request.request().expected_identity.is_none() {
                return Err(Error::Integrity);
            }
            super::file_content::require_for_dispatch(c, latest, &request)
                .map_err(|_| Error::Integrity)?;
        }
        if latest.phase() == Phase::OutcomeUnknown {
            io_evidence::require_pending_file_response(c, history)?;
        }
        if latest.phase() == Phase::Observed {
            let response = io_evidence::load_material(
                c,
                &first.command().operation_id,
                Kind::Response,
                history,
            )?
            .ok_or(Error::Integrity)?;
            let outcome =
                ReplaceOutcome::decode(response.payload()).map_err(|_| Error::Integrity)?;
            replace_outcome_matches(&outcome, &request, first.command().request_sha256)
                .map_err(|_| Error::Integrity)?;
            if latest.data().observation_sha256 != response.digest().as_slice()
                || !matches!(
                    ObservationSource::try_from(latest.data().observation_source),
                    Ok(ObservationSource::OriginalResponse | ObservationSource::Reconciliation)
                )
            {
                return Err(Error::Integrity);
            }
        }
        return Ok(());
    }
    if latest.phase() == Phase::OutcomeUnknown {
        io_evidence::require_pending_file_response(c, history)?;
    }
    ensure_no_delete_content(c, &first.command().operation_id)?;
    if history.iter().any(|v| {
        !matches!(
            v.phase(),
            Phase::Prepared
                | Phase::CancelledBeforeDispatch
                | Phase::OutcomeUnknown
                | Phase::Observed
        )
    }) {
        return Err(Error::UnsupportedVersion);
    }
    if latest.phase() == Phase::Observed {
        let response =
            io_evidence::load_material(c, &first.command().operation_id, Kind::Response, history)?
                .ok_or(Error::Integrity)?;
        let outcome = DeleteOutcome::decode(response.payload()).map_err(|_| Error::Integrity)?;
        outcome_matches(&outcome, &request, first.command().request_sha256)
            .map_err(|_| Error::Integrity)?;
        if latest.data().observation_sha256 != response.digest().as_slice()
            || !matches!(
                ObservationSource::try_from(latest.data().observation_source),
                Ok(ObservationSource::OriginalResponse | ObservationSource::Reconciliation)
            )
        {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}
