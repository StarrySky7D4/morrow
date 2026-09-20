//! Trusted application administration. Only redacted metadata crosses back to UI.
use crate::{Result, Workbench, WorkbenchState};
use morrow_core::outbound_authority::{Record, proto::record::Kind};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct CredentialInfo {
    pub reference: [u8; 32],
    pub revision: u64,
    pub created_ms: u64,
    pub expires_ms: u64,
    pub disabled: bool,
}
pub struct CredentialPage {
    pub entries: Vec<CredentialInfo>,
    pub snapshot: [u8; 32],
    pub next: Option<[u8; 32]>,
}
fn info(record: &Record) -> Result<CredentialInfo> {
    if !matches!(record.value().kind, Some(Kind::Credential(_))) {
        return Err("credential reference has a different record type".into());
    }
    let value = record.value();
    Ok(CredentialInfo {
        reference: record.reference(),
        revision: value.revision,
        created_ms: value.created_ms,
        expires_ms: value.expires_ms,
        disabled: value.disabled,
    })
}
fn reference(bytes: &[u8]) -> Result<[u8; 32]> {
    let value: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "invalid credential reference")?;
    if value == [0; 32] {
        return Err("invalid credential reference".into());
    }
    Ok(value)
}
impl WorkbenchState {
    pub fn credential_page(&mut self, after: &[u8], snapshot: &[u8]) -> Result<CredentialPage> {
        let after = (!after.is_empty()).then(|| reference(after)).transpose()?;
        let snapshot = (!snapshot.is_empty())
            .then(|| {
                snapshot
                    .try_into()
                    .map_err(|_| "invalid credential snapshot")
            })
            .transpose()?;
        let page = self
            .host
            .store_local_mut()
            .list_outbound_authorities_local(after, snapshot, 16)?;
        Ok(CredentialPage {
            entries: page
                .records
                .iter()
                .filter(|r| matches!(r.value().kind, Some(Kind::Credential(_))))
                .map(info)
                .collect::<Result<_>>()?,
            snapshot: page.snapshot,
            next: page.next,
        })
    }
    /// Creation generates a new reference. Replacement requires a new secret and
    /// the original revision; no old plaintext is read or returned.
    pub fn save_credential(
        &mut self,
        requested_reference: &[u8],
        expected_revision: u64,
        header_name: &str,
        header_value: &str,
        lifetime_days: u32,
    ) -> Result<CredentialInfo> {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (
                requested_reference,
                expected_revision,
                header_name,
                header_value,
                lifetime_days,
            );
            Err("protected credentials are unavailable on this platform".into())
        }
        #[cfg(target_os = "windows")]
        {
            if !(1..=30).contains(&lifetime_days) {
                return Err("credential lifetime must be between 1 and 30 days".into());
            }
            let revision = expected_revision
                .checked_add(1)
                .filter(|v| *v <= i64::MAX as u64)
                .ok_or("credential revision exhausted")?;
            let key = if requested_reference.is_empty() {
                if expected_revision != 0 {
                    return Err("new credential requires revision zero".into());
                }
                let mut chosen = None;
                for _ in 0..4 {
                    let mut candidate = [0; 32];
                    getrandom::fill(&mut candidate)?;
                    if candidate != [0; 32]
                        && self
                            .host
                            .store_local()
                            .load_outbound_authority(&candidate)?
                            .is_none()
                    {
                        chosen = Some(candidate);
                        break;
                    }
                }
                chosen.ok_or("credential identity unavailable")?
            } else {
                let key = reference(requested_reference)?;
                let previous = self
                    .host
                    .store_local()
                    .load_outbound_authority(&key)?
                    .ok_or("credential not found")?;
                let previous = info(&previous)?;
                if previous.revision != expected_revision {
                    return Err("credential changed; refresh before replacing".into());
                }
                key
            };
            let created = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
            let expires = created
                .checked_add(u64::from(lifetime_days) * 86_400_000)
                .ok_or("credential expiry overflow")?;
            let record = morrow_audit::credentials::seal(
                key,
                revision,
                created,
                expires,
                header_name,
                header_value,
            )?;
            self.host.prepare_write()?;
            self.host
                .store_local_mut()
                .save_outbound_authority_local(&record, expected_revision)?;
            info(&record)
        }
    }
    /// A disabled tombstone needs no decryption. It cannot be re-enabled without
    /// explicitly replacing the secret under another revision.
    pub fn disable_credential(
        &mut self,
        key: &[u8],
        expected_revision: u64,
    ) -> Result<CredentialInfo> {
        let key = reference(key)?;
        let record = self
            .host
            .store_local()
            .load_outbound_authority(&key)?
            .ok_or("credential not found")?;
        let current = info(&record)?;
        if current.revision != expected_revision {
            return Err("credential changed; refresh before disabling".into());
        }
        if current.disabled {
            return Ok(current);
        }
        let mut value = record.value().clone();
        value.revision = value
            .revision
            .checked_add(1)
            .ok_or("credential revision exhausted")?;
        value.disabled = true;
        let disabled = Record::encode(value)?;
        self.host.prepare_write()?;
        self.host
            .store_local_mut()
            .save_outbound_authority_local(&disabled, expected_revision)?;
        info(&disabled)
    }
}

impl Workbench {
    pub fn credential_page(&mut self, after: &[u8], snapshot: &[u8]) -> Result<CredentialPage> {
        self.local_state_mut()?.credential_page(after, snapshot)
    }
    pub fn save_credential(
        &mut self,
        requested_reference: &[u8],
        expected_revision: u64,
        header_name: &str,
        header_value: &str,
        lifetime_days: u32,
    ) -> Result<CredentialInfo> {
        self.local_state_mut()?.save_credential(
            requested_reference,
            expected_revision,
            header_name,
            header_value,
            lifetime_days,
        )
    }
    pub fn disable_credential(
        &mut self,
        key: &[u8],
        expected_revision: u64,
    ) -> Result<CredentialInfo> {
        self.local_state_mut()?
            .disable_credential(key, expected_revision)
    }
}
