//! Trusted original-owner TLS administration. Only redacted metadata goes to UI.
use crate::{Result, Workbench, WorkbenchState, service_tls::TlsSelection};
use morrow_core::{
    store::{ServiceAuthorityResource, Store},
    tls_identity::Record,
};
use morrow_plugin_runtime::service_authority::AuthorityDependency;

#[derive(Clone, Copy)]
pub struct ProtectedTlsChoice {
    pub reference: [u8; 32],
    pub revision: u64,
    pub certificate_sha256: [u8; 32],
}
pub struct TlsIdentityInfo {
    pub selection: ProtectedTlsChoice,
    pub disabled: bool,
}
pub struct TlsIdentityPage {
    pub entries: Vec<TlsIdentityInfo>,
    pub snapshot: [u8; 32],
    pub next: Option<[u8; 32]>,
}
fn info(record: &Record) -> TlsIdentityInfo {
    TlsIdentityInfo {
        selection: ProtectedTlsChoice {
            reference: record.reference(),
            revision: record.value().revision,
            certificate_sha256: record
                .value()
                .certificate_sha256
                .as_slice()
                .try_into()
                .expect("validated digest"),
        },
        disabled: record.value().disabled,
    }
}
fn reference(bytes: &[u8]) -> Result<[u8; 32]> {
    let value: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "invalid TLS identity reference")?;
    if value == [0; 32] {
        return Err("invalid TLS identity reference".into());
    }
    Ok(value)
}
impl ProtectedTlsChoice {
    pub(crate) fn load(
        &self,
        store: &mut Store,
    ) -> Result<(
        morrow_network_node::server::TlsIdentity,
        crate::tls_validity::TlsValidity,
        AuthorityDependency,
    )> {
        if self.reference == [0; 32] || self.certificate_sha256 == [0; 32] || self.revision == 0 {
            return Err("invalid TLS identity choice".into());
        }
        let guard = store.pin_service_authority()?;
        let record = store
            .load_tls_identity(&self.reference)?
            .ok_or("TLS identity missing")?;
        if record.value().revision != self.revision
            || record.value().certificate_sha256 != self.certificate_sha256
            || record.value().disabled
        {
            return Err("TLS identity changed or disabled".into());
        }
        let lease = store.narrow_service_authority(
            &guard,
            &[ServiceAuthorityResource::TlsIdentity(self.reference)],
        )?;
        let (identity, validity) = crate::service_tls::load_protected(
            &record,
            &store.tls_store_identity()?,
            &self.reference,
            self.revision,
        )?;
        let dependency = AuthorityDependency::new(store, lease, || true)?;
        Ok((identity, validity, dependency))
    }
}
impl WorkbenchState {
    pub fn tls_identity_page(&mut self, after: &[u8], snapshot: &[u8]) -> Result<TlsIdentityPage> {
        let after = if after.is_empty() {
            None
        } else {
            Some(reference(after)?)
        };
        let snapshot = if snapshot.is_empty() {
            None
        } else {
            Some(reference(snapshot)?)
        };

        let page = self
            .host
            .store_local_mut()
            .list_tls_identities_local(after, snapshot, 16)?;

        let entries: Vec<TlsIdentityInfo> = page.records.iter().map(info).collect();

        Ok(TlsIdentityPage {
            entries,
            snapshot: page.snapshot,
            next: page.next,
        })
    }
    pub fn save_tls_identity(
        &mut self,
        requested: &[u8],
        expected: u64,
        selected: &TlsSelection,
    ) -> Result<TlsIdentityInfo> {
        let revision = expected
            .checked_add(1)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or("TLS revision exhausted")?;
        let key = if requested.is_empty() {
            if expected != 0 {
                return Err("new TLS identity requires revision zero".into());
            }
            let mut chosen = None;
            for _ in 0..4 {
                let mut candidate = [0; 32];
                getrandom::fill(&mut candidate)?;
                if candidate != [0; 32]
                    && self
                        .host
                        .store_local()
                        .load_tls_identity(&candidate)?
                        .is_none()
                {
                    chosen = Some(candidate);
                    break;
                }
            }
            chosen.ok_or("TLS identity reference unavailable")?
        } else {
            let key = reference(requested)?;
            let record = self
                .host
                .store_local()
                .load_tls_identity(&key)?
                .ok_or("TLS identity missing")?;
            if record.value().revision != expected {
                return Err("TLS identity changed".into());
            }
            key
        };
        let record =
            selected.protect(self.host.store_local().tls_store_identity()?, key, revision)?;
        self.host.prepare_write()?;
        self.host
            .store_local_mut()
            .save_tls_identity_local(&record, expected)?;
        Ok(info(&record))
    }
    pub fn disable_tls_identity(&mut self, key: &[u8], expected: u64) -> Result<TlsIdentityInfo> {
        let record = self
            .host
            .store_local()
            .load_tls_identity(&reference(key)?)?
            .ok_or("TLS identity missing")?;
        if record.value().revision != expected {
            return Err("TLS identity changed".into());
        }
        if record.value().disabled {
            return Ok(info(&record));
        }
        let mut value = record.value().clone();
        value.revision = value
            .revision
            .checked_add(1)
            .ok_or("TLS revision exhausted")?;
        value.disabled = true;
        let disabled = Record::encode(value)?;
        self.host.prepare_write()?;
        self.host
            .store_local_mut()
            .save_tls_identity_local(&disabled, expected)?;
        Ok(info(&disabled))
    }
}
impl Workbench {
    pub fn tls_identity_page(&mut self, after: &[u8], snapshot: &[u8]) -> Result<TlsIdentityPage> {
        self.local_state_mut()?.tls_identity_page(after, snapshot)
    }
    pub fn save_tls_identity(
        &mut self,
        requested: &[u8],
        expected: u64,
        selected: &TlsSelection,
    ) -> Result<TlsIdentityInfo> {
        self.local_state_mut()?
            .save_tls_identity(requested, expected, selected)
    }
    pub fn disable_tls_identity(&mut self, key: &[u8], expected: u64) -> Result<TlsIdentityInfo> {
        self.local_state_mut()?.disable_tls_identity(key, expected)
    }
}
