use super::{Store, blob, boundary, read_commit, sql};
use crate::{
    Error, Result, identity,
    records::{self, Kind, Receipt, Record, proto},
};
use rusqlite::{Connection, TransactionBehavior, params};
pub(super) const SCHEMA: &str = "CREATE TABLE records(kind INTEGER NOT NULL CHECK(kind BETWEEN 1 AND 3),id TEXT NOT NULL,payload BLOB NOT NULL,PRIMARY KEY(kind,id)) STRICT; CREATE INDEX operation_object ON operations(object_kind,card_id);";
fn read(connection: &Connection, kind: Kind, id: &str) -> Result<Option<Record>> {
    let mut statement =
        sql(connection.prepare("SELECT payload FROM records WHERE kind=?1 AND id=?2"))?;
    let mut rows = sql(statement.query(params![kind as u32, id]))?;
    let Some(row) = sql(rows.next())? else {
        return Ok(None);
    };
    let raw = sql(row.get_ref(0))?
        .as_blob()
        .map_err(|_| Error::Integrity)?;
    let record = Record::from_container(kind, raw)?;
    if record.id() != id {
        return Err(Error::Integrity);
    }
    Ok(Some(record))
}
fn references(connection: &Connection, record: &Record) -> Result<()> {
    match record.kind() {
        Kind::Workspace => {}
        Kind::Placement => {
            if read(connection, Kind::Workspace, &record.text("workspace_id"))?.is_none()
                || super::read_card(connection, &record.text("card_id"))?.is_none()
            {
                return Err(Error::NotFound);
            }
        }
        Kind::Draft => {
            let card = super::read_card(connection, &record.text("card_id"))?
                .ok_or(Error::NotFound)?
                .summary();
            if record.number64("base_revision") > card.revision
                || record.text("type_id") != card.type_id
                || record.number32("format_version") != card.format_version
            {
                return Err(Error::RevisionConflict);
            }
        }
    }
    Ok(())
}
impl Store {
    /// Trusted local entry. Not an untrusted plugin command.
    pub fn record_local(&self, kind: Kind, id: &str) -> Result<Option<Record>> {
        identity(id)?;
        read(&self.connection, kind, id)
    }
    pub fn create_record_local(&mut self, operation: &str, record: &Record) -> Result<Receipt> {
        self.apply_record_local(&records::create_command(operation, record)?)
    }
    pub fn patch_record_local(
        &mut self,
        operation: &str,
        kind: Kind,
        id: &str,
        revision: u64,
        patch: proto::Patch,
    ) -> Result<Receipt> {
        self.apply_record_local(&records::patch_command(
            operation, kind, id, revision, patch,
        )?)
    }
    pub fn lookup_record_local(
        &self,
        kind: Kind,
        id: &str,
        operation: &str,
    ) -> Result<Option<Receipt>> {
        identity(id)?;
        identity(operation)?;
        let mut statement = sql(self.connection.prepare(
            "SELECT payload FROM operations WHERE object_kind=?1 AND card_id=?2 AND id=?3",
        ))?;
        let mut rows = sql(statement.query(params![kind as u32, id, operation]))?;
        let Some(row) = sql(rows.next())? else {
            return Ok(None);
        };
        let raw = sql(row.get_ref(0))?
            .as_blob()
            .map_err(|_| Error::Integrity)?;
        let (_, receipt) = records::decode_commit(raw)?;
        if receipt.kind != kind || receipt.object_id != id || receipt.operation_id != operation {
            return Err(Error::Integrity);
        }
        Ok(Some(receipt))
    }
    fn apply_record_local(&mut self, input: &[u8]) -> Result<Receipt> {
        let command = records::decode_command(input)?;
        let kind = Kind::try_from(command.kind)?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        boundary("record-after-begin");
        super::read_capture::reject_tracked(&tx, &command.operation_id)?;
        if let Some(raw) = read_commit(&tx, &command.operation_id)? {
            if !raw.starts_with(b"MORROWR1") {
                return Err(Error::OperationConflict);
            }
            let (old, receipt) = records::decode_commit(&raw)?;
            if old.command != input {
                return Err(Error::OperationConflict);
            }
            return Ok(receipt);
        }
        let current = read(&tx, kind, &command.object_id)?;
        let next = match command
            .action
            .as_ref()
            .ok_or(Error::Invalid("record action"))?
        {
            proto::command::Action::Create(raw) => {
                if current.is_some() {
                    return Err(Error::RevisionConflict);
                }
                Record::decode(kind, raw)?
            }
            proto::command::Action::Patch(p) => current
                .ok_or(Error::NotFound)?
                .patched(command.expected_revision, p)?,
        };
        references(&tx, &next)?;
        let event = records::encode_commit(input.to_vec(), &next)?;
        let receipt = records::decode_commit(&event)?.1;
        super::event_room(&tx, self.budget, event.len() as u64)?;
        sql(tx.execute("INSERT INTO records(kind,id,payload) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET payload=excluded.payload",params![kind as u32,command.object_id,next.container()?]))?;
        boundary("record-after-content");
        sql(tx.execute(
            "INSERT INTO operations(id,card_id,object_kind,payload) VALUES(?1,?2,?3,?4)",
            params![command.operation_id, command.object_id, kind as u32, event],
        ))?;
        boundary("record-after-operation");
        sql(tx.execute(
            "INSERT INTO outbox(id,payload) VALUES(?1,?2)",
            params![command.operation_id, event],
        ))?;
        sql(tx.execute(
            "INSERT INTO operation_events(sequence,id) VALUES(last_insert_rowid(),?1)",
            [&command.operation_id],
        ))?;
        boundary("record-after-event");
        boundary("record-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("record-after-commit");
        Ok(receipt)
    }
}
pub(super) fn verify_operation(
    connection: &Connection,
    kind: i64,
    operation: &str,
    id: &str,
    raw: &[u8],
) -> Result<()> {
    let (_, receipt) = records::decode_commit(raw)?;
    if kind != receipt.kind as i64 || receipt.operation_id != operation || receipt.object_id != id {
        return Err(Error::Integrity);
    }
    if read(connection, receipt.kind, id)?.is_none() {
        return Err(Error::Integrity);
    }
    super::blobs::verify_event(connection, operation, &[])
}
pub(super) fn verify(connection: &Connection) -> Result<()> {
    let mut statement = sql(connection.prepare("SELECT kind,id,payload FROM records"))?;
    let mut rows = sql(statement.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let kind = Kind::try_from(sql(row.get::<_, u32>(0))?)?;
        let id: String = sql(row.get(1))?;
        let raw = sql(row.get_ref(2))?
            .as_blob()
            .map_err(|_| Error::Integrity)?;
        let record = Record::from_container(kind, raw)?;
        references(connection, &record)?;
        // Kind is a checked enum, interpolated only as an integer; identity remains parameterized.
        let query = format!(
            "SELECT o.payload FROM operations o JOIN operation_events e ON o.id=e.id WHERE o.object_kind={} AND o.card_id=?1 ORDER BY e.sequence DESC LIMIT 1",
            kind as u32
        );
        let event = blob(
            connection,
            &query,
            &id,
            crate::transaction::MAX_EVENT_BYTES + crate::transaction::MAX_EVENT_BYTES / 255 + 128,
        )?
        .ok_or(Error::Integrity)?;
        let (event, receipt) = records::decode_commit(&event)?;
        if receipt.kind != kind
            || receipt.object_id != id
            || record.id() != id
            || record.encode() != event.result
        {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    #[test]
    fn sqlite_full_during_draft_save_preserves_last_durable_body() {
        let dir = tempfile::tempdir().unwrap();
        let mut s =
            Store::open(&dir.path().join("db"), super::super::EventBudget::default()).unwrap();
        s.create_local(
            "card",
            &crate::content::CardRecord::new("card", "type", 1, "", vec![]).unwrap(),
        )
        .unwrap();
        s.create_record_local(
            "draft",
            &Record::draft("draft", "card", 1, "type", 1, vec![1]).unwrap(),
        )
        .unwrap();
        let original = s
            .record_local(Kind::Draft, "draft")
            .unwrap()
            .unwrap()
            .encode();
        let pages: i64 = s
            .connection
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        s.connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        let mut seed = 0x12345678u32;
        let body = (0..512 * 1024)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
            .collect();
        assert_eq!(
            s.patch_record_local(
                "full",
                Kind::Draft,
                "draft",
                1,
                proto::Patch {
                    body: Some(body),
                    ..Default::default()
                }
            ),
            Err(Error::StorageFull)
        );
        assert_eq!(
            s.record_local(Kind::Draft, "draft")
                .unwrap()
                .unwrap()
                .encode(),
            original
        );
        assert!(
            s.lookup_record_local(Kind::Draft, "draft", "full")
                .unwrap()
                .is_none()
        );
        assert_eq!(s.pending(0, 10).unwrap().len(), 2);
        s.integrity_check().unwrap();
    }
}
