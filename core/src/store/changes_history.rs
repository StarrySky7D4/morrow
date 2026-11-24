//! Cross-window checkpoints for reviewed card history. The two retained epochs
//! are isolated from other channels. Permanent card events remain the source.
use super::{Store, channel_journal::ChannelCheckpoint, sql};
use crate::{
    Error, Result,
    changes_metadata::{ChangesCursor, Metadata, reviewed_subscription},
    channel::Frame,
};
use rusqlite::{Connection, OptionalExtension, params};

const HEAD: &str = "CREATE TABLE changes_history_heads(subscription BLOB PRIMARY KEY CHECK(length(subscription)=32),cursor BLOB NOT NULL CHECK(length(cursor)=73),previous_epoch BLOB CHECK(previous_epoch IS NULL OR length(previous_epoch)=32)) STRICT";
const EVENTS: &str = "CREATE TABLE changes_history_cards(card TEXT NOT NULL,sequence INTEGER NOT NULL CHECK(sequence>0),PRIMARY KEY(card,sequence)) STRICT, WITHOUT ROWID";
const ADD: &str = "CREATE TRIGGER changes_history_card_add AFTER INSERT ON operation_events BEGIN INSERT INTO changes_history_cards(card,sequence) SELECT card_id,NEW.sequence FROM operations WHERE id=NEW.id AND object_kind=0; END";
const REMOVE: &str = "CREATE TRIGGER changes_history_card_remove AFTER DELETE ON operation_events BEGIN DELETE FROM changes_history_cards WHERE card=(SELECT card_id FROM operations WHERE id=OLD.id) AND sequence=OLD.sequence; END";
const DDL: &[(&str, &str)] = &[
    ("changes_history_heads", HEAD),
    ("changes_history_cards", EVENTS),
    ("changes_history_card_add", ADD),
    ("changes_history_card_remove", REMOVE),
];
const HEAD_ROOM: u64 = 32 + 73 + 32;

