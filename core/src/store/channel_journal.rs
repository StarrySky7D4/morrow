//! Host-only subscription history in the original Store. No source or resume authority.
use super::{Store, boundary, byte_room, sql};
use crate::{
    Error, Result,
    channel::{Action, Frame, Request, Response, Status},
    envelope,
};
use prost::Message;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.channel.journal.v1.rs"));
}
pub const MAX_CHANNEL_RECEIPTS: usize = 4096;
const MAX_CHECKPOINT_RAW: usize = 1024;
const MAX_RECEIPT_RAW: usize = 3 * crate::channel::MAX_WIRE_BYTES + MAX_CHECKPOINT_RAW;
const MAX_RECEIPT_CONTAINER: usize = MAX_RECEIPT_RAW + MAX_RECEIPT_RAW / 255 + 128;
const CHECKPOINT_MAGIC: &[u8; 8] = b"MORRCHP1";
const RECEIPT_MAGIC: &[u8; 8] = b"MORRCHA1";
const CHECKPOINT_SCHEMA: &str = "CREATE TABLE channel_checkpoints(subscription BLOB NOT NULL CHECK(length(subscription)=32),source_epoch BLOB NOT NULL CHECK(length(source_epoch)=32),sequence INTEGER NOT NULL CHECK(sequence>0),revision INTEGER NOT NULL CHECK(revision>0),payload BLOB NOT NULL,PRIMARY KEY(subscription,source_epoch)) STRICT";
const RECEIPT_SCHEMA: &str = "CREATE TABLE channel_ack_receipts(subscription BLOB NOT NULL CHECK(length(subscription)=32),source_epoch BLOB NOT NULL CHECK(length(source_epoch)=32),sequence INTEGER NOT NULL CHECK(sequence>0),payload BLOB NOT NULL,PRIMARY KEY(subscription,source_epoch,sequence),FOREIGN KEY(subscription,source_epoch) REFERENCES channel_checkpoints(subscription,source_epoch)) STRICT";
pub(super) fn create(c: &Connection) -> Result<()> {
    sql(c.execute_batch(CHECKPOINT_SCHEMA))?;
    sql(c.execute_batch(RECEIPT_SCHEMA))?;
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelCheckpoint {
    pub subscription: [u8; 32],
    pub source_epoch: [u8; 32],
    pub sequence: u64,
    pub frame_sha256: [u8; 32],
    pub cursor: Vec<u8>,
    pub revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelAckReceipt {
    pub checkpoint: ChannelCheckpoint,
    pub frame_wire: Vec<u8>,
    pub request_wire: Vec<u8>,
    pub response_wire: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelCommit {
    Committed(ChannelCheckpoint),
    Duplicate(ChannelCheckpoint),
}
fn id(value: &[u8; 32]) -> Result<()> {
    if *value == [0; 32] {
        return Err(Error::Invalid("channel journal identity"));
    }
    Ok(())
}
fn fixed(value: &[u8]) -> Result<[u8; 32]> {
    let value = value.try_into().map_err(|_| Error::Integrity)?;
    id(&value)?;
    Ok(value)
}
impl ChannelCheckpoint {
    fn validate(&self) -> Result<()> {
        id(&self.subscription)?;
        id(&self.source_epoch)?;
        id(&self.frame_sha256)?;
        if self.sequence == 0
            || self.sequence > MAX_CHANNEL_RECEIPTS as u64
            || self.revision != self.sequence
            || self.cursor.len() > crate::channel::MAX_CURSOR_BYTES
        {
            return Err(Error::Integrity);
        }
        Ok(())
    }
    fn proto(&self) -> proto::Checkpoint {
        proto::Checkpoint {
            schema_version: 1,
            subscription: self.subscription.to_vec(),
            source_epoch: self.source_epoch.to_vec(),
            sequence: self.sequence,
            frame_sha256: self.frame_sha256.to_vec(),
            cursor: self.cursor.clone(),
            revision: self.revision,
        }
    }
    fn from_proto(value: proto::Checkpoint) -> Result<Self> {
        if value.schema_version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        let value = Self {
            subscription: fixed(&value.subscription)?,
            source_epoch: fixed(&value.source_epoch)?,
            sequence: value.sequence,
            frame_sha256: fixed(&value.frame_sha256)?,
            cursor: value.cursor,
            revision: value.revision,
        };
        value.validate()?;
        Ok(value)
    }
    fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        envelope::pack(
            CHECKPOINT_MAGIC,
            &self.proto().encode_to_vec(),
            MAX_CHECKPOINT_RAW,
        )
    }
    fn decode(bytes: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(CHECKPOINT_MAGIC, bytes, MAX_CHECKPOINT_RAW)?;
        let value = proto::Checkpoint::decode(raw.as_slice()).map_err(|_| Error::Integrity)?;
        if value.encode_to_vec() != raw {
            return Err(Error::Integrity);
        }
        Self::from_proto(value)
    }
}
impl ChannelAckReceipt {
    fn validate(&self) -> Result<()> {
        self.checkpoint.validate()?;
        let frame = Frame::decode(&self.frame_wire)?;
        let request = Request::decode(&self.request_wire)?;
        let response = Response::decode(&self.response_wire)?;
        response.validate_for(&request)?;
        let Action::Ack {
            sequence,
            frame_sha256,
            cursor,
        } = &request.action
        else {
            return Err(Error::Invalid("channel journal requires ACK"));
        };
        if response.status != Status::Acked
            || request.source_epoch != frame.source_epoch
            || *sequence != frame.sequence
            || *frame_sha256 != frame.digest()?
            || *cursor != frame.cursor
            || self.checkpoint.sequence != frame.sequence
            || self.checkpoint.source_epoch != frame.source_epoch
            || self.checkpoint.frame_sha256 != *frame_sha256
            || self.checkpoint.cursor != *cursor
        {
            return Err(Error::Integrity);
        }
        Ok(())
    }
    fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let raw = proto::AckReceipt {
            schema_version: 1,
            checkpoint: Some(self.checkpoint.proto()),
            frame_wire: self.frame_wire.clone(),
            request_wire: self.request_wire.clone(),
            response_wire: self.response_wire.clone(),
        }
        .encode_to_vec();
        envelope::pack(RECEIPT_MAGIC, &raw, MAX_RECEIPT_RAW)
    }
    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_RECEIPT_CONTAINER {
            return Err(Error::Limit);
        }
        let raw = envelope::unpack(RECEIPT_MAGIC, bytes, MAX_RECEIPT_RAW)?;
        let value = proto::AckReceipt::decode(raw.as_slice()).map_err(|_| Error::Integrity)?;
        if value.schema_version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        if value.encode_to_vec() != raw {
            return Err(Error::Integrity);
        }
        let value = Self {
            checkpoint: ChannelCheckpoint::from_proto(value.checkpoint.ok_or(Error::Integrity)?)?,
            frame_wire: value.frame_wire,
            request_wire: value.request_wire,
            response_wire: value.response_wire,
        };
        value.validate()?;
        Ok(value)
    }
}
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
pub(super) fn accounted(c: &Connection) -> Result<u64> {
    if version(c)? < 24 {
        return Ok(0);
    }
    let bytes: i64 = sql(c.query_row("SELECT (SELECT coalesce(sum(length(payload)),0) FROM channel_checkpoints)+(SELECT coalesce(sum(length(payload)),0) FROM channel_ack_receipts)", [], |r| r.get(0)))?;
    u64::try_from(bytes).map_err(|_| Error::Integrity)
}
pub(super) fn checkpoint(
    c: &Connection,
    subscription: &[u8; 32],
    epoch: &[u8; 32],
) -> Result<Option<ChannelCheckpoint>> {
    id(subscription)?;
    id(epoch)?;
    if version(c)? < 24 {
        return Ok(None);
    }
    let value: Option<(i64,i64,Option<Vec<u8>>)> = sql(c.query_row("SELECT sequence,revision,CASE WHEN length(payload)<=?3 THEN payload ELSE NULL END FROM channel_checkpoints WHERE subscription=?1 AND source_epoch=?2",params![subscription.as_slice(),epoch.as_slice(),MAX_CHECKPOINT_RAW as i64+128],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional())?;
    let Some((sequence, revision, bytes)) = value else {
        return Ok(None);
    };
    let value = ChannelCheckpoint::decode(&bytes.ok_or(Error::Integrity)?)?;
    if value.subscription != *subscription
        || value.source_epoch != *epoch
        || i64::try_from(value.sequence).ok() != Some(sequence)
        || i64::try_from(value.revision).ok() != Some(revision)
    {
        return Err(Error::Integrity);
    }
    Ok(Some(value))
}
pub(super) fn receipt(
    c: &Connection,
    subscription: &[u8; 32],
    epoch: &[u8; 32],
    sequence: u64,
) -> Result<Option<ChannelAckReceipt>> {
    id(subscription)?;
    id(epoch)?;
    if sequence == 0 || sequence > MAX_CHANNEL_RECEIPTS as u64 {
        return Err(Error::Limit);
    }
    if version(c)? < 24 {
        return Ok(None);
    }
    let bytes: Option<Option<Vec<u8>>> = sql(c.query_row("SELECT CASE WHEN length(payload)<=?4 THEN payload ELSE NULL END FROM channel_ack_receipts WHERE subscription=?1 AND source_epoch=?2 AND sequence=?3",params![subscription.as_slice(),epoch.as_slice(),sequence as i64,MAX_RECEIPT_CONTAINER as i64],|r|r.get(0)).optional())?;
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    let value = ChannelAckReceipt::decode(&bytes.ok_or(Error::Integrity)?)?;
    if value.checkpoint.subscription != *subscription
        || value.checkpoint.source_epoch != *epoch
        || value.checkpoint.sequence != sequence
    {
        return Err(Error::Integrity);
    }
    Ok(Some(value))
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    let version = version(c)?;
    for (name, expected) in [
        ("channel_checkpoints", CHECKPOINT_SCHEMA),
        ("channel_ack_receipts", RECEIPT_SCHEMA),
    ] {
        let stored: Option<(String, String)> = sql(c
            .query_row(
                "SELECT type,sql FROM sqlite_schema WHERE name=?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional())?;
        if version < 24 {
            if stored.is_some() {
                return Err(Error::Integrity);
            }
        } else if stored != Some(("table".into(), expected.into())) {
            return Err(Error::Integrity);
        }
    }
    if version < 24 {
        return Ok(());
    }
    let count: i64 = sql(
        c.query_row("SELECT count(*) FROM channel_ack_receipts", [], |r| {
            r.get(0)
        }),
    )?;
    if count > MAX_CHANNEL_RECEIPTS as i64 {
        return Err(Error::Limit);
    }
    let mut keys =
        sql(c.prepare("SELECT subscription,source_epoch,sequence FROM channel_ack_receipts"))?;
    let mut rows = sql(keys.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let sub = fixed(
            sql(row.get_ref(0))?
                .as_blob()
                .map_err(|_| Error::Integrity)?,
        )?;
        let epoch = fixed(
            sql(row.get_ref(1))?
                .as_blob()
                .map_err(|_| Error::Integrity)?,
        )?;
        let sequence: i64 = sql(row.get(2))?;
        let receipt = receipt(
            c,
            &sub,
            &epoch,
            u64::try_from(sequence).map_err(|_| Error::Integrity)?,
        )?
        .ok_or(Error::Integrity)?;
        let latest = checkpoint(c, &sub, &epoch)?.ok_or(Error::Integrity)?;
        if latest.sequence < receipt.checkpoint.sequence {
            return Err(Error::Integrity);
        }
    }
    let mut keys =
        sql(c.prepare("SELECT subscription,source_epoch FROM channel_checkpoints LIMIT ?1"))?;
    let mut rows = sql(keys.query([MAX_CHANNEL_RECEIPTS as i64 + 1]))?;
    let mut count = 0;
    while let Some(row) = sql(rows.next())? {
        count += 1;
        if count > MAX_CHANNEL_RECEIPTS {
            return Err(Error::Limit);
        }
        let sub = fixed(
            sql(row.get_ref(0))?
                .as_blob()
                .map_err(|_| Error::Integrity)?,
        )?;
        let epoch = fixed(
            sql(row.get_ref(1))?
                .as_blob()
                .map_err(|_| Error::Integrity)?,
        )?;
        let checkpoint = checkpoint(c, &sub, &epoch)?.ok_or(Error::Integrity)?;
        let latest = receipt(c, &sub, &epoch, checkpoint.sequence)?.ok_or(Error::Integrity)?;
        let (count,min,max): (i64,i64,i64) = sql(c.query_row("SELECT count(*),coalesce(min(sequence),0),coalesce(max(sequence),0) FROM channel_ack_receipts WHERE subscription=?1 AND source_epoch=?2",params![sub.as_slice(),epoch.as_slice()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))))?;
        if latest.checkpoint != checkpoint
            || min != 1
            || count != checkpoint.sequence as i64
            || max != count
        {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}
pub(super) struct PreparedAck {
    value: ChannelAckReceipt,
    receipt_bytes: Vec<u8>,
    checkpoint_bytes: Vec<u8>,
}
pub(super) fn prepare_ack(
    c: &Connection,
    subscription: &[u8; 32],
    frame_wire: &[u8],
    request_wire: &[u8],
    response_wire: &[u8],
) -> Result<PreparedAck> {
    id(subscription)?;
    if version(c)? < 24 {
        return Err(Error::UnsupportedVersion);
    }
    let frame = Frame::decode(frame_wire)?;
    let value = ChannelAckReceipt {
        checkpoint: ChannelCheckpoint {
            subscription: *subscription,
            source_epoch: frame.source_epoch,
            sequence: frame.sequence,
            frame_sha256: frame.digest()?,
            cursor: frame.cursor.clone(),
            revision: frame.sequence,
        },
        frame_wire: frame_wire.to_vec(),
        request_wire: request_wire.to_vec(),
        response_wire: response_wire.to_vec(),
    };
    let receipt_bytes = value.encode()?;
    let checkpoint_bytes = value.checkpoint.encode()?;
    Ok(PreparedAck {
        value,
        receipt_bytes,
        checkpoint_bytes,
    })
}

pub(super) fn commit_ack_in_transaction(
    c: &Connection,
    budget: super::EventBudget,
    expected: Option<&ChannelCheckpoint>,
    prepared: PreparedAck,
    mut guard: impl FnMut() -> Result<()>,
) -> Result<super::StoreMutation<ChannelCommit>> {
    let PreparedAck {
        value,
        receipt_bytes,
        checkpoint_bytes,
    } = prepared;
    let subscription = &value.checkpoint.subscription;
    guard()?;
    let current = checkpoint(c, subscription, &value.checkpoint.source_epoch)?;
    if let Some(prior) = receipt(
        c,
        subscription,
        &value.checkpoint.source_epoch,
        value.checkpoint.sequence,
    )? {
        if prior.frame_wire != value.frame_wire
            || prior.checkpoint != value.checkpoint
            || Request::decode(&prior.request_wire)?.reference
                != Request::decode(&value.request_wire)?.reference
        {
            return Err(Error::OperationConflict);
        }
        guard()?;
        return Ok(super::StoreMutation::Unchanged(ChannelCommit::Duplicate(
            current.ok_or(Error::Integrity)?,
        )));
    }
    if current.as_ref() != expected
        || current.as_ref().map_or(1, |v| v.sequence.saturating_add(1)) != value.checkpoint.sequence
    {
        return Err(Error::RevisionConflict);
    }
    let count: i64 = sql(
        c.query_row("SELECT count(*) FROM channel_ack_receipts", [], |r| {
            r.get(0)
        }),
    )?;
    if count >= MAX_CHANNEL_RECEIPTS as i64 {
        return Err(Error::EventCapacity);
    }
    byte_room(
        c,
        budget,
        (receipt_bytes.len() + checkpoint_bytes.len()) as u64,
    )?;
    sql(c.execute("INSERT INTO channel_checkpoints(subscription,source_epoch,sequence,revision,payload) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(subscription,source_epoch) DO UPDATE SET sequence=excluded.sequence,revision=excluded.revision,payload=excluded.payload",params![subscription.as_slice(),value.checkpoint.source_epoch.as_slice(),value.checkpoint.sequence as i64,value.checkpoint.revision as i64,checkpoint_bytes]))?;
    sql(c.execute("INSERT INTO channel_ack_receipts(subscription,source_epoch,sequence,payload) VALUES(?1,?2,?3,?4)",params![subscription.as_slice(),value.checkpoint.source_epoch.as_slice(),value.checkpoint.sequence as i64,receipt_bytes]))?;
    guard()?;
    boundary("channel-checkpoint-before-commit");
    Ok(super::StoreMutation::Commit(ChannelCommit::Committed(
        value.checkpoint,
    )))
}

impl Store {
    /// Durable history, never live source approval or an automatic resume token.
    pub fn channel_checkpoint(
        &self,
        subscription: &[u8; 32],
        source_epoch: &[u8; 32],
    ) -> Result<Option<ChannelCheckpoint>> {
        checkpoint(&self.connection, subscription, source_epoch)
    }
    pub fn channel_ack_receipt(
        &self,
        subscription: &[u8; 32],
        source_epoch: &[u8; 32],
        sequence: u64,
    ) -> Result<Option<ChannelAckReceipt>> {
        receipt(&self.connection, subscription, source_epoch, sequence)
    }
    /// Atomically retain original validated ACK material and CAS the consumption cursor.
    /// Calling this requires the trusted host's subscription/source binding.
    /// CommitUnknown is not an advance confirmation; query the durable row before resuming.
    pub fn commit_channel_ack(
        &mut self,
        subscription: &[u8; 32],
        expected: Option<&ChannelCheckpoint>,
        frame_wire: &[u8],
        request_wire: &[u8],
        response_wire: &[u8],
    ) -> Result<ChannelCommit> {
        self.commit_channel_ack_guarded(
            subscription,
            expected,
            frame_wire,
            request_wire,
            response_wire,
            || Ok(()),
        )
    }
    /// Recheck the original trusted-host authority after acquiring SQLite and
    /// immediately before commit. Rejection rolls back both cursor and receipt.
    /// The guard must not reenter this Store or acquire an already held source lock.
    pub fn commit_channel_ack_guarded(
        &mut self,
        subscription: &[u8; 32],
        expected: Option<&ChannelCheckpoint>,
        frame_wire: &[u8],
        request_wire: &[u8],
        response_wire: &[u8],
        guard: impl FnMut() -> Result<()>,
    ) -> Result<ChannelCommit> {
        let prepared = prepare_ack(
            &self.connection,
            subscription,
            frame_wire,
            request_wire,
            response_wire,
        )?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        match commit_ack_in_transaction(&tx, self.budget, expected, prepared, guard)? {
            super::StoreMutation::Unchanged(value) => Ok(value),
            super::StoreMutation::Commit(value) => {
                tx.commit().map_err(|_| Error::CommitUnknown)?;
                boundary("channel-checkpoint-after-commit");
                Ok(value)
            }
        }
    }
}
