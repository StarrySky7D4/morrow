//! Trusted application policy administration; saving never creates a live grant.
use crate::{Result, Workbench, WorkbenchState};
use morrow_core::{
    outbound_authority::{
        Record, WINDOWS_DPAPI_PROVIDER,
        proto::{self, record::Kind},
    },
    plugin_package::io::IoCapability,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct EndpointInfo {
    pub reference: [u8; 32],
    pub revision: u64,
    pub created_ms: u64,
    pub expires_ms: u64,
    pub disabled: bool,
    pub policy: proto::Endpoint,
}

pub struct EndpointPage {
    pub entries: Vec<EndpointInfo>,
    pub snapshot: [u8; 32],
    pub next: Option<[u8; 32]>,
}
pub struct EndpointUpdate {
    pub reference: Vec<u8>,
    pub expected_revision: u64,
    pub registry_revision: u64,
    pub lifetime_days: u32,
    pub policy: proto::Endpoint,
}
fn info(record: &Record) -> Result<EndpointInfo> {
    let value = record.value();
    let Some(Kind::Endpoint(policy)) = value.kind.as_ref() else {
        return Err("endpoint reference has a different record type".into());
    };
    Ok(EndpointInfo {
        reference: record.reference(),
        revision: value.revision,
        created_ms: value.created_ms,
        expires_ms: value.expires_ms,
        disabled: value.disabled,
        policy: policy.clone(),
    })
}
fn reference(bytes: &[u8]) -> Result<[u8; 32]> {
    let value: [u8; 32] = bytes.try_into().map_err(|_| "invalid endpoint reference")?;
    if value == [0; 32] {
        return Err("invalid endpoint reference".into());
    }
    Ok(value)
}
impl WorkbenchState {
    pub fn endpoint_page(&mut self, after: &[u8], snapshot: &[u8]) -> Result<EndpointPage> {
        let after = (!after.is_empty()).then(|| reference(after)).transpose()?;
        let snapshot = (!snapshot.is_empty())
            .then(|| snapshot.try_into().map_err(|_| "invalid endpoint snapshot"))
            .transpose()?;
        // Two full policies, including DER roots, fit the bounded host frame.
        let page = self
            .host
            .store_local_mut()
            .list_outbound_authorities_local(after, snapshot, 2)?;
        Ok(EndpointPage {
            entries: page
                .records
                .iter()
                .filter(|r| matches!(r.value().kind, Some(Kind::Endpoint(_))))
                .map(info)
                .collect::<Result<_>>()?,
            snapshot: page.snapshot,
            next: page.next,
        })
    }

    pub fn save_endpoint(&mut self, update: EndpointUpdate) -> Result<EndpointInfo> {
        if !(1..=30).contains(&update.lifetime_days) {
            return Err("endpoint lifetime must be between 1 and 30 days".into());
        }
        let revision = update
            .expected_revision
            .checked_add(1)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or("endpoint revision exhausted")?;
        let key = if update.reference.is_empty() {
            if update.expected_revision != 0 {
                return Err("new endpoint requires revision zero".into());
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
            chosen.ok_or("endpoint identity unavailable")?
        } else {
            let key = reference(&update.reference)?;
            let previous = self
                .host
                .store_local()
                .load_outbound_authority(&key)?
                .ok_or("endpoint not found")?;
            let previous = info(&previous)?;
            if previous.revision != update.expected_revision {
                return Err("endpoint changed; refresh before replacing".into());
            }
            if previous.policy.package_id != update.policy.package_id {
                return Err("endpoint package identity cannot change".into());
            }
            key
        };
        let created = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        let expires = created
            .checked_add(u64::from(update.lifetime_days) * 86_400_000)
            .ok_or("endpoint expiry overflow")?;
        let record = Record::encode(proto::Record {
            schema_version: 1,
            reference: key.to_vec(),
            revision,
            created_ms: created,
            expires_ms: expires,
            disabled: false,
            kind: Some(Kind::Endpoint(update.policy)),
        })?;
        let result = info(&record)?;
        let policy = &result.policy;
        let manager = self.manager.as_ref().ok_or("plugin manager unavailable")?;
        if manager.revision() != update.registry_revision {
            return Err("plugin registry changed; refresh before saving".into());
        }
        let selection = manager
            .selection(&policy.package_id)
            .ok_or("plugin is not selected")?;
        if policy.package_sha256.as_slice() != selection.digest {
            return Err("plugin digest changed".into());
        }
        let package = self
            .manager
            .as_ref()
            .ok_or("plugin catalog unavailable")?
            .installed_package(selection.digest)?;
        if package.manifest().package_id != policy.package_id {
            return Err("plugin package identity mismatch".into());
        }
        for capability in [
            Some(IoCapability::HttpRequest),
            (!policy.credential_reference.is_empty()).then_some(IoCapability::CredentialUse),
        ]
        .into_iter()
        .flatten()
        {
            if !selection.approved_io.contains(&capability)
                || !package.io_capabilities().contains(&capability)
            {
                return Err("endpoint requires declared and approved IO capabilities".into());
            }
        }
        morrow_network_node::stored_http::validate_endpoint_policy(policy)?;
        if !policy.credential_reference.is_empty() {
            let key = reference(&policy.credential_reference)?;
            let credential = self
                .host
                .store_local()
                .load_outbound_authority(&key)?
                .ok_or("credential not found")?;
            let Some(Kind::Credential(value)) = credential.value().kind.as_ref() else {
                return Err("credential reference has a different record type".into());
            };
            if !cfg!(target_os = "windows") || value.provider != WINDOWS_DPAPI_PROVIDER {
                return Err("protected credentials are unavailable on this platform".into());
            }
            credential.check_time(created)?;
            if expires > credential.value().expires_ms {
                return Err("endpoint lifetime exceeds credential lifetime".into());
            }
            // No plaintext or DPAPI access: only a bound live instance may decrypt.
        }
        self.host.prepare_write()?;
        self.host
            .store_local_mut()
            .save_outbound_authority_local(&record, update.expected_revision)?;
        Ok(result)
    }

    pub fn disable_endpoint(&mut self, key: &[u8], expected_revision: u64) -> Result<EndpointInfo> {
        let key = reference(key)?;
        let record = self
            .host
            .store_local()
            .load_outbound_authority(&key)?
            .ok_or("endpoint not found")?;
        let current = info(&record)?;
        if current.revision != expected_revision {
            return Err("endpoint changed; refresh before disabling".into());
        }
        if current.disabled {
            return Ok(current);
        }
        let mut value = record.value().clone();
        value.revision = value
            .revision
            .checked_add(1)
            .ok_or("endpoint revision exhausted")?;
        value.disabled = true;
        let disabled = Record::encode(value)?;
        self.host.prepare_write()?;
        self.host
            .store_local_mut()
            .save_outbound_authority_local(&disabled, expected_revision)?;
        info(&disabled)
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use morrow_core::plugin_package::{Package, catalog, io};
    use std::path::Path;

    fn setup(dir: &Path) -> (Workbench, proto::Endpoint) {
        let base = catalog::read_file(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("sdk/compat/guest-v1-rc1/rust-task.mplugin"),
        )
        .unwrap();
        let mut manifest = base.manifest().clone();
        manifest.package_id = "test.endpoint-lease".into();
        manifest.required_features.push(io::FEATURE.into());
        manifest.io_declaration = Some(io::declaration(
            vec![IoCapability::HttpRequest, IoCapability::CredentialUse],
            vec!["api.invoke".into()],
        ));
        let package = Package::build(manifest, base.module()).unwrap();
        let path = dir.join("incoming.mplugin");
        std::fs::write(&path, package.archive()).unwrap();
        let mut w = Workbench::open_managed(dir, None).unwrap();
        w.import_plugin(
            &path,
            &package.digest(),
            w.local_state()
                .unwrap()
                .manager
                .as_ref()
                .unwrap()
                .revision(),
        )
        .unwrap();
        w.configure_external_io(
            &package.manifest().package_id,
            &package.digest(),
            w.local_state()
                .unwrap()
                .manager
                .as_ref()
                .unwrap()
                .revision(),
            &["http-request".into(), "credential-use".into()],
        )
        .unwrap();
        let policy = proto::Endpoint {
            package_id: package.manifest().package_id.clone(),
            package_sha256: package.digest().to_vec(),
            origin: "http://127.0.0.1:32991".into(),
            profile: 2,
            methods: vec!["GET".into()],
            credential_reference: vec![],
            root_certificate: vec![],
            max_request_bytes: 1024,
            max_response_bytes: 2048,
            max_header_bytes: 4096,
            max_concurrent: 1,
            timeout_ms: 1000,
            max_frame_bytes: 8192,
        };
        (w, policy)
    }
    fn update(
        w: &Workbench,
        policy: proto::Endpoint,
        reference: Vec<u8>,
        expected_revision: u64,
    ) -> EndpointUpdate {
        EndpointUpdate {
            reference,
            expected_revision,
            registry_revision: w
                .local_state()
                .unwrap()
                .manager
                .as_ref()
                .unwrap()
                .revision(),
            lifetime_days: 1,
            policy,
        }
    }
    #[test]
    fn update_and_disable_invalidate_original_store_leases_but_rejected_cas_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let (mut w, policy) = setup(dir.path());
        let first = w
            .save_endpoint(update(&w, policy.clone(), vec![], 0))
            .unwrap();
        let lease = w
            .local_state_mut()
            .unwrap()
            .host
            .store_local_mut()
            .pin_service_authority()
            .unwrap();
        assert!(
            w.save_endpoint(update(&w, policy.clone(), first.reference.to_vec(), 0))
                .is_err()
        );
        lease.check().unwrap();
        let second = w
            .save_endpoint(update(&w, policy, first.reference.to_vec(), 1))
            .unwrap();
        assert!(lease.check().is_err());
        let lease = w
            .local_state_mut()
            .unwrap()
            .host
            .store_local_mut()
            .pin_service_authority()
            .unwrap();
        w.disable_endpoint(&second.reference, second.revision)
            .unwrap();
        assert!(lease.check().is_err());
    }
    #[test]
    fn valid_credential_metadata_does_not_trigger_decryption_while_saving() {
        let dir = tempfile::tempdir().unwrap();
        let (mut w, mut policy) = setup(dir.path());
        let now = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        let record = Record::encode(proto::Record {
            schema_version: 1,
            reference: vec![77; 32],
            revision: 1,
            created_ms: now,
            expires_ms: now + 3 * 86_400_000,
            disabled: false,
            kind: Some(Kind::Credential(proto::Credential {
                provider: WINDOWS_DPAPI_PROVIDER.into(),
                ciphertext: vec![9; 32],
            })),
        })
        .unwrap();
        assert!(morrow_audit::credentials::open(&record).is_err());
        w.local_state_mut().unwrap().host.prepare_write().unwrap();
        w.local_state_mut()
            .unwrap()
            .host
            .store_local_mut()
            .save_outbound_authority_local(&record, 0)
            .unwrap();
        policy.credential_reference = record.reference().to_vec();
        // Corrupt protected payload cannot be opened, but metadata-only approval
        // intentionally leaves that decision to the bound live-instance path.
        assert!(w.save_endpoint(update(&w, policy, vec![], 0)).is_ok());
    }

    #[test]
    fn private_protocol_pages_preserve_maximum_historical_roots_within_frame_budget() {
        use crate::{host_capnp as wire, protocol};
        use capnp::{message::Builder, serialize};
        let dir = tempfile::tempdir().unwrap();
        let (mut w, policy) = setup(dir.path());
        w.local_state_mut().unwrap().host.prepare_write().unwrap();
        for byte in 1..=3u8 {
            let mut historical = policy.clone();
            // Core-valid historical metadata, deliberately not valid DER. Reading
            // preserves it without passing live transport approval or renewing it.
            historical.root_certificate =
                (0..32768).map(|i| (i as u8).wrapping_add(byte)).collect();
            let record = Record::encode(proto::Record {
                schema_version: 1,
                reference: vec![byte; 32],
                revision: 1,
                created_ms: 10,
                expires_ms: 1010,
                disabled: byte == 2,
                kind: Some(Kind::Endpoint(historical)),
            })
            .unwrap();
            w.local_state_mut()
                .unwrap()
                .host
                .store_local_mut()
                .save_outbound_authority_local(&record, 0)
                .unwrap();
        }
        let before = w.endpoint_page(&[], &[]).unwrap().snapshot;
        let mut cursor = Vec::new();
        let mut snapshot = Vec::new();
        let mut seen = Vec::new();
        for expected_count in [2, 1] {
            let mut request = Builder::new_default();
            let mut r = request.init_root::<wire::request::Builder>();
            r.set_version(1);
            r.set_digest(&protocol::digest());
            r.set_action(wire::Action::EndpointPage);
            r.set_endpoint_cursor(&cursor);
            r.set_endpoint_snapshot(&snapshot);
            let output =
                protocol::respond(&mut w, &serialize::write_message_to_words(&request)).unwrap();
            assert!(output.len() <= morrow_core::io::MAX_FRAME_BYTES);
            let message = serialize::read_message(&mut &output[..], Default::default()).unwrap();
            let response = message.get_root::<wire::response::Reader>().unwrap();
            assert!(response.get_error().unwrap().is_empty());
            assert_eq!(response.get_endpoint_snapshot().unwrap(), before);
            let entries = response.get_endpoints().unwrap();
            assert_eq!(entries.len(), expected_count);
            for entry in entries.iter() {
                let key = entry.get_reference().unwrap();
                let byte = key[0];
                assert_eq!(key, [byte; 32]);
                assert_eq!(entry.get_revision(), 1);
                assert_eq!(entry.get_created_ms(), 10);
                assert_eq!(entry.get_expires_ms(), 1010);
                assert_eq!(entry.get_disabled(), byte == 2);
                let root = entry.get_policy().unwrap().get_root_certificate().unwrap();
                assert_eq!(root.len(), 32768);
                assert!(
                    root.iter()
                        .enumerate()
                        .all(|(i, actual)| *actual == (i as u8).wrapping_add(byte))
                );
                seen.push(byte);
            }
            cursor = response.get_endpoint_cursor().unwrap().to_vec();
            snapshot = response.get_endpoint_snapshot().unwrap().to_vec();
            if expected_count == 2 {
                assert_eq!(cursor, [2; 32]);
            }
        }
        assert!(cursor.is_empty());
        assert_eq!(seen, [1, 2, 3]);
        assert_eq!(w.endpoint_page(&[], &[]).unwrap().snapshot, before);
    }
}

impl Workbench {
    pub fn endpoint_page(&mut self, after: &[u8], snapshot: &[u8]) -> Result<EndpointPage> {
        self.local_state_mut()?.endpoint_page(after, snapshot)
    }
    pub fn save_endpoint(&mut self, update: EndpointUpdate) -> Result<EndpointInfo> {
        self.local_state_mut()?.save_endpoint(update)
    }
    pub fn disable_endpoint(&mut self, key: &[u8], expected_revision: u64) -> Result<EndpointInfo> {
        self.local_state_mut()?
            .disable_endpoint(key, expected_revision)
    }
}
