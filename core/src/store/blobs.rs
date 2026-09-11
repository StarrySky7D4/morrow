//! Raw attachment payloads and their references share the content database.
use super::{Store, blob, boundary, sql};
use crate::{
    Error, Result,
    attachment::{self, BlobInfo, RetentionKind},
    content::CardRecord,
    identity,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, blob::ZeroBlob, params};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
const CHUNK: usize = 64 * 1024;
const MAX_TOTAL: i64 = 2 * 1024 * 1024 * 1024;
pub(super) const SCHEMA: &str = "
CREATE TABLE blobs (row INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, digest BLOB NOT NULL, size INTEGER NOT NULL, retired INTEGER, metadata BLOB NOT NULL, payload BLOB NOT NULL) STRICT;
CREATE INDEX blob_digest ON blobs(digest);
CREATE INDEX blob_retired ON blobs(retired);
CREATE TABLE card_blobs (card_id TEXT NOT NULL REFERENCES cards(id), attachment_id TEXT NOT NULL, blob_id TEXT NOT NULL REFERENCES blobs(id), PRIMARY KEY(card_id,attachment_id)) STRICT;
CREATE TABLE event_blobs (event_id TEXT NOT NULL REFERENCES operations(id), blob_id TEXT NOT NULL REFERENCES blobs(id), PRIMARY KEY(event_id,blob_id)) STRICT;
CREATE TABLE retentions (owner TEXT NOT NULL, kind INTEGER NOT NULL, blob_id TEXT NOT NULL REFERENCES blobs(id), metadata BLOB NOT NULL, PRIMARY KEY(owner,kind,blob_id)) STRICT;
CREATE TABLE blob_clock (id INTEGER PRIMARY KEY CHECK(id=1), metadata BLOB NOT NULL) STRICT;";
fn advance_clock(connection: &Connection, now: i64) -> Result<()> {
    let previous = blob(
        connection,
        "SELECT metadata FROM blob_clock WHERE id=?1",
        "1",
        8192,
    )?;
    if now < 0
        || previous
            .as_deref()
            .map(attachment::decode_clock)
            .transpose()?
            .is_some_and(|last| now < last)
    {
        return Err(Error::Invalid("attachment clock regression"));
    }
    sql(connection.execute("INSERT INTO blob_clock(id,metadata) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET metadata=excluded.metadata", [match &previous { Some(raw) => attachment::with_clock(raw,now)?, None => attachment::encode_clock(now)? }]))?;
    Ok(())
}
fn info(connection: &Connection, id: &str) -> Result<(i64, BlobInfo)> {
    identity(id)?;
    let metadata = blob(
        connection,
        "SELECT metadata FROM blobs WHERE id=?1",
        id,
        8192,
    )?
    .ok_or(Error::NotFound)?;
    let value = BlobInfo::decode(&metadata)?;
    let (row, matches, size, retired, length): (i64, bool, i64, Option<i64>, i64) =
        sql(connection.query_row(
            "SELECT row,digest=?2,size,retired,length(payload) FROM blobs WHERE id=?1",
            params![id, value.sha256.as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        ))?;
    if value.id != id
        || !matches
        || size < 0
        || size as u64 != value.byte_length
        || length != size
        || retired != value.retired_at_unix_ms
    {
        return Err(Error::Integrity);
    }
    Ok((row, value))
}
fn save_metadata(connection: &Connection, info: &BlobInfo) -> Result<()> {
    let original = blob(
        connection,
        "SELECT metadata FROM blobs WHERE id=?1",
        &info.id,
        8192,
    )?
    .ok_or(Error::NotFound)?;
    let updated = attachment::with_retirement(&original, info.retired_at_unix_ms)?;
    sql(connection.execute(
        "UPDATE blobs SET metadata=?1, retired=?2 WHERE id=?3",
        params![updated, info.retired_at_unix_ms, info.id],
    ))?;
    Ok(())
}
fn retained(connection: &Connection, id: &str) -> Result<bool> {
    sql(connection.query_row("SELECT EXISTS(SELECT 1 FROM card_blobs WHERE blob_id=?1) OR EXISTS(SELECT 1 FROM event_blobs WHERE blob_id=?1) OR EXISTS(SELECT 1 FROM retentions WHERE blob_id=?1)", [id], |r| r.get(0)))
}
fn stream(
    connection: &Connection,
    row: i64,
    info: &BlobInfo,
    writer: &mut impl Write,
) -> Result<()> {
    let handle = sql(connection.blob_open("main", "blobs", "payload", row, true))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; CHUNK];
    let mut offset = 0;
    while offset < info.byte_length as usize {
        let length = CHUNK.min(info.byte_length as usize - offset);
        sql(handle.read_at_exact(&mut buffer[..length], offset))?;
        hash.update(&buffer[..length]);
        writer.write_all(&buffer[..length]).map_err(|_| Error::Io)?;
        offset += length;
    }
    sql(handle.close())?;
    if <[u8; 32]>::from(hash.finalize()) != info.sha256 {
        return Err(Error::Integrity);
    }
    Ok(())
}
impl Store {
    /// Host-owned reader; the exact staged bytes, length and hash become immutable.
    /// Ready stages stay protected until referenced or explicitly retired, not merely timed out.
    pub fn stage_blob(
        &mut self,
        reader: &mut impl Read,
        byte_length: u64,
        expected: Option<[u8; 32]>,
        now_unix_ms: i64,
    ) -> Result<BlobInfo> {
        if byte_length > attachment::MAX_BLOB_BYTES {
            return Err(Error::Limit);
        }
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        advance_clock(&tx, now_unix_ms)?;
        let (count, total): (i64, i64) = sql(tx.query_row(
            "SELECT count(*),coalesce(sum(size),0) FROM blobs",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ))?;
        if count >= 2048 || total.saturating_add(byte_length as i64) > MAX_TOTAL {
            return Err(Error::Limit);
        }
        let id: String = sql(tx.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0)))?;
        sql(tx.execute(
            "INSERT INTO blobs(id,digest,size,metadata,payload) VALUES(?1,x'',?2,x'',?3)",
            params![id, byte_length as i64, ZeroBlob(byte_length as i32)],
        ))?;
        boundary("stage-after-allocation");
        let row = tx.last_insert_rowid();
        let mut handle = sql(tx.blob_open("main", "blobs", "payload", row, false))?;
        let mut buffer = vec![0; CHUNK];
        let mut offset = 0;
        let mut hash = Sha256::new();
        while offset < byte_length as usize {
            let length = CHUNK.min(byte_length as usize - offset);
            reader
                .read_exact(&mut buffer[..length])
                .map_err(|_| Error::Io)?;
            sql(handle.write_at(&buffer[..length], offset))?;
            hash.update(&buffer[..length]);
            offset += length;
            boundary("stage-after-chunk");
        }
        let mut extra = [0; 1];
        loop {
            match reader.read(&mut extra) {
                Ok(0) => break,
                Ok(_) => return Err(Error::Invalid("source length changed")),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(Error::Io),
            }
        }
        sql(handle.close())?;
        let digest: [u8; 32] = hash.finalize().into();
        if expected.is_some_and(|v| v != digest) {
            return Err(Error::Integrity);
        }
        let existing: Option<String> = sql(tx
            .query_row(
                "SELECT id FROM blobs WHERE digest=?1 AND size=?2 AND id!=?3 ORDER BY row LIMIT 1",
                params![digest.as_slice(), byte_length as i64, id],
                |r| r.get(0),
            )
            .optional())?;
        let value = if let Some(existing) = existing {
            let (row, mut existing) = info(&tx, &existing)?;
            stream(&tx, row, &existing, &mut std::io::sink())?;
            existing.retired_at_unix_ms = None;
            save_metadata(&tx, &existing)?;
            sql(tx.execute("DELETE FROM blobs WHERE id=?1", [&id]))?;
            existing
        } else {
            let value = BlobInfo {
                id,
                sha256: digest,
                byte_length,
                created_at_unix_ms: now_unix_ms,
                retired_at_unix_ms: None,
            };
            sql(tx.execute(
                "UPDATE blobs SET digest=?1,metadata=?2 WHERE row=?3",
                params![digest.as_slice(), value.encode()?, row],
            ))?;
            value
        };
        boundary("stage-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("stage-after-commit");
        Ok(value)
    }
    pub fn blob_info_local(&self, id: &str) -> Result<BlobInfo> {
        let snapshot = sql(self.connection.unchecked_transaction())?;
        Ok(info(&snapshot, id)?.1)
    }
    /// A recovery listing is evidence of durable staging, not permission to delete it.
    pub fn list_blobs_local(&self, after_id: &str, limit: u32) -> Result<Vec<BlobInfo>> {
        if limit == 0 || limit > 128 {
            return Err(Error::Limit);
        }
        let snapshot = sql(self.connection.unchecked_transaction())?;
        let mut statement =
            sql(snapshot.prepare("SELECT id FROM blobs WHERE id>?1 ORDER BY id LIMIT ?2"))?;
        let rows = sql(statement.query_map(params![after_id, limit], |r| r.get::<_, String>(0)))?;
        rows.map(|row| info(&snapshot, &sql(row)?).map(|v| v.1))
            .collect()
    }
    /// Output must not be published until this returns Ok (the digest is checked at the end).
    pub fn export_blob_local(&self, id: &str, writer: &mut impl Write) -> Result<BlobInfo> {
        let snapshot = sql(self.connection.unchecked_transaction())?;
        let (row, value) = info(&snapshot, id)?;
        stream(&snapshot, row, &value, writer)?;
        Ok(value)
    }
    pub fn export_attachment_local(
        &self,
        card_id: &str,
        attachment_id: &str,
        writer: &mut impl Write,
    ) -> Result<BlobInfo> {
        identity(card_id)?;
        identity(attachment_id)?;
        let snapshot = sql(self.connection.unchecked_transaction())?;
        let card = super::read_card(&snapshot, card_id)?.ok_or(Error::NotFound)?;
        let item = card
            .attachments()
            .into_iter()
            .find(|v| v.id == attachment_id)
            .ok_or(Error::NotFound)?;
        let id: String = sql(snapshot.query_row(
            "SELECT blob_id FROM card_blobs WHERE card_id=?1 AND attachment_id=?2",
            params![card_id, attachment_id],
            |r| r.get(0),
        ))?;
        let (row, value) = info(&snapshot, &id)?;
        if value.sha256 != item.sha256 || value.byte_length != item.byte_length {
            return Err(Error::Integrity);
        }
        stream(&snapshot, row, &value, writer)?;
        Ok(value)
    }
    pub fn retain_blob_local(&mut self, id: &str, owner: &str, kind: RetentionKind) -> Result<()> {
        let metadata = attachment::encode_retention(owner, id, kind)?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        let (_, mut value) = info(&tx, id)?;
        value.retired_at_unix_ms = None;
        sql(tx.execute(
            "INSERT OR IGNORE INTO retentions(owner,kind,blob_id,metadata) VALUES(?1,?2,?3,?4)",
            params![owner, kind as i32, id, metadata],
        ))?;
        save_metadata(&tx, &value)?;
        tx.commit().map_err(|_| Error::CommitUnknown)
    }
    pub fn release_retention_local(
        &mut self,
        id: &str,
        owner: &str,
        kind: RetentionKind,
        now: i64,
    ) -> Result<()> {
        attachment::encode_retention(owner, id, kind)?;
        if kind == RetentionKind::Evidence {
            return Err(Error::Retained);
        }
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        advance_clock(&tx, now)?;
        let (_, mut value) = info(&tx, id)?;
        let removed = sql(tx.execute(
            "DELETE FROM retentions WHERE owner=?1 AND kind=?2 AND blob_id=?3",
            params![owner, kind as i32, id],
        ))?;
        if removed == 0 {
            return Err(Error::NotFound);
        }
        if !retained(&tx, id)? {
            value.retired_at_unix_ms = Some(now);
            save_metadata(&tx, &value)?;
        }
        tx.commit().map_err(|_| Error::CommitUnknown)
    }
    pub fn retire_blob_local(&mut self, id: &str, now: i64) -> Result<()> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        advance_clock(&tx, now)?;
        let (_, mut value) = info(&tx, id)?;
        if retained(&tx, id)? {
            return Err(Error::Retained);
        }
        if value.retired_at_unix_ms.is_none() {
            value.retired_at_unix_ms = Some(now);
            save_metadata(&tx, &value)?;
        }
        tx.commit().map_err(|_| Error::CommitUnknown)
    }
    pub fn collect_retired_local(&mut self, now: i64, grace_ms: i64) -> Result<Vec<String>> {
        if grace_ms < attachment::MIN_RETIREMENT_MS {
            return Err(Error::Invalid("retirement grace"));
        }
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        advance_clock(&tx, now)?;
        let cutoff = now.checked_sub(grace_ms).ok_or(Error::Invalid("clock"))?;
        let mut statement =
            sql(tx.prepare("SELECT id FROM blobs WHERE retired<=?1 ORDER BY retired,id LIMIT 16"))?;
        let rows = sql(statement.query_map([cutoff], |r| r.get::<_, String>(0)))?;
        let ids: Vec<String> = rows.map(sql).collect::<Result<_>>()?;
        drop(statement);
        let mut removed = Vec::new();
        for id in ids {
            let (_, value) = info(&tx, &id)?;
            if value.retired_at_unix_ms.is_none_or(|v| v > cutoff) {
                return Err(Error::Integrity);
            }
            if retained(&tx, &id)? {
                return Err(Error::Integrity);
            }
            sql(tx.execute("DELETE FROM blobs WHERE id=?1", [&id]))?;
            removed.push(id);
            boundary("gc-after-delete");
        }
        boundary("gc-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("gc-after-commit");
        Ok(removed)
    }
}
pub(super) fn bind(connection: &Connection, card: &CardRecord, operation: &str) -> Result<()> {
    let card_id = card.summary().id;
    sql(connection.execute("DELETE FROM card_blobs WHERE card_id=?1", [&card_id]))?;
    let mut verified = std::collections::BTreeSet::new();
    for item in card.attachments() {
        let id: Option<String> = sql(connection
            .query_row(
                "SELECT id FROM blobs WHERE digest=?1 AND size=?2 ORDER BY row LIMIT 1",
                params![item.sha256.as_slice(), item.byte_length as i64],
                |r| r.get(0),
            )
            .optional())?;
        let id = id.ok_or(Error::NotFound)?;
        let (row, mut value) = info(connection, &id)?;
        // Revalidate fixed raw bytes inside the committing transaction, before publication.
        if verified.insert(id.clone()) {
            stream(connection, row, &value, &mut std::io::sink())?;
        }
        value.retired_at_unix_ms = None;
        save_metadata(connection, &value)?;
        sql(connection.execute(
            "INSERT INTO card_blobs(card_id,attachment_id,blob_id) VALUES(?1,?2,?3)",
            params![card_id, item.id, id],
        ))?;
        sql(connection.execute(
            "INSERT OR IGNORE INTO event_blobs(event_id,blob_id) VALUES(?1,?2)",
            params![operation, id],
        ))?;
    }
    boundary("after-blob-references");
    Ok(())
}
pub(super) fn verify(connection: &Connection) -> Result<()> {
    let foreign: bool = sql(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
        [],
        |r| r.get(0),
    ))?;
    if foreign {
        return Err(Error::Integrity);
    }
    let mut statement = sql(connection.prepare("SELECT id FROM blobs"))?;
    let rows = sql(statement.query_map([], |r| r.get::<_, String>(0)))?;
    for row in rows {
        let id = sql(row)?;
        let (row, value) = info(connection, &id)?;
        if value.retired_at_unix_ms.is_some() && retained(connection, &id)? {
            return Err(Error::Integrity);
        }
        stream(connection, row, &value, &mut std::io::sink())?;
    }
    let clock = blob(
        connection,
        "SELECT metadata FROM blob_clock WHERE id=?1",
        "1",
        8192,
    )?;
    if let Some(value) = clock {
        attachment::decode_clock(&value)?;
    }
    let mut statement =
        sql(connection.prepare("SELECT owner,kind,blob_id,metadata FROM retentions"))?;
    let mut rows = sql(statement.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let raw = sql(row.get_ref(3))?
            .as_blob()
            .map_err(|_| Error::Integrity)?;
        let value = attachment::decode_retention(raw)?;
        if value.owner_id != sql(row.get::<_, String>(0))?
            || value.kind != sql(row.get::<_, i32>(1))?
            || value.blob_id != sql(row.get::<_, String>(2))?
        {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}
pub(super) fn verify_card(connection: &Connection, card: &CardRecord) -> Result<()> {
    let id = card.summary().id;
    let attachments = card.attachments();
    let count: i64 = sql(connection.query_row(
        "SELECT count(*) FROM card_blobs WHERE card_id=?1",
        [&id],
        |r| r.get(0),
    ))?;
    if count as usize != attachments.len() {
        return Err(Error::Integrity);
    }
    for item in attachments {
        let blob_id: String = sql(connection.query_row(
            "SELECT blob_id FROM card_blobs WHERE card_id=?1 AND attachment_id=?2",
            params![id, item.id],
            |r| r.get(0),
        ))?;
        let (_, value) = info(connection, &blob_id)?;
        if item.sha256 != value.sha256 || item.byte_length != value.byte_length {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}
pub(super) fn verify_event(connection: &Connection, id: &str, digests: &[Vec<u8>]) -> Result<()> {
    let expected: std::collections::BTreeSet<_> = digests.iter().cloned().collect();
    let mut statement = sql(connection.prepare(
        "SELECT b.digest FROM event_blobs e JOIN blobs b ON e.blob_id=b.id WHERE e.event_id=?1",
    ))?;
    let rows = sql(statement.query_map([id], |r| {
        let bytes = r.get_ref(0)?.as_blob()?;
        if bytes.len() != 32 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(bytes.to_vec())
    }))?;
    let actual = rows
        .map(sql)
        .collect::<Result<std::collections::BTreeSet<_>>>()?;
    if expected != actual {
        return Err(Error::Integrity);
    }
    Ok(())
}
