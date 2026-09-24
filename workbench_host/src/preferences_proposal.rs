//! A single bounded, core-owned recovery proposal. It is not current settings.
//! Reading it never executes a plugin or retries a business operation.
use crate::ui_preferences::proto;
use crate::{Result, Workbench, WorkbenchState, now};
use morrow_core::{
    content::CardRecord, content_change::ContentChange, lifecycle::GrantKind, transaction::Lookup,
};
use morrow_workbench_plugin::preferences;
use prost::Message;
use sha2::{Digest, Sha256};
const ID: &str = "morrow-host-preferences-proposal";
const TYPE: &str = "org.morrow.host.preferences-proposal";
const SETTINGS: &str = "morrow-studio-preferences";

impl WorkbenchState {
    fn proposal_record(&self) -> Result<Option<proto::PreferencesProposal>> {
        let Some(card) = self.host.store_local().card(ID)? else {
            return Ok(None);
        };
        if card.summary().type_id != TYPE
            || card.summary().format_version != 1
            || card.body().len() > preferences::MAX_BYTES + 1024
        {
            return Err("unsupported preferences recovery record".into());
        }
        let bytes = card.body();
        let p = proto::PreferencesProposal::decode(bytes.as_slice())?;
        if p.schema_version != 1 || p.encode_to_vec() != bytes || p.sha256.len() != 32 {
            return Err("invalid preferences recovery record".into());
        }
        morrow_core::runtime::Command::ReadSummary {
            request_id: p.operation.clone(),
            card_id: SETTINGS.into(),
        }
        .validate()?;
        if p.active {
            if p.intent.is_empty()
                || p.intent.len() > preferences::MAX_BYTES
                || Sha256::digest(&p.intent).as_slice() != p.sha256
            {
                return Err("invalid preferences recovery digest".into());
            }
            preferences::decode_wire(&p.intent)?;
        } else if !p.intent.is_empty() {
            return Err("inactive preferences recovery body".into());
        }
        Ok(Some(p))
    }

    fn write_proposal(&mut self, p: &proto::PreferencesProposal) -> Result<()> {
        self.write_proposal_as(p, None)
    }