pub(super) fn create(c: &Connection) -> Result<()> {
    for (_, ddl) in DDL {
        sql(c.execute_batch(ddl))?;
    }
    sql(c.execute("INSERT INTO changes_history_cards(card,sequence) SELECT o.card_id,e.sequence FROM operation_events e JOIN operations o ON o.id=e.id WHERE o.object_kind=0",[]))?;
    Ok(())
}
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
pub(super) fn accounted(c: &Connection) -> Result<u64> {
    if version(c)? < 28 {
        return Ok(0);
    }
    let count: i64 = sql(
        c.query_row("SELECT count(*) FROM changes_history_heads", [], |r| {
            r.get(0)
        }),
    )?;
    u64::try_from(count)
        .map_err(|_| Error::Integrity)?
        .checked_mul(HEAD_ROOM)
        .ok_or(Error::Limit)
}
fn original(receipt: &super::ChannelAckReceipt, subscription: &[u8; 32]) -> Result<ChangesCursor> {
    let frame = Frame::decode(&receipt.frame_wire)?;
    let metadata = Metadata::decode(&frame.bytes)?;
    let checkpoint = &receipt.checkpoint;
    let digest = metadata.cursor()?;
    if reviewed_subscription(metadata.scope_digest) != *subscription
        || metadata.window_id != checkpoint.source_epoch
        || frame.cursor != digest
        || checkpoint.cursor != digest
        || frame.sequence != checkpoint.sequence
        || frame.source_epoch != checkpoint.source_epoch
    {
        return Err(Error::Integrity);
    }
    Ok(ChangesCursor {
        epoch: checkpoint.source_epoch,
        sequence: checkpoint.sequence,
        metadata: digest,
    })
}
fn load(c: &Connection, sub: &[u8; 32]) -> Result<Option<(ChangesCursor, Option<[u8; 32]>)>> {
    let row: Option<(Vec<u8>, Option<Vec<u8>>)> = sql(c
        .query_row(
            "SELECT cursor,previous_epoch FROM changes_history_heads WHERE subscription=?1",
            [sub.as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional())?;
    row.map(|(wire, old)| {
        Ok((
            ChangesCursor::decode(&wire)?,
            old.map(|bytes| bytes.try_into().map_err(|_| Error::Integrity))
                .transpose()?,
        ))
    })
    .transpose()
}
pub(super) fn verify(c: &Connection) -> Result<()> {
    let installed = version(c)? >= 28;
    for &(name, want) in DDL {
        let actual: Option<String> = sql(c
            .query_row("SELECT sql FROM sqlite_schema WHERE name=?1", [name], |r| {
                r.get(0)
            })
            .optional())?;
        if actual.as_deref() != installed.then_some(want) {
            return Err(Error::Integrity);
        }
    }
    if !installed {
        return Ok(());
    }
    let missing:bool=sql(c.query_row("SELECT EXISTS(SELECT 1 FROM operations o JOIN operation_events e ON e.id=o.id LEFT JOIN changes_history_cards h ON h.card=o.card_id AND h.sequence=e.sequence WHERE o.object_kind=0 AND h.sequence IS NULL)",[],|r|r.get(0)))?;
    let invalid:bool=sql(c.query_row("SELECT EXISTS(SELECT 1 FROM changes_history_cards h LEFT JOIN operation_events e ON e.sequence=h.sequence LEFT JOIN operations o ON o.id=e.id WHERE o.id IS NULL OR o.object_kind<>0 OR o.card_id<>h.card)",[],|r|r.get(0)))?;
    if missing || invalid {
        return Err(Error::Integrity);
    }
    let mut stmt = sql(c.prepare("SELECT subscription FROM changes_history_heads"))?;
    let mut keys = sql(stmt.query([]))?;
    while let Some(row) = sql(keys.next())? {
        let sub: Vec<u8> = sql(row.get(0))?;
        let sub: [u8; 32] = sub.try_into().map_err(|_| Error::Integrity)?;
        let (head, previous) = load(c, &sub)?.ok_or(Error::Integrity)?;
        let current =
            super::channel_journal::checkpoint(c, &sub, &head.epoch)?.ok_or(Error::Integrity)?;
        let receipt = super::channel_journal::receipt(c, &sub, &head.epoch, head.sequence)?
            .ok_or(Error::Integrity)?;
        if current != receipt.checkpoint
            || original(&receipt, &sub)? != head
            || previous == Some(head.epoch)
        {
            return Err(Error::Integrity);
        }
        if let Some(prior) = previous
            && super::channel_journal::checkpoint(c, &sub, &prior)?.is_none()
        {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}
impl Store {
    pub fn reviewed_changes_cursor(&self, sub: &[u8; 32]) -> Result<Option<ChangesCursor>> {
        Ok(load(&self.connection, sub)?.map(|(cursor, _)| cursor))
    }
}
/// Merge at most one pending event per reviewed card. Index seeks skip unrelated
/// history without loading private events, including when a library has a long gap.
#[allow(clippy::too_many_arguments)]
pub(super) fn candidates(
    c: &Connection,
    cards: &[String],
    lower: i64,
    upper: i64,
    limit: u16,
    used: &mut u32,
    maximum: u32,
    authorize: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<(i64, String)>> {
    let mut stmt=sql(c.prepare("SELECT h.sequence,CASE WHEN o.object_kind=0 AND o.card_id=h.card AND length(CAST(e.id AS BLOB)) BETWEEN 1 AND 256 THEN e.id ELSE NULL END FROM changes_history_cards h LEFT JOIN operation_events e ON e.sequence=h.sequence LEFT JOIN operations o ON o.id=e.id WHERE h.card=?1 AND h.sequence>?2 AND h.sequence<=?3 ORDER BY h.sequence LIMIT 1"))?;
    let mut pending = std::collections::BTreeMap::new();
    let mut probe = |index: usize,
                     after: i64,
                     pending: &mut std::collections::BTreeMap<i64, (usize, String)>|
     -> Result<()> {
        authorize()?;
        let next: Option<(i64, Option<String>)> = sql(stmt
            .query_row(params![&cards[index], after, upper], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional())?;
        if let Some((sequence, id)) = next {
            *used = used
                .checked_add(1)
                .filter(|n| *n <= maximum)
                .ok_or(Error::Limit)?;
            let id = id.ok_or(Error::Integrity)?;
            crate::identity(&id)?;
            if pending.insert(sequence, (index, id)).is_some() {
                return Err(Error::Integrity);
            }
        }
        Ok(())
    };
    for index in 0..cards.len() {
        probe(index, lower, &mut pending)?;
    }
    let mut selected = Vec::new();
    while let Some((seq, (index, id))) = pending.pop_first() {
        selected.push((seq, id));
        if selected.len() == usize::from(limit) {
            break;
        }
        probe(index, seq, &mut pending)?;
    }
    Ok(selected)
}
/// Write under the same transaction as the original ACK. A concurrent window
/// must still own its exact parent. Retire only a third epoch of this subscription.
pub(super) fn advance(
    c: &Connection,
    budget: super::EventBudget,
    sub: &[u8; 32],
    expected: Option<ChangesCursor>,
    checkpoint: &ChannelCheckpoint,
) -> Result<()> {
    let receipt =
        super::channel_journal::receipt(c, sub, &checkpoint.source_epoch, checkpoint.sequence)?
            .ok_or(Error::Integrity)?;
    if &receipt.checkpoint != checkpoint {
        return Err(Error::Integrity);
    }
    let next = original(&receipt, sub)?;
    let saved = load(c, sub)?;
    if saved.map(|(head, _)| head) != expected {
        return Err(Error::RevisionConflict);
    }
    let previous = match saved {
        Some((head, old)) if head.epoch != next.epoch => {
            if let Some(obsolete) = old {
                if obsolete == next.epoch || old == Some(head.epoch) {
                    return Err(Error::Integrity);
                }
                sql(c.execute(
                    "DELETE FROM channel_ack_receipts WHERE subscription=?1 AND source_epoch=?2",
                    params![sub.as_slice(), obsolete.as_slice()],
                ))?;
                if sql(c.execute(
                    "DELETE FROM channel_checkpoints WHERE subscription=?1 AND source_epoch=?2",
                    params![sub.as_slice(), obsolete.as_slice()],
                ))? != 1
                {
                    return Err(Error::Integrity);
                }
            }
            Some(head.epoch)
        }
        Some((_, old)) => old,
        None => {
            super::byte_room(c, budget, HEAD_ROOM)?;
            None
        }
    };
    sql(c.execute("INSERT INTO changes_history_heads(subscription,cursor,previous_epoch) VALUES(?1,?2,?3) ON CONFLICT(subscription) DO UPDATE SET cursor=excluded.cursor,previous_epoch=excluded.previous_epoch",
        params![sub.as_slice(),next.encode().as_slice(),previous.as_ref().map(|v|v.as_slice())]))?;
    Ok(())
}
