//! Finite metadata materialization from permanent card operation events.
//!
//! Private SQL positions never enter metadata, public cursors, or diagnostics.
//! Budgets bound logical bytes read/declared decoded and retained wire bytes;
//! transaction decoding can have additional temporary allocations. These are
//! not a proof of peak allocator usage. No transaction survives materialization.
use super::{Store, service_authority, sql};
use crate::{
    Error, Result,
    changes_metadata::{self, Change, Metadata},
    transaction,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

pub const MAX_CHANGES_CANDIDATES: u32 = 4096;
pub const MAX_CHANGES_PAGE_CANDIDATES: u16 = 64;
pub const MAX_CHANGES_EVENTS: u16 = 64;
pub const MAX_CHANGES_METADATA_BYTES: u64 = 64 * 1024;
pub const MAX_CHANGES_READ_BYTES: u64 = 32 * 1024 * 1024;
const MAX_CONTAINER: u64 =
    (transaction::MAX_EVENT_BYTES + transaction::MAX_EVENT_BYTES / 255 + 128) as u64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangesBudget {
    pub max_duration_ms: u64,
    pub max_candidates: u32,
    pub page_candidates: u16,
    pub max_container_bytes: u64,
    pub max_decoded_bytes: u64,
    pub max_page_container_bytes: u64,
    pub max_page_decoded_bytes: u64,
    pub max_single_decoded_bytes: u32,
    pub max_events: u16,
    pub max_metadata_bytes: u64,
}
impl Default for ChangesBudget {
    fn default() -> Self {
        Self {
            max_duration_ms: 30_000,
            max_candidates: MAX_CHANGES_CANDIDATES,
            page_candidates: MAX_CHANGES_PAGE_CANDIDATES,
            max_container_bytes: MAX_CHANGES_READ_BYTES,
            max_decoded_bytes: MAX_CHANGES_READ_BYTES,
            max_page_container_bytes: MAX_CHANGES_READ_BYTES,
            max_page_decoded_bytes: MAX_CHANGES_READ_BYTES,
            max_single_decoded_bytes: transaction::MAX_EVENT_BYTES as u32,
            max_events: MAX_CHANGES_EVENTS,
            max_metadata_bytes: MAX_CHANGES_METADATA_BYTES,
        }
    }
}
impl ChangesBudget {
    pub fn validate(&self) -> Result<()> {
        if !(1..=60_000).contains(&self.max_duration_ms)
            || !(1..=MAX_CHANGES_CANDIDATES).contains(&self.max_candidates)
            || !(1..=MAX_CHANGES_PAGE_CANDIDATES).contains(&self.page_candidates)
            || !(1..=MAX_CHANGES_EVENTS).contains(&self.max_events)
            || !(1..=MAX_CHANGES_METADATA_BYTES).contains(&self.max_metadata_bytes)
            || !(1..=transaction::MAX_EVENT_BYTES as u32).contains(&self.max_single_decoded_bytes)
            || [
                self.max_container_bytes,
                self.max_decoded_bytes,
                self.max_page_container_bytes,
                self.max_page_decoded_bytes,
            ]
            .iter()
            .any(|v| !(1..=MAX_CHANGES_READ_BYTES).contains(v))
        {
            return Err(Error::Limit);
        }
        Ok(())
    }
}
/// Only a fresh host decision or a previously verified committed ACK may select
/// the start. An After value alone confers no authority; the caller approves it.
pub enum ChangesStart {
    Beginning,
    After(Metadata),
}
#[derive(Default)]
struct Usage {
    candidates: u32,
    container: u64,
    decoded: u64,
    metadata: u64,
}
/// Single-use, nonserializable and nonclone, bound to the original live Store.
/// This deliberately has no Debug implementation exposing internal positions.
pub struct ChangesWindow {
    owner: Arc<()>,
    liveness: Weak<()>,
    store_id: [u8; 32],
    package: [u8; 32],
    cards: Vec<String>,
    scope: [u8; 32],
    lower: i64,
    upper: i64,
    budget: ChangesBudget,
    usage: Usage,
    deadline: Instant,
}
/// Fully owned approved facts. No SQL connection/transaction/borrow remains.
/// This deliberately cannot be constructed or cloned outside Core.
pub struct ChangesBatch {
    owner: Arc<()>,
    liveness: Weak<()>,
    store_id: [u8; 32],
    package: [u8; 32],
    cards: Vec<String>,
    scope: [u8; 32],
    budget: ChangesBudget,
    changes: Vec<Change>,
    deadline: Instant,
}
/// Read-only original Store identity and lifetime; not authority, serializable,
/// or a way to keep the Store alive. Logical identity may survive a DB copy.
pub struct ChangesStoreBinding {
    liveness: Weak<()>,
    store_id: [u8; 32],
}
impl ChangesStoreBinding {
    /// Pure bounded lifetime observation, safe in a final ACK guard.
    pub fn check_live(&self) -> Result<()> {
        live(&self.liveness)
    }
}
fn live(owner: &Weak<()>) -> Result<()> {
    if owner.upgrade().is_none() {
        Err(Error::Invalid("inactive changes Store"))
    } else {
        Ok(())
    }
}
impl ChangesWindow {
    pub fn scope_digest(&self) -> [u8; 32] {
        self.scope
    }
    pub fn card_ids(&self) -> &[String] {
        &self.cards
    }
    pub fn package_digest(&self) -> [u8; 32] {
        self.package
    }
    pub fn budget(&self) -> ChangesBudget {
        self.budget
    }
}
impl ChangesBatch {
    pub fn scope_digest(&self) -> [u8; 32] {
        self.scope
    }
    pub fn card_ids(&self) -> &[String] {
        &self.cards
    }
    pub fn package_digest(&self) -> [u8; 32] {
        self.package
    }
    pub fn budget(&self) -> ChangesBudget {
        self.budget
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }
    pub fn into_metadata(self, window_id: [u8; 32]) -> Result<Vec<Metadata>> {
        check_deadline(self.deadline)?;
        live(&self.liveness)?;
        if window_id == [0; 32] {
            return Err(Error::Invalid("changes window identity"));
        }
        self.changes
            .into_iter()
            .map(|v| v.notification(self.scope, window_id))
            .collect()
    }
}
fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        Err(Error::Invalid("changes window expired"))
    } else {
        Ok(())
    }
}
fn durable(c: &Connection) -> Result<[u8; 32]> {
    service_authority::store_identity(c)?.ok_or(Error::UnsupportedVersion)
}
fn charge(value: &mut u64, additional: u64, limit: u64) -> Result<()> {
    *value = value.checked_add(additional).ok_or(Error::Limit)?;
    if *value > limit {
        return Err(Error::Limit);
    }
    Ok(())
}
/// Header and lengths are copied only in fixed/bounded form before the complete
/// blob is fetched. SQLite length/CASE checks precede any rusqlite allocation.
fn receipt(
    c: &Connection,
    operation: &str,
    card: &str,
    budget: ChangesBudget,
    usage: &mut Usage,
    page: &mut Usage,
    authorize: &mut impl FnMut() -> Result<()>,
) -> Result<transaction::Receipt> {
    authorize()?;
    let (length,header):(i64,Vec<u8>)=sql(c.query_row(
        "SELECT length(payload),substr(payload,1,18) FROM operations WHERE id=?1 AND card_id=?2 AND object_kind=0",
        params![operation,card],|r|Ok((r.get(0)?,r.get(1)?))))?;
    if length < 50
        || length as u64 > MAX_CONTAINER
        || header.len() != 18
        || &header[..8] != b"MORROWT1"
        || u16::from_le_bytes(header[8..10].try_into().unwrap()) != 1
    {
        return Err(Error::Integrity);
    }
    let raw = u32::from_le_bytes(header[10..14].try_into().unwrap()) as u64;
    let packed = u32::from_le_bytes(header[14..18].try_into().unwrap()) as u64;
    if packed + 50 != length as u64 {
        return Err(Error::Integrity);
    }
    if raw > u64::from(budget.max_single_decoded_bytes) {
        return Err(Error::Limit);
    }
    charge(
        &mut usage.container,
        length as u64,
        budget.max_container_bytes,
    )?;
    charge(&mut usage.decoded, raw, budget.max_decoded_bytes)?;
    charge(
        &mut page.container,
        length as u64,
        budget.max_page_container_bytes,
    )?;
    charge(&mut page.decoded, raw, budget.max_page_decoded_bytes)?;
    authorize()?;
    let mut stmt=sql(c.prepare("SELECT CASE WHEN length(payload)=?3 THEN payload ELSE NULL END FROM operations WHERE id=?1 AND card_id=?2 AND object_kind=0"))?;
    let mut rows = sql(stmt.query(params![operation, card, length]))?;
    let row = sql(rows.next())?.ok_or(Error::Integrity)?;
    let bytes = sql(row.get_ref(0))?;
    let bytes = bytes.as_blob().map_err(|_| Error::Integrity)?;
    if bytes.len() != length as usize {
        return Err(Error::Integrity);
    }
    // Fetching may block; authority must still be current before costly decode.
    authorize()?;
    let (_, receipt) = transaction::decode_commit(bytes)?;
    if receipt.operation_id != operation
        || receipt.card_id != card
        || receipt.event_id != operation
        || receipt.revision == 0
    {
        return Err(Error::Integrity);
    }
    authorize()?;
    Ok(receipt)
}
pub(super) fn open_connection(
    c: &Connection,
    owner: Arc<()>,
    liveness: Weak<()>,
    package: [u8; 32],
    cards: &[String],
    start: ChangesStart,
    budget: ChangesBudget,
    authorize: &mut impl FnMut() -> Result<()>,
) -> Result<ChangesWindow> {
    budget.validate()?;
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(budget.max_duration_ms))
        .ok_or(Error::Limit)?;
    let mut checked = || {
        check_deadline(deadline)?;
        live(&liveness)?;
        authorize()?;
        check_deadline(deadline)
    };
    let authorize = &mut checked;
    let cards = changes_metadata::canonical_cards(cards)?;
    authorize()?;
    let store_id = durable(c)?;
    let scope = changes_metadata::scope_digest(store_id, package, &cards)?;
    let mut usage = Usage::default();
    let lower = match start {
        ChangesStart::Beginning => 0,
        ChangesStart::After(anchor) => {
            anchor.validate()?;
            if anchor.scope_digest != scope || cards.binary_search(&anchor.card_id).is_err() {
                return Err(Error::Invalid("foreign changes anchor"));
            }
            authorize()?;
            let position:Option<i64>=sql(c.query_row(
                "SELECT e.sequence FROM operations o JOIN operation_events e ON e.id=o.id WHERE o.id=?1 AND o.card_id=?2 AND o.object_kind=0",
                params![&anchor.operation_id,&anchor.card_id],|r|r.get(0)).optional())?;
            let position = position.ok_or(Error::NotFound)?;
            if position <= 0 {
                return Err(Error::Integrity);
            }
            usage.candidates = 1;
            let r = receipt(
                c,
                &anchor.operation_id,
                &anchor.card_id,
                budget,
                &mut usage,
                &mut Usage::default(),
                authorize,
            )?;
            if r.revision != anchor.revision || r.content_sha256 != anchor.card_sha256 {
                return Err(Error::Integrity);
            }
            position
        }
    };
    let upper: i64 = sql(c.query_row(
        "SELECT coalesce(max(sequence),0) FROM operation_events",
        [],
        |r| r.get(0),
    ))?;
    if upper < lower {
        return Err(Error::Integrity);
    }
    // Permanent events have contiguous positions. Reject too-large windows,
    // rather than calling a truncated scan successful or filtering before LIMIT.
    if (upper - lower) as u64 + u64::from(usage.candidates) > u64::from(budget.max_candidates) {
        return Err(Error::Limit);
    }
    authorize()?;
    Ok(ChangesWindow {
        owner,
        liveness,
        store_id,
        package,
        cards,
        scope,
        lower,
        upper,
        budget,
        usage,
        deadline,
    })
}
pub(super) fn materialize_connection(
    c: &Connection,
    owner: &Arc<()>,
    mut w: ChangesWindow,
    authorize: &mut impl FnMut() -> Result<()>,
) -> Result<ChangesBatch> {
    if !Arc::ptr_eq(owner, &w.owner) {
        return Err(Error::Invalid("foreign changes window"));
    }
    let mut checked = || {
        check_deadline(w.deadline)?;
        live(&w.liveness)?;
        authorize()?;
        check_deadline(w.deadline)
    };
    let authorize = &mut checked;
    authorize()?;
    if durable(c)? != w.store_id {
        return Err(Error::Invalid("foreign changes Store"));
    }
    let mut changes = Vec::new();
    while w.lower < w.upper {
        authorize()?;
        let mut page = Usage::default();
        // First collect at most one bounded page of permanent candidates. No
        // card filter or operations payload participates in this range scan.
        let candidates = {
            let mut stmt=sql(c.prepare("SELECT sequence,CASE WHEN length(CAST(id AS BLOB)) BETWEEN 1 AND 256 THEN id ELSE NULL END FROM operation_events WHERE sequence>?1 AND sequence<=?2 ORDER BY sequence LIMIT ?3"))?;
            let mut rows = sql(stmt.query(params![
                w.lower,
                w.upper,
                i64::from(w.budget.page_candidates)
            ]))?;
            let mut values = Vec::with_capacity(usize::from(w.budget.page_candidates));
            while let Some(row) = sql(rows.next())? {
                let seq: i64 = sql(row.get(0))?;
                let id: Option<String> = sql(row.get(1))?;
                let id = id.ok_or(Error::Integrity)?;
                crate::identity(&id)?;
                values.push((seq, id));
            }
            values
        };
        if candidates.is_empty() {
            return Err(Error::Integrity);
        }
        for (seq, operation) in candidates {
            if seq != w.lower.checked_add(1).ok_or(Error::Limit)? {
                return Err(Error::Integrity);
            }
            w.usage.candidates = w.usage.candidates.checked_add(1).ok_or(Error::Limit)?;
            if w.usage.candidates > w.budget.max_candidates {
                return Err(Error::Limit);
            }
            authorize()?;
            let row:Option<(i64,Option<String>)>=sql(c.query_row(
                "SELECT object_kind,CASE WHEN length(CAST(card_id AS BLOB)) BETWEEN 1 AND 256 THEN card_id ELSE NULL END FROM operations WHERE id=?1",
                [&operation],|r|Ok((r.get(0)?,r.get(1)?))).optional())?;
            let (kind, card) = row.ok_or(Error::Integrity)?;
            // Noncard and unselected candidates are never decoded as payloads.
            if kind == 0 {
                let card = card.ok_or(Error::Integrity)?;
                crate::identity(&card)?;
                if w.cards.binary_search(&card).is_ok() {
                    if changes.len() >= usize::from(w.budget.max_events) {
                        return Err(Error::Limit);
                    }
                    let metadata_bytes =
                        (changes_metadata::HEADER_BYTES + card.len() + operation.len()) as u64;
                    charge(
                        &mut w.usage.metadata,
                        metadata_bytes,
                        w.budget.max_metadata_bytes,
                    )?;
                    let r = receipt(
                        c,
                        &operation,
                        &card,
                        w.budget,
                        &mut w.usage,
                        &mut page,
                        authorize,
                    )?;
                    changes.push(Change {
                        card_id: card,
                        operation_id: operation,
                        revision: r.revision,
                        card_sha256: r.content_sha256,
                    });
                }
            }
            w.lower = seq;
        }
        authorize()?;
    }
    authorize()?;
    Ok(ChangesBatch {
        owner: w.owner,
        liveness: w.liveness,
        store_id: w.store_id,
        package: w.package,
        cards: w.cards,
        scope: w.scope,
        budget: w.budget,
        changes,
        deadline: w.deadline,
    })
}
impl Store {
    pub fn changes_store_binding(&self) -> Result<ChangesStoreBinding> {
        Ok(ChangesStoreBinding {
            liveness: Arc::downgrade(&self.changes_liveness),
            store_id: durable(&self.connection)?,
        })
    }
    /// Run before acquiring queue/final transaction locks. The caller must
    /// permanently reject the source on mismatch, even if an old Store returns.
    pub fn validate_changes_store_binding(&self, binding: &ChangesStoreBinding) -> Result<()> {
        binding.check_live()?;
        if !Weak::ptr_eq(&Arc::downgrade(&self.changes_liveness), &binding.liveness)
            || durable(&self.connection)? != binding.store_id
        {
            return Err(Error::Invalid("foreign changes Store binding"));
        }
        Ok(())
    }
    pub fn changes_scope_digest(&self, package: [u8; 32], cards: &[String]) -> Result<[u8; 32]> {
        changes_metadata::scope_digest(durable(&self.connection)?, package, cards)
    }
    pub fn open_changes_window(
        &self,
        package: [u8; 32],
        cards: &[String],
        start: ChangesStart,
        budget: ChangesBudget,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<ChangesWindow> {
        let snapshot = sql(self.connection.unchecked_transaction())?;
        let result = open_connection(
            &snapshot,
            self.snapshot_identity.clone(),
            Arc::downgrade(&self.changes_liveness),
            package,
            cards,
            start,
            budget,
            &mut authorize,
        )?;
        sql(snapshot.rollback())?;
        Ok(result)
    }
    pub fn materialize_changes_window(
        &self,
        window: ChangesWindow,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<ChangesBatch> {
        if !Arc::ptr_eq(&self.snapshot_identity, &window.owner) {
            return Err(Error::Invalid("foreign changes window"));
        }
        let snapshot = sql(self.connection.unchecked_transaction())?;
        let result =
            materialize_connection(&snapshot, &self.snapshot_identity, window, &mut authorize)?;
        sql(snapshot.rollback())?;
        Ok(result)
    }
    pub fn validate_changes_batch(
        &self,
        batch: &ChangesBatch,
        package: [u8; 32],
        cards: &[String],
    ) -> Result<()> {
        check_deadline(batch.deadline)?;
        live(&batch.liveness)?;
        if !Arc::ptr_eq(&self.snapshot_identity, &batch.owner)
            || batch.package != package
            || changes_metadata::canonical_cards(cards)? != batch.cards
            || durable(&self.connection)? != batch.store_id
            || self.changes_scope_digest(package, cards)? != batch.scope
        {
            return Err(Error::Invalid("foreign changes batch"));
        }
        Ok(())
    }
}