    fn write_proposal_as(
        &mut self,
        p: &proto::PreferencesProposal,
        reserved: Option<&str>,
    ) -> Result<()> {
        let body = p.encode_to_vec();
        if body.len() > preferences::MAX_BYTES + 1024 {
            return Err("preferences recovery budget".into());
        }
        self.host.prepare_write()?;
        let previous = self.host.store_local().card(ID)?;
        let kind = if previous.is_some() {
            GrantKind::EditContent
        } else {
            GrantKind::CreateContent
        };
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| "proposal identity unavailable")?;
        let operation = reserved.map(str::to_owned).unwrap_or_else(|| {
            format!(
                "prefs-journal-{}",
                random
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            )
        });
        let mut connection = self.host.connect()?;
        let start = self.start;
        let result = (|| -> Result<()> {
            let tick = now(start);
            self.host
                .grant(&mut connection, kind, ID, tick.saturating_add(30_000), tick)?;
            if let Some(previous) = previous {
                self.host.edit_content(
                    &connection,
                    &ContentChange {
                        operation_id: operation,
                        card_id: ID.into(),
                        expected_revision: previous.summary().revision,
                        title: "Preferences recovery".into(),
                        body,
                        preview_text: String::new(),
                        attachments: None,
                    },
                    || now(start),
                )?;
            } else {
                let card = CardRecord::new(ID, TYPE, 1, "Preferences recovery", body)?;
                self.host
                    .create_content(&connection, &operation, &card, || now(start))?;
            }
            Ok(())
        })();
        let disconnected = self.host.disconnect(&connection);
        result?;
        disconnected?;
        Ok(())
    }

    pub(crate) fn pending_preferences(&self) -> Result<Option<(String, Vec<u8>)>> {
        Ok(self
            .proposal_record()?
            .filter(|p| p.active)
            .map(|p| (p.operation, p.intent)))
    }

    fn proposal_committed(&self, p: &proto::PreferencesProposal) -> Result<bool> {
        if self
            .host
            .store_local()
            .operation_commit(SETTINGS, &p.operation)?
            .is_none()
        {
            return Ok(false);
        }
        let evidence = self
            .host
            .store_local()
            .operation_evidence(SETTINGS, &p.operation)?;
        if evidence.is_empty() {
            return self.preferences_commit_matches(&p.operation, &p.intent);
        }
        Ok(evidence.len() == 1
            && evidence[0].data().batch.as_ref().is_some_and(|b| {
                b.intent_type == "morrow.studio.preferences-save.v1" && b.intent == p.intent
            }))
    }

    pub(crate) fn preferences_proposal_committed(&self) -> Result<bool> {
        match self.proposal_record()?.filter(|p| p.active) {
            Some(p) => self.proposal_committed(&p),
            None => Ok(false),
        }
    }

    pub(crate) fn preferences_proposal_conflict(&self) -> Result<bool> {
        let Some(p) = self.proposal_record()?.filter(|p| p.active) else {
            return Ok(false);
        };
        Ok(!self.proposal_committed(&p)?
            && (!matches!(
                self.host.store_local().lookup(&p.operation)?,
                Lookup::Absent
            ) || self
                .host
                .store_local()
                .card(SETTINGS)?
                .map_or(0, |c| c.summary().revision)
                != p.base_revision))
    }

    /// Only a serialized local operation proved absent can be abandoned. Reserve
    /// its ID in this same atomic journal commit so a delayed retry cannot revive
    /// it even after the bounded recovery slot has been reused.
    pub(crate) fn abandon_preferences(&mut self, operation: &str, digest: &[u8]) -> Result<()> {
        let Some(mut p) = self.proposal_record()? else {
            return Err("preferences proposal absent".into());
        };
        if p.operation != operation || p.sha256 != digest {
            return Err("preferences abandonment mismatch".into());
        }
        match self.host.store_local().lookup(operation)? {
            Lookup::Committed(r) if !p.active && r.card_id == ID => return Ok(()),
            Lookup::Absent if p.active => {}
            _ => return Err("preferences operation cannot be abandoned".into()),
        }
        p.active = false;
        p.intent.clear();
        self.write_proposal_as(&p, Some(operation))
    }

    pub(crate) fn submit_preferences(
        &mut self,
        operation: &str,
        input: Vec<u8>,
    ) -> Result<Vec<u8>> {
        // Reject bad input before creating durable recovery state.
        let value = preferences::decode_wire(&input)
            .map_err(|_| crate::preferences_evidence::PreferencesRejected)?;
        preferences::validation_pages(&value)
            .map_err(|_| crate::preferences_evidence::PreferencesRejected)?;
        let intent = preferences::encode_wire(&value)?;
        morrow_core::runtime::Command::ReadSummary {
            request_id: operation.into(),
            card_id: SETTINGS.into(),
        }
        .validate()?;
        if let Lookup::Committed(r) = self.host.store_local().lookup(operation)?
            && r.card_id != SETTINGS
        {
            return Err(crate::preferences_evidence::PreferencesRejected.into());
        }
        let p = if let Some(p) = self.proposal_record()?.filter(|p| p.active) {
            if p.operation != operation || p.intent != intent {
                return Err("pending preferences proposal requires reconciliation".into());
            }
            p
        } else {
            // Persist before the business operation can execute, under this same
            // library owner. The original revision cannot drift across restarts.
            let p = proto::PreferencesProposal {
                schema_version: 1,
                operation: operation.into(),
                base_revision: self
                    .host
                    .store_local()
                    .card(SETTINGS)?
                    .map_or(0, |c| c.summary().revision),
                sha256: Sha256::digest(&intent).to_vec(),
                intent,
                active: true,
            };
            self.write_proposal(&p)?;
            p
        };
        if matches!(self.host.store_local().lookup(operation)?, Lookup::Absent)
            && self
                .host
                .store_local()
                .card(SETTINGS)?
                .map_or(0, |c| c.summary().revision)
                != p.base_revision
        {
            return Err("preferences recovery base revision conflict".into());
        }
        // Host-owned settings need no enabled guest; core transactions and the
        // durable proposal still enforce identity, base revision and recovery.
        self.save_local_preferences(operation, p.intent)
    }

    pub(crate) fn acknowledge_preferences(&mut self, operation: &str, digest: &[u8]) -> Result<()> {
        let Some(mut p) = self.proposal_record()? else {
            return Err("preferences proposal absent".into());
        };
        if p.operation != operation || p.sha256 != digest {
            return Err("preferences acknowledgement mismatch".into());
        }
        if !p.active {
            if let Lookup::Committed(r) = self.host.store_local().lookup(operation)?
                && r.card_id == ID
            {
                return Err("preferences operation was abandoned".into());
            }
            return Ok(());
        } // Lost acknowledgement reply is retryable.
        let committed = self.proposal_committed(&p)?;
        let no_change = self
            .host
            .store_local()
            .card(SETTINGS)?
            .map_or(0, |c| c.summary().revision)
            == p.base_revision
            && self.read_preferences()?.as_deref() == Some(p.intent.as_slice());
        if !committed && !no_change {
            return Err("preferences outcome not confirmed".into());
        }
        p.active = false;
        p.intent.clear();
        self.write_proposal(&p)
    }
}

impl Workbench {
    pub fn abandon_preferences(&mut self, operation: &str, digest: &[u8]) -> Result<()> {
        self.local_state_mut()?
            .abandon_preferences(operation, digest)
    }
    pub fn pending_preferences(&self) -> Result<Option<(String, Vec<u8>)>> {
        self.local_state()?.pending_preferences()
    }
    pub fn submit_preferences(&mut self, operation: &str, input: Vec<u8>) -> Result<Vec<u8>> {
        self.local_state_mut()?.submit_preferences(operation, input)
    }
    pub fn acknowledge_preferences(&mut self, operation: &str, digest: &[u8]) -> Result<()> {
        self.local_state_mut()?
            .acknowledge_preferences(operation, digest)
    }
}
