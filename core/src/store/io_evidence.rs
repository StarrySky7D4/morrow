//! Protected IO request/response originals: fixed historical material storage
//! with pre-send capacity reservation. Stored bytes are neither credentials, a
//! dispatch permit, nor proof of remote success. The reservation is a logical
//! Store quota consumed atomically when material is admitted.
use super::{Store, boundary, sql};
use crate::{
    Error, Result, identity,
    io_evidence::{self, Kind, Material},
    io_intent::{Phase, Record},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(super) const SCHEMA: &str = "CREATE TABLE io_evidence(digest BLOB PRIMARY KEY CHECK(length(digest)=32),operation_id TEXT NOT NULL,subject TEXT NOT NULL,kind INTEGER NOT NULL CHECK(kind IN(1,2)),request_sha256 BLOB NOT NULL CHECK(length(request_sha256)=32),payload_sha256 BLOB NOT NULL CHECK(length(payload_sha256)=32),payload_bytes INTEGER NOT NULL CHECK(payload_bytes>=0),container BLOB NOT NULL) STRICT;
CREATE UNIQUE INDEX io_evidence_kind ON io_evidence(operation_id,kind);
CREATE TABLE io_material_reservations(operation_id TEXT NOT NULL,kind INTEGER NOT NULL CHECK(kind IN(1,2)),subject TEXT NOT NULL,reserved_bytes INTEGER NOT NULL CHECK(reserved_bytes>0),PRIMARY KEY(operation_id,kind)) STRICT;";
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    let v = version(c)?;
    let n: i64 = sql(c.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE name IN ('io_evidence','io_evidence_kind','io_material_reservations')",
        [],
        |r| r.get(0),
    ))?;
    if n != 3 * i64::from(v >= 17) {
        return Err(Error::Integrity);
    }
    Ok(())
}
/// Logical byte capacity held by stored material and pending reservations.
/// Zero before v17; counted against the byte budget only, never event counts.
pub(super) fn accounted(c: &Connection) -> Result<i64> {
    if version(c)? < 17 {
        return Ok(0);
    }
    sql(c.query_row(
        "SELECT (SELECT coalesce(sum(length(container)),0) FROM io_evidence) + (SELECT coalesce(sum(reserved_bytes),0) FROM io_material_reservations)",
        [],
        |r| r.get(0),
    ))
}
fn held(c: &Connection, operation: &str, kind: Kind) -> Result<Option<(i64, String)>> {
    sql(c.query_row(
        "SELECT reserved_bytes,subject FROM io_material_reservations WHERE operation_id=?1 AND kind=?2",
        params![operation, kind.number() as i64],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional())
}
// All callers hold one SQLite transaction. The command comes from the bounded,
// fully verified immutable history, not merely the metadata in revision one.
fn load_material(
    c: &Connection,
    operation: &str,
    kind: Kind,
    history: &[Record],
) -> Result<Option<Material>> {
    let command = history.first().ok_or(Error::Integrity)?.command();
    let mut statement = sql(c.prepare(
        "SELECT subject,request_sha256,payload_sha256,payload_bytes,digest,CASE WHEN length(container)<=?3 THEN container ELSE NULL END FROM io_evidence WHERE operation_id=?1 AND kind=?2",
    ))?;
    let mut rows = sql(statement.query(params![
        operation,
        kind.number() as i64,
        io_evidence::MAX_CONTAINER_BYTES as i64
    ]))?;
    let Some(row) = sql(rows.next())? else {
        return Ok(None);
    };
    let container = sql(row.get_ref(5))?;
    if matches!(container, rusqlite::types::ValueRef::Null) {
        return Err(Error::Limit);
    }
    let material = Material::decode(container.as_blob().map_err(|_| Error::Integrity)?)?;
    let subject_ref = sql(row.get_ref(0))?;
    let subject = subject_ref.as_str().map_err(|_| Error::Integrity)?;
    let request_sha256 = sql(row.get_ref(1))?;
    let payload_sha256 = sql(row.get_ref(2))?;
    let payload_bytes: i64 = sql(row.get(3))?;
    let digest = sql(row.get_ref(4))?;
    if material.operation_id() != operation
        || material.subject() != subject
        || subject != command.subject
        || material.kind() != kind
        || material.request_sha256() != command.request_sha256
        || material.digest().as_slice() != digest.as_blob().map_err(|_| Error::Integrity)?
        || material.request_sha256().as_slice()
            != request_sha256.as_blob().map_err(|_| Error::Integrity)?
        || material.payload_sha256().as_slice()
            != payload_sha256.as_blob().map_err(|_| Error::Integrity)?
        || material.payload().len() as i64 != payload_bytes
        || held(c, operation, kind)?.is_some()
    {
        return Err(Error::Integrity);
    }
    match kind {
        Kind::Request
            if material.payload().len() as u64 != command.request_bytes
                || material.payload_sha256() != command.request_sha256 =>
        {
            return Err(Error::Integrity);
        }
        Kind::Response
            if material.payload().len() as u64 > command.response_limit
                || !matches!(
                    history.last().ok_or(Error::Integrity)?.phase(),
                    Phase::OutcomeUnknown | Phase::Observed
                ) =>
        {
            return Err(Error::Integrity);
        }
        _ => {}
    }
    Ok(Some(material))
}
pub(super) fn verify(c: &Connection) -> Result<()> {
    verify_schema(c)?;
    let v = version(c)?;
    if v < 17 {
        return Ok(());
    }
    // Every original and reservation must reference the stored revision-1
    // history of the same operation, owned by the same subject; a kind slot
    // holds either material or a reservation, never both. A response
    // reservation legitimately survives the dispatch boundary until the
    // response is admitted, so reservations only have to be gone once the
    // history is terminal (Observed or CancelledBeforeDispatch).
    let invalid: bool = sql(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM io_evidence e LEFT JOIN io_intents i ON i.operation_id=e.operation_id AND i.revision=1 LEFT JOIN operations o ON o.id=i.event_id WHERE i.event_id IS NULL OR o.card_id!=e.subject) OR EXISTS(SELECT 1 FROM io_evidence e JOIN io_material_reservations r ON r.operation_id=e.operation_id AND r.kind=e.kind) OR EXISTS(SELECT 1 FROM io_material_reservations r LEFT JOIN io_intents i ON i.operation_id=r.operation_id AND i.revision=1 LEFT JOIN operations o ON o.id=i.event_id WHERE i.event_id IS NULL OR o.card_id!=r.subject)",
        [],
        |r| r.get(0),
    ))?;
    if invalid {
        return Err(Error::Integrity);
    }
    let mut statement = sql(c.prepare("SELECT operation_id,kind FROM io_evidence"))?;
    let mut rows = sql(statement.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let operation: String = sql(row.get(0))?;
        let kind = Kind::from_number(sql(row.get(1))?)?;
        let history = super::io_intent::history(c, &operation)?;
        load_material(c, &operation, kind, &history)?.ok_or(Error::Integrity)?;
    }
    // Each held reservation must still be the exact pre-send bound the stored
    // command implies for its kind and subject.
    let mut statement = sql(
        c.prepare("SELECT operation_id,subject,kind,reserved_bytes FROM io_material_reservations")
    )?;
    let mut rows = sql(statement.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let operation: String = sql(row.get(0))?;
        let subject: String = sql(row.get(1))?;
        let kind: i64 = sql(row.get(2))?;
        let reserved: i64 = sql(row.get(3))?;
        let history = super::io_intent::history(c, &operation)?;
        let command = history.first().ok_or(Error::Integrity)?;
        if !matches!(
            history.last().ok_or(Error::Integrity)?.phase(),
            Phase::Prepared | Phase::OutcomeUnknown
        ) {
            return Err(Error::Integrity);
        }
        if command.command().subject != subject {
            return Err(Error::Integrity);
        }
        let bound = match kind {
            1 => command.command().request_bytes,
            2 => command.command().response_limit,
            _ => return Err(Error::Integrity),
        };
        if io_evidence::max_container_bytes(bound)? != reserved as u64 {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}
/// Logical pre-send capacity held for one operation's request and response
/// material. Pure accounting inside the Store: not disk space, not dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IoMaterialReservation {
    pub request_bytes: u64,
    pub response_bytes: u64,
}
impl Store {
    /// Reserves the protected material capacity for one stored Prepared
    /// command before the dispatch boundary may be crossed. The reservation is
    /// bound to the exact stored command, is idempotent per operationId, and
    /// reduces the byte capacity every other writer may consume. It is NOT
    /// physical disk space and NOT permission to contact a backend.
    pub fn reserve_io_materials(
        &mut self,
        command: &crate::io_intent::Command,
        authorize: impl FnOnce() -> Result<()>,
    ) -> Result<IoMaterialReservation> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        // Protected material only exists from format 17; older stores must be
        // reopened and migrated before any admission is attempted.
        if version(&tx)? < 17 {
            return Err(Error::UnsupportedVersion);
        }
        let all = super::io_intent::history(&tx, &command.operation_id)?;
        let stored = all.first().ok_or(Error::NotFound)?;
        // A different command for the same operationId can never reserve the
        // capacity bound to the stored original.
        stored.matches_command(command)?;
        if all.last().ok_or(Error::Integrity)?.phase() != Phase::Prepared {
            return Err(Error::Invalid("IO material reservation phase"));
        }
        let planned_request = io_evidence::max_container_bytes(command.request_bytes)?;
        let planned_response = io_evidence::max_container_bytes(command.response_limit)?;
        let request_held = held(&tx, &command.operation_id, Kind::Request)?;
        let response_held = held(&tx, &command.operation_id, Kind::Response)?;
        let request_admitted =
            load_material(&tx, &command.operation_id, Kind::Request, &all)?.is_some();
        let response_admitted =
            load_material(&tx, &command.operation_id, Kind::Response, &all)?.is_some();
        // A kind slot holds a reservation or an admitted original, never both;
        // an admitted original is already charged and needs no reservation.
        if (request_held.is_some() && request_admitted)
            || (response_held.is_some() && response_admitted)
        {
            return Err(Error::Integrity);
        }
        let mut incoming = 0u64;
        for (existing, planned, admitted) in [
            (request_held.as_ref(), planned_request, request_admitted),
            (response_held.as_ref(), planned_response, response_admitted),
        ] {
            if let Some((bytes, subject)) = existing {
                if subject != &command.subject || *bytes != planned as i64 {
                    return Err(Error::Integrity);
                }
            } else if !admitted {
                incoming = incoming.checked_add(planned).ok_or(Error::Limit)?;
            }
        }
        if incoming > 0 {
            super::byte_room(&tx, self.budget, incoming)?;
        }
        if request_held.is_none() && !request_admitted {
            sql(tx.execute(
                "INSERT INTO io_material_reservations(operation_id,kind,subject,reserved_bytes) VALUES(?1,1,?2,?3)",
                params![&command.operation_id, command.subject, planned_request as i64],
            ))?;
        }
        if response_held.is_none() && !response_admitted {
            sql(tx.execute(
                "INSERT INTO io_material_reservations(operation_id,kind,subject,reserved_bytes) VALUES(?1,2,?2,?3)",
                params![&command.operation_id, command.subject, planned_response as i64],
            ))?;
        }
        if incoming == 0 {
            // Fully idempotent: the same command never deducts twice, and the
            // authorized callback still observes the request.
            authorize()?;
            return Ok(IoMaterialReservation {
                request_bytes: planned_request,
                response_bytes: planned_response,
            });
        }
        boundary("io-material-reservation-after-insert");
        // No network or external work may occur in this synchronous callback.
        authorize()?;
        boundary("io-material-reservation-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("io-material-reservation-after-commit");
        Ok(IoMaterialReservation {
            request_bytes: planned_request,
            response_bytes: planned_response,
        })
    }
    /// Releases both material reservations while the command is still before
    /// the dispatch boundary. After OutcomeUnknown the response must be stored
    /// instead; double release stays idempotent.
    pub fn release_io_materials(&mut self, subject: &str, operation: &str) -> Result<()> {
        identity(subject)?;
        identity(operation)?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        // Protected material only exists from format 17; older stores must be
        // reopened and migrated before any admission is attempted.
        if version(&tx)? < 17 {
            return Err(Error::UnsupportedVersion);
        }
        let all = super::io_intent::history(&tx, operation)?;
        let stored = all.first().ok_or(Error::NotFound)?;
        if stored.command().subject != subject {
            return Err(Error::NotFound);
        }
        match all.last().ok_or(Error::Integrity)?.phase() {
            Phase::Prepared => {}
            _ => return Err(Error::Invalid("IO material reservation terminal")),
        }
        sql(tx.execute(
            "DELETE FROM io_material_reservations WHERE operation_id=?1",
            [operation],
        ))?;
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        Ok(())
    }
    /// Releases one never-admitted material reservation after the dispatch
    /// boundary, so an observation reconciled from an external source can close
    /// the history without pretending an original was retained. An already
    /// admitted original is never deleted and refuses this call; a reservation
    /// that is still before the boundary must use `release_io_materials`.
    pub fn release_io_material_reconciliation(
        &mut self,
        subject: &str,
        operation: &str,
        kind: io_evidence::Kind,
    ) -> Result<()> {
        identity(subject)?;
        identity(operation)?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        if version(&tx)? < 17 {
            return Err(Error::UnsupportedVersion);
        }
        let all = super::io_intent::history(&tx, operation)?;
        let stored = all.first().ok_or(Error::NotFound)?;
        if stored.command().subject != subject {
            return Err(Error::NotFound);
        }
        // Reconciliation is only meaningful once the send boundary was crossed.
        if all.last().ok_or(Error::Integrity)?.phase() != Phase::OutcomeUnknown {
            return Err(Error::Invalid("IO material reconciliation phase"));
        }
        let admitted: bool = sql(tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM io_evidence WHERE operation_id=?1 AND kind=?2)",
            params![operation, kind.number() as i64],
            |r| r.get(0),
        ))?;
        if admitted {
            return Err(Error::Invalid("IO material already admitted"));
        }
        let deleted = sql(tx.execute(
            "DELETE FROM io_material_reservations WHERE operation_id=?1 AND kind=?2 AND subject=?3",
            params![operation, kind.number() as i64, subject],
        ))?;
        if deleted != 1 {
            return Err(Error::NotFound);
        }
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        Ok(())
    }
    /// Admits one protected original for a stored command inside the caller's
    /// authorization: the matching reservation is consumed and the material is
    /// stored in the same transaction. An exact re-admission is idempotent; a
    /// different original for the same slot is a hard conflict.
    pub fn store_io_material(
        &mut self,
        subject: &str,
        kind: io_evidence::Kind,
        material: &io_evidence::Material,
        authorize: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        identity(subject)?;
        // The caller-declared slot must be the slot the record was encoded for;
        // a mismatch would otherwise only surface later as a store-integrity
        // fault after an unusable original had already been admitted.
        if material.kind() != kind {
            return Err(Error::Invalid("IO material kind"));
        }
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        // Protected material only exists from format 17; older stores must be
        // reopened and migrated before any admission is attempted.
        if version(&tx)? < 17 {
            return Err(Error::UnsupportedVersion);
        }
        let operation = material.operation_id();
        let all = super::io_intent::history(&tx, operation)?;
        let stored = all.first().ok_or(Error::NotFound)?;
        // A subject can never store or observe another subject's material.
        if stored.command().subject != subject {
            return Err(Error::NotFound);
        }
        if material.subject() != subject {
            return Err(Error::OperationConflict);
        }
        if material.request_sha256() != stored.command().request_sha256 {
            return Err(Error::OperationConflict);
        }
        if let Some(existing) = load_material(&tx, operation, kind, &all)? {
            if existing.raw() != material.raw() {
                return Err(Error::OperationConflict);
            }
            // Idempotent admission: its reservation must already be consumed.
            if held(&tx, operation, kind)?.is_some() {
                return Err(Error::Integrity);
            }
            authorize()?;
            return Ok(());
        }
        // Request material may exist before or after the dispatch boundary;
        // response material can only exist once the boundary is crossed.
        let allowed = matches!(
            (kind, all.last().ok_or(Error::Integrity)?.phase()),
            (Kind::Request, Phase::Prepared | Phase::OutcomeUnknown)
                | (Kind::Response, Phase::OutcomeUnknown)
        );
        if !allowed {
            return Err(Error::Invalid("IO material phase"));
        }
        let (reserved_bytes, reservation_subject) =
            held(&tx, operation, kind)?.ok_or(Error::Invalid("IO material reservation"))?;
        if reservation_subject != subject {
            return Err(Error::Integrity);
        }
        // A request original must be exactly the bytes the command binds; a
        // response original can never exceed the declared response limit.
        match kind {
            Kind::Request => {
                if material.payload().len() as u64 != stored.command().request_bytes
                    || material.payload_sha256() != stored.command().request_sha256
                {
                    return Err(Error::OperationConflict);
                }
            }
            Kind::Response => {
                if material.payload().len() as u64 > stored.command().response_limit {
                    return Err(Error::Limit);
                }
            }
        }
        if material.container().len() as u64 > reserved_bytes as u64 {
            return Err(Error::EventCapacity);
        }
        let deleted = sql(tx.execute(
            "DELETE FROM io_material_reservations WHERE operation_id=?1 AND kind=?2 AND subject=?3",
            params![operation, kind.number() as i64, subject],
        ))?;
        if deleted != 1 {
            return Err(Error::Integrity);
        }
        sql(tx.execute(
            "INSERT INTO io_evidence(digest,operation_id,subject,kind,request_sha256,payload_sha256,payload_bytes,container) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                material.digest().as_slice(),
                operation,
                material.subject(),
                kind.number() as i64,
                material.request_sha256().as_slice(),
                material.payload_sha256().as_slice(),
                material.payload().len() as i64,
                material.container(),
            ],
        ))?;
        boundary("io-material-after-evidence");
        // No network or external work may occur in this synchronous callback.
        authorize()?;
        boundary("io-material-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("io-material-after-commit");
        Ok(())
    }
    /// Reads back one stored original. A different subject or missing history
    /// is indistinguishable from absence; a owned history without this kind of
    /// material is reported explicitly as unavailable evidence.
    pub fn io_material(
        &self,
        subject: &str,
        operation: &str,
        kind: io_evidence::Kind,
    ) -> Result<Option<io_evidence::Material>> {
        identity(subject)?;
        identity(operation)?;
        let snapshot = sql(self.connection.unchecked_transaction())?;
        if version(&snapshot)? < 17 {
            return Ok(None);
        }
        let (present, initial, owned): (bool, bool, bool) = sql(snapshot.query_row(
            "SELECT EXISTS(SELECT 1 FROM io_intents WHERE operation_id=?1),EXISTS(SELECT 1 FROM io_intents i JOIN operations o ON o.id=i.event_id WHERE i.operation_id=?1 AND i.revision=1),EXISTS(SELECT 1 FROM io_intents i JOIN operations o ON o.id=i.event_id WHERE i.operation_id=?1 AND i.revision=1 AND o.card_id=?2)",
            params![operation, subject], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ))?;
        if present && !initial {
            return Err(Error::Integrity);
        }
        if !owned {
            return Ok(None);
        }
        let history = super::io_intent::history(&snapshot, operation)?;
        let command = history.first().ok_or(Error::Integrity)?.command();
        if command.subject != subject {
            return Err(Error::Integrity);
        }
        let material = load_material(&snapshot, operation, kind, &history)?
            .ok_or(Error::EvidenceUnavailable)?;
        Ok(Some(material))
    }
    /// Logical material capacity currently held by pending reservations.
    pub fn io_material_reservation_usage(&self) -> Result<(u64, u64)> {
        let snapshot = sql(self.connection.unchecked_transaction())?;
        if version(&snapshot)? < 17 {
            return Ok((0, 0));
        }
        let (rows, bytes): (i64, i64) = sql(snapshot.query_row(
            "SELECT count(*),coalesce(sum(reserved_bytes),0) FROM io_material_reservations",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ))?;
        Ok((
            u64::try_from(rows).map_err(|_| Error::Integrity)?,
            u64::try_from(bytes).map_err(|_| Error::Integrity)?,
        ))
    }
}
#[cfg(all(test, not(target_arch = "wasm32")))]
mod disk_tests {
    use super::*;
    use crate::{
        io_intent::{Command, Record},
        plugin_package::io,
    };
    use sha2::{Digest, Sha256};
    #[test]
    fn full_disk_rolls_material_storage_back_without_partial_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("full.db");
        let mut store = Store::open(&path, Default::default()).unwrap();
        // Incompressible so the stored container really needs new database
        // pages; a run of identical bytes would compress into one free page.
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let payload: Vec<u8> = (0..512 * 1024)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                (state >> 33) as u8
            })
            .collect();
        let payload_sha256 = <[u8; 32]>::from(Sha256::digest(&payload));
        let command = Command {
            operation_id: "material".into(),
            subject: "plugin.io-test".into(),
            package_sha256: [1; 32],
            capability: io::IoCapability::HttpRequest,
            protocol_sha256: io::schema_digest(),
            request_sha256: payload_sha256,
            approval_sha256: [3; 32],
            target_sha256: [4; 32],
            request_bytes: payload.len() as u64,
            response_limit: 512 * 1024,
        };
        let prepared = Record::prepared(command.clone()).unwrap();
        store
            .append_io_intent_local_authorized(&prepared, || Ok(()))
            .unwrap();
        store.reserve_io_materials(&command, || Ok(())).unwrap();
        let before = store.io_material_reservation_usage().unwrap();
        let material = Material::encode(
            Kind::Request,
            "material",
            "plugin.io-test",
            payload_sha256,
            &payload,
        )
        .unwrap();
        let pages: i64 = store
            .connection
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        store
            .connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        assert_eq!(
            store.store_io_material("plugin.io-test", Kind::Request, &material, || Ok(())),
            Err(Error::StorageFull)
        );
        assert_eq!(store.io_material_reservation_usage().unwrap(), before);
        assert!(matches!(
            store.io_material("plugin.io-test", "material", Kind::Request),
            Err(Error::EvidenceUnavailable)
        ));
        drop(store);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(store.io_material_reservation_usage().unwrap(), before);
        store.integrity_check().unwrap();
    }
}
