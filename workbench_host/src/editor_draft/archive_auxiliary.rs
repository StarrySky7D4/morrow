//! Read-only bindings for the original auxiliary journals retained by archive.
//! These observations never retire resources or authorize a historical effect.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MAX_ARCHIVED_AUXILIARY: usize = 4096;
// Observation bound only; admission and archive storage retain their separate
// tighter identity/byte budgets. Includes encoded-card overhead and old data.
pub(crate) const MAX_AUXILIARY_SCAN_BYTES: u64 = 192 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Import,
    Decision,
    Proposal,
}

pub(crate) struct Record {
    pub kind: Kind,
    pub card: String,
    pub draft: String,
    pub id: String,
    pub closed: bool,
    bytes: u64,
    digest: [u8; 32],
}
impl Record {
    pub(crate) fn checked(
        kind: Kind,
        card: &str,
        draft: &str,
        source: &CardRecord,
        closed: bool,
    ) -> Self {
        let raw = source.encode();
        Self {
            kind,
            card: card.into(),
            draft: draft.into(),
            id: source.summary().id,
            closed,
            bytes: raw.len() as u64,
            digest: Sha256::digest(raw).into(),
        }
    }
    pub(super) fn export(&self, state: &WorkbenchState) -> Result<serde_json::Value> {
        let source = state
            .host
            .store_local()
            .card(&self.id)?
            .ok_or("auxiliary export source missing")?;
        let raw = source.encode();
        if raw.len() as u64 != self.bytes || Sha256::digest(&raw).as_slice() != self.digest {
            return Err("auxiliary export source changed".into());
        }
        let hex = |b: &[u8]| b.iter().map(|v| format!("{v:02x}")).collect::<String>();
        Ok(serde_json::json!({
            "kind": match self.kind { Kind::Import => "import", Kind::Decision => "decision", Kind::Proposal => "proposal" },
            "journal_id": self.id, "card_id": self.card, "draft_id": self.draft,
            "closed": self.closed, "bytes": self.bytes.to_string(), "sha256": hex(&self.digest),
            "canonical_card_protobuf_hex": hex(&raw), "replay_authority": false,
        }))
    }
}

pub(super) struct Inventory {
    scopes: BTreeMap<String, Vec<Record>>,
}
pub(super) struct Binding {
    pub count: u32,
    pub bytes: u64,
    pub digest: Vec<u8>,
}
impl Inventory {
    pub(super) fn scope(&self, card: &str, draft: &str) -> &[Record] {
        self.scopes
            .get(&key(card, draft))
            .map_or(&[], Vec::as_slice)
    }
    pub(super) fn scope_key(&self, id: &str) -> &[Record] {
        self.scopes.get(id).map_or(&[], Vec::as_slice)
    }
    pub(super) fn binding(&self, card: &str, draft: &str) -> Result<Binding> {
        let records = self.scope(card, draft);
        if records.iter().any(|r| !r.closed) {
            return Err("resolve original auxiliary draft journals before archiving".into());
        }
        let mut hash = Sha256::new();
        hash.update(b"morrow.editor-history-auxiliary.v1\0");
        let mut bytes = 0u64;
        for r in records {
            hash.update([match r.kind {
                Kind::Import => 0,
                Kind::Decision => 1,
                Kind::Proposal => 2,
            }]);
            hash.update((r.id.len() as u64).to_le_bytes());
            hash.update(r.id.as_bytes());
            hash.update(r.bytes.to_le_bytes());
            hash.update(r.digest);
            bytes = bytes
                .checked_add(r.bytes)
                .ok_or("auxiliary history bytes overflow")?;
        }
        Ok(Binding {
            count: records.len().try_into()?,
            bytes,
            digest: hash.finalize().to_vec(),
        })
    }
}
impl WorkbenchState {
    pub(super) fn draft_archive_auxiliary(&self) -> Result<Inventory> {
        let mut records = self.draft_archive_import_records()?;
        records.extend(self.draft_archive_decision_records()?);
        records.extend(self.draft_archive_proposal_records()?);
        if records.len() > MAX_ARCHIVED_AUXILIARY + 3 * MAX_JOURNALS {
            return Err("auxiliary history identity limit".into());
        }
        records.sort_by(|a, b| a.id.cmp(&b.id));
        let mut ids = BTreeSet::new();
        let mut scopes: BTreeMap<String, Vec<Record>> = BTreeMap::new();
        for record in records {
            if !ids.insert(record.id.clone()) {
                return Err("duplicate auxiliary journal identity".into());
            }
            scopes
                .entry(key(&record.card, &record.draft))
                .or_default()
                .push(record);
        }
        Ok(Inventory { scopes })
    }
}
