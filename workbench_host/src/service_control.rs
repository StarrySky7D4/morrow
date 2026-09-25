//! Trusted service administration on the original Store. These operations save
//! desired configuration and explicit approval, never start a network listener.
use crate::{Result, Workbench, WorkbenchState};
use morrow_core::{
    plugin_package::io::IoCapability,
    service_authority::{
        Record,
        proto::{self as auth, record::Kind},
    },
    service_config::{Config, proto as config},
    store::ServiceConfigPage,
};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

pub enum AuthorityKind {
    Authentication { principal_id: String },
    Publication(auth::Publication),
}
/// Redacted metadata: no bearer token or authentication verifier.
pub struct AuthorityInfo {
    pub reference: [u8; 32],
    pub revision: u64,
    pub created_ms: u64,
    pub expires_ms: u64,
    pub disabled: bool,
    pub kind: AuthorityKind,
}
pub struct AuthorityPage {
    pub entries: Vec<AuthorityInfo>,
    pub snapshot: [u8; 32],
    pub next: Option<[u8; 32]>,
}
/// Returned only after a successful explicit creation or rotation. No Debug.
/// Lost replies require a fresh explicit rotation; the original token is not recoverable.
pub struct IssuedAuthentication {
    pub info: AuthorityInfo,
    pub token: Zeroizing<String>,
}
pub struct ServiceConfigUpdate {
    pub id: String,
    pub expected_revision: u64,
    pub registry_revision: u64,
    pub package_id: String,
    pub package_digest: [u8; 32],
    pub service: String,
    pub handler: String,
    pub retention_ms: u64,
    pub principals: Vec<config::Principal>,
}
pub struct PublicationUpdate {
    pub reference: [u8; 32],
    pub expected_revision: u64,
    pub config_id: String,
    pub config_revision: u64,
    pub config_digest: [u8; 32],
    pub registry_revision: u64,
    pub package_id: String,
    /// Requested maximum. The returned expiry is capped by every principal's
    /// authentication expiry; callers must display that actual timestamp.
    pub lifetime_days: u32,
    pub listen_address: String,
    pub tls_required: bool,
    pub method: String,
    pub path: String,
    pub query_path: String,
}
fn key(bytes: &[u8]) -> Result<[u8; 32]> {
    let key: [u8; 32] = bytes.try_into().map_err(|_| "invalid service reference")?;
    if key == [0; 32] {
        return Err("invalid service reference".into());
    }
    Ok(key)
}
fn revision(previous: u64) -> Result<u64> {
    previous
        .checked_add(1)
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or_else(|| "service revision exhausted".into())
}
fn utc() -> Result<u64> {
    Ok(u64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}
fn lifetime(days: u32) -> Result<(u64, u64)> {
    if !(1..=30).contains(&days) {
        return Err("service lifetime must be between 1 and 30 days".into());
    }
    let now = utc()?;
    Ok((
        now,
        now.checked_add(u64::from(days) * 86_400_000)
            .ok_or("service expiry overflow")?,
    ))
}
fn random_key() -> Result<[u8; 32]> {
    for _ in 0..4 {
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes)?;
        if bytes != [0; 32] {
            return Ok(bytes);
        }
    }
    Err("service identity unavailable".into())
}
fn info(record: &Record) -> AuthorityInfo {
    let value = record.value();
    let kind = match value.kind.as_ref().expect("validated authority") {
        Kind::Authentication(auth) => AuthorityKind::Authentication {
            principal_id: auth.principal_id.clone(),
        },
        Kind::Publication(publication) => AuthorityKind::Publication(publication.clone()),
    };
    AuthorityInfo {
        reference: record.reference(),
        revision: value.revision,
        created_ms: value.created_ms,
        expires_ms: value.expires_ms,
        disabled: value.disabled,
        kind,
    }
}
impl WorkbenchState {
    fn service_package(&self, id: &str, digest: &[u8], revision: u64, handler: &str) -> Result<()> {
        let manager = self.manager.as_ref().ok_or("plugin manager unavailable")?;
        if manager.revision() != revision {
            return Err("plugin registry changed".into());
        }
        let selection = manager.selection(id).ok_or("plugin is not selected")?;
        if digest != selection.digest {
            return Err("plugin digest changed".into());
        }
        let package = self
            .manager
            .as_ref()
            .ok_or("plugin catalog unavailable")?
            .installed_package(selection.digest)?;
        if package.manifest().package_id != id {
            return Err("plugin identity mismatch".into());
        }
        let declaration = package
            .io_declaration()
            .ok_or("service requires IO declaration")?;
        if declaration.service_schema_sha256 != morrow_core::service::schema_digest()
            || !declaration.handlers.iter().any(|v| v == handler)
        {
            return Err("service handler is not declared".into());
        }
        for capability in [IoCapability::HttpListen, IoCapability::HttpPublish] {
            if !selection.approved_io.contains(&capability)
                || !package.io_capabilities().contains(&capability)
            {
                return Err(
                    "service requires declared and approved listen/publish capabilities".into(),
                );
            }
        }
        Ok(())
    }
    fn service_principals(
        &self,
        principals: &[config::Principal],
        now: u64,
        expires: Option<u64>,
    ) -> Result<u64> {
        if principals.is_empty() {
            return Err("service needs an authenticated principal".into());
        }
        let mut effective_expiry = expires.unwrap_or(u64::MAX);
        for principal in principals {
            let record = self
                .host
                .store_local()
                .load_service_authority(&key(&principal.authentication_reference)?)?
                .ok_or("service authentication missing")?;
            record.check_time(now)?;
            let Some(Kind::Authentication(authentication)) = record.value().kind.as_ref() else {
                return Err("service authentication reference type mismatch".into());
            };
            if authentication.principal_id != principal.id {
                return Err("service principal changed".into());
            }
            effective_expiry = effective_expiry.min(record.value().expires_ms);
        }
        Ok(effective_expiry)
    }
    pub fn service_config_page(
        &mut self,
        after: &str,
        snapshot: &[u8],
    ) -> Result<ServiceConfigPage> {
        let snapshot = if snapshot.is_empty() {
            None
        } else {
            Some(key(snapshot)?)
        };
        Ok(self.host.store_local_mut().list_service_configs_local(
            (!after.is_empty()).then_some(after),
            snapshot,
            1,
        )?)
    }
    pub fn service_authority_page(
        &mut self,
        after: &[u8],
        snapshot: &[u8],
    ) -> Result<AuthorityPage> {
        let after = (!after.is_empty()).then(|| key(after)).transpose()?;
        let snapshot = (!snapshot.is_empty()).then(|| key(snapshot)).transpose()?;
        let page = self
            .host
            .store_local_mut()
            .list_service_authorities_local(after, snapshot, 2)?;
        Ok(AuthorityPage {
            entries: page.records.iter().map(info).collect(),
            snapshot: page.snapshot,
            next: page.next,
        })
    }
    pub fn issue_service_authentication(
        &mut self,
        reference: &[u8],
        expected_revision: u64,
        principal: &str,
        lifetime_days: u32,
    ) -> Result<IssuedAuthentication> {
        let next = revision(expected_revision)?;
        let (created, expires) = lifetime(lifetime_days)?;
        let reference = if reference.is_empty() {
            if expected_revision != 0 {
                return Err("new authentication requires revision zero".into());
            }
            let candidate = random_key()?;
            if self
                .host
                .store_local()
                .load_service_authority(&candidate)?
                .is_some()
            {
                return Err("service identity collision".into());
            }
            candidate
        } else {
            let key = key(reference)?;
            let old = self
                .host
                .store_local()
                .load_service_authority(&key)?
                .ok_or("authentication not found")?;
            if old.value().revision != expected_revision {
                return Err("authentication changed".into());
            }
            if !matches!(old.value().kind.as_ref(), Some(Kind::Authentication(a)) if a.principal_id == principal)
            {
                return Err("authentication principal cannot change".into());
            }
            key
        };
        let mut entropy = Zeroizing::new([0u8; 32]);
        getrandom::fill(&mut *entropy)?;
        if *entropy == [0; 32] {
            return Err("authentication entropy unavailable".into());
        }
        let mut token = Zeroizing::new(String::with_capacity(64));
        use std::fmt::Write;
        for byte in entropy.iter() {
            write!(&mut *token, "{byte:02x}")?;
        }
        let record = Record::encode(auth::Record {
            schema_version: 1,
            reference: reference.to_vec(),
            revision: next,
            created_ms: created,
            expires_ms: expires,
            disabled: false,
            kind: Some(Kind::Authentication(auth::Authentication {
                principal_id: principal.into(),
                token_sha256: Sha256::digest(token.as_bytes()).to_vec(),
            })),
        })?;
        self.host.prepare_write()?;
        self.host
            .store_local_mut()
            .save_service_authority_local(&record, expected_revision)?;
        Ok(IssuedAuthentication {
            info: info(&record),
            token,
        })
    }
    pub fn save_service_config(&mut self, update: ServiceConfigUpdate) -> Result<Config> {
        let next = revision(update.expected_revision)?;
        let (id, namespace, references) = if update.id.is_empty() {
            if update.expected_revision != 0 {
                return Err("new configuration requires revision zero".into());
            }
            let id = format!(
                "service-{}",
                random_key()?
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            );
            let publication = random_key()?;
            if self
                .host
                .store_local()
                .load_service_authority(&publication)?
                .is_some()
            {
                return Err("publication identity collision".into());
            }
            (id, random_key()?.to_vec(), vec![publication.to_vec()])
        } else {
            let old = self
                .host
                .store_local()
                .load_service_config(&update.id)?
                .ok_or("service configuration not found")?;
            if old.value().revision != update.expected_revision {
                return Err("service configuration changed".into());
            }
            if old.value().service != update.service {
                return Err("service identity cannot change".into());
            }
            (
                update.id,
                old.value().namespace.clone(),
                old.value().approval_references.clone(),
            )
        };
        let config = Config::encode(config::Configuration {
            schema_version: 1,
            id,
            revision: next,
            namespace,
            retention_ms: update.retention_ms,
            service: update.service,
            handler: update.handler,
            package_sha256: update.package_digest.to_vec(),
            disabled: false,
            principals: update.principals,
            approval_references: references,
        })?;
        self.service_package(
            &update.package_id,
            &update.package_digest,
            update.registry_revision,
            &config.value().handler,
        )?;
        self.service_principals(&config.value().principals, utc()?, None)?;
        self.host.prepare_write()?;
        self.host
            .store_local_mut()
            .save_service_config_local(&config, update.expected_revision)?;
        Ok(config)
    }
    pub fn disable_service_config(&mut self, id: &str, expected_revision: u64) -> Result<Config> {
        let old = self
            .host
            .store_local()
            .load_service_config(id)?
            .ok_or("service configuration not found")?;
        if old.value().revision != expected_revision {
            return Err("service configuration changed".into());
        }
        if old.value().disabled {
            return Ok(old);
        }
        let mut value = old.value().clone();
        value.revision = revision(expected_revision)?;
        value.disabled = true;
        let disabled = Config::encode(value)?;
        self.host.prepare_write()?;
        self.host
            .store_local_mut()
            .save_service_config_local(&disabled, expected_revision)?;
        Ok(disabled)
    }
    pub fn save_service_publication(&mut self, update: PublicationUpdate) -> Result<AuthorityInfo> {
        let config = self
            .host
            .store_local()
            .load_service_config(&update.config_id)?
            .ok_or("service configuration not found")?;
        let value = config.value();
        if value.disabled
            || value.revision != update.config_revision
            || config.digest() != update.config_digest
            || !value
                .approval_references
                .iter()
                .any(|r| r.as_slice() == update.reference)
        {
            return Err("service configuration or publication identity changed".into());
        }
        self.service_package(
            &update.package_id,
            &value.package_sha256,
            update.registry_revision,
            &value.handler,
        )?;
        let (created, expires) = lifetime(update.lifetime_days)?;
        let expires = self.service_principals(&value.principals, created, Some(expires))?;
        if !["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]
            .contains(&update.method.as_str())
        {
            return Err("unsupported service method".into());
        }
        let record = Record::encode(auth::Record {
            schema_version: 1,
            reference: update.reference.to_vec(),
            revision: revision(update.expected_revision)?,
            created_ms: created,
            expires_ms: expires,
            disabled: false,
            kind: Some(Kind::Publication(auth::Publication {
                config_id: update.config_id,
                config_sha256: update.config_digest.to_vec(),
                listen_address: update.listen_address,
                tls_required: update.tls_required,
                method: update.method,
                path: update.path,
                query_path: update.query_path,
            })),
        })?;
        self.host.prepare_write()?;
        self.host
            .store_local_mut()
            .save_service_authority_local(&record, update.expected_revision)?;
        Ok(info(&record))
    }
    /// Revocation does not require package files, a live token or current expiry.
    pub fn disable_service_authority(
        &mut self,
        reference: &[u8],
        expected_revision: u64,
    ) -> Result<AuthorityInfo> {
        let old = self
            .host
            .store_local()
            .load_service_authority(&key(reference)?)?
            .ok_or("service authority not found")?;
        if old.value().revision != expected_revision {
            return Err("service authority changed".into());
        }
        if old.value().disabled {
            return Ok(info(&old));
        }
        let mut value = old.value().clone();
        value.revision = revision(expected_revision)?;
        value.disabled = true;
        let disabled = Record::encode(value)?;
        self.host.prepare_write()?;
        self.host
            .store_local_mut()
            .save_service_authority_local(&disabled, expected_revision)?;
        Ok(info(&disabled))
    }
}

impl Workbench {
    pub fn service_config_page(
        &mut self,
        after: &str,
        snapshot: &[u8],
    ) -> Result<ServiceConfigPage> {
        self.local_state_mut()?.service_config_page(after, snapshot)
    }
    pub fn service_authority_page(
        &mut self,
        after: &[u8],
        snapshot: &[u8],
    ) -> Result<AuthorityPage> {
        self.local_state_mut()?
            .service_authority_page(after, snapshot)
    }
    pub fn issue_service_authentication(
        &mut self,
        reference: &[u8],
        expected_revision: u64,
        principal: &str,
        lifetime_days: u32,
    ) -> Result<IssuedAuthentication> {
        self.local_state_mut()?.issue_service_authentication(
            reference,
            expected_revision,
            principal,
            lifetime_days,
        )
    }
    pub fn save_service_config(&mut self, update: ServiceConfigUpdate) -> Result<Config> {
        self.local_state_mut()?.save_service_config(update)
    }
    pub fn disable_service_config(&mut self, id: &str, expected_revision: u64) -> Result<Config> {
        self.local_state_mut()?
            .disable_service_config(id, expected_revision)
    }
    pub fn save_service_publication(&mut self, update: PublicationUpdate) -> Result<AuthorityInfo> {
        self.local_state_mut()?.save_service_publication(update)
    }
    pub fn disable_service_authority(
        &mut self,
        reference: &[u8],
        expected_revision: u64,
    ) -> Result<AuthorityInfo> {
        self.local_state_mut()?
            .disable_service_authority(reference, expected_revision)
    }
}
