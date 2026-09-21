//! Restore explicitly approved outbound policy on the original native Store.
//! Ciphertext/verifiers never substitute for current registry or instance grants.
use crate::{
    Error, Limits, Result,
    client::{Client, EndpointPolicy},
    managed_http::{
        Credential, EndpointApproval, HttpEndpoint, MAX_SERVICE_ENDPOINTS, NetworkProfile,
    },
};
use morrow_core::{
    dispatch::HostRuntime,
    io,
    outbound_authority::{Record, proto::record::Kind},
    store::{ServiceAuthorityLease, Store},
};
use morrow_plugin_runtime::{
    http_io::HttpGrant,
    io_binding::IoBinding,
    manager::{ManagedInstance, Manager},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
struct Clock {
    sample: Box<dyn FnMut() -> u64 + Send>,
    high_water: u64,
    failed: bool,
}
#[derive(Clone)]
struct Live {
    lease: ServiceAuthorityLease,
    clock: Arc<Mutex<Clock>>,
    created: u64,
    expires: u64,
    deadline: Instant,
}
impl Live {
    fn check(&self) -> Result<()> {
        self.lease.check().map_err(|_| Error::Denied)?;
        let mut clock = self.clock.lock().map_err(|_| Error::Denied)?;
        if clock.failed {
            return Err(Error::Denied);
        }
        let now = (clock.sample)();
        if now == 0
            || now < clock.high_water
            || now < self.created
            || now >= self.expires
            || Instant::now() >= self.deadline
        {
            clock.failed = true;
            return Err(Error::Denied);
        }
        clock.high_water = now;
        self.lease.check().map_err(|_| Error::Denied)
    }
}
/// A resolved policy and protected credential record, never a live HTTP grant.
/// No Debug implementation: host credential material stays out of diagnostics.
pub struct StoredHttpEndpoint {
    endpoint: Record,
    credential: Option<Record>,
    live: Live,
}
fn current(record: &Record, now: u64) -> Result<()> {
    let value = record.value();
    if value.disabled || now < value.created_ms || now >= value.expires_ms {
        Err(Error::Denied)
    } else {
        Ok(())
    }
}
fn reference(record: &Record) -> Vec<u8> {
    record
        .value()
        .reference
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        .into_bytes()
}
/// Validate the same transport policy used by live approval without DNS, sockets
/// or credential access. Persistent callers must first validate the core Record.
/// Custom TLS trust accepts one DER certificate, never PEM text.
pub fn validate_endpoint_policy(
    value: &morrow_core::outbound_authority::proto::Endpoint,
) -> Result<()> {
    validated_transport_policy(value).map(|_| ())
}
fn validated_transport_policy(
    value: &morrow_core::outbound_authority::proto::Endpoint,
) -> Result<(NetworkProfile, Limits)> {
    let profile = match value.profile {
        1 => NetworkProfile::PublicHttps,
        2 => NetworkProfile::LoopbackHttp,
        3 => NetworkProfile::LoopbackHttps,
        _ => return Err(Error::Invalid),
    };
    let limits = Limits {
        max_request_bytes: value
            .max_request_bytes
            .try_into()
            .map_err(|_| Error::Limit)?,
        max_response_bytes: value
            .max_response_bytes
            .try_into()
            .map_err(|_| Error::Limit)?,
        max_header_bytes: value
            .max_header_bytes
            .try_into()
            .map_err(|_| Error::Limit)?,
        max_concurrent: value.max_concurrent.try_into().map_err(|_| Error::Limit)?,
        timeout: Duration::from_millis(value.timeout_ms),
    };
    limits.validate()?;
    if value.max_frame_bytes == 0
        || value.max_frame_bytes > io::MAX_FRAME_BYTES as u64
        || limits.max_request_bytes > io::MAX_PAYLOAD_BYTES
        || limits.max_response_bytes > io::MAX_PAYLOAD_BYTES
        || limits.max_header_bytes > io::MAX_HEADER_BYTES
        || value.timeout_ms > io::MAX_SUBMIT_DEADLINE_MS
    {
        return Err(Error::Limit);
    }
    let origin = url::Url::parse(&value.origin).map_err(|_| Error::Invalid)?;
    if origin.origin().ascii_serialization() != value.origin
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
    {
        return Err(Error::Invalid);
    }
    let names: Vec<_> = value.methods.iter().map(String::as_str).collect();
    let policy = match profile {
        NetworkProfile::PublicHttps => EndpointPolicy::new(&value.origin, &names, false),
        NetworkProfile::LoopbackHttp => {
            if !value.origin.starts_with("http://") {
                return Err(Error::Denied);
            }
            EndpointPolicy::new(&value.origin, &names, true)
        }
        NetworkProfile::LoopbackHttps => EndpointPolicy::local_https(&value.origin, &names),
    }?;
    // Parse TLS trust before opening a credential. This builds no connection.
    if !value.root_certificate.is_empty() {
        Client::with_root_certificate(policy, limits, &value.root_certificate)?;
    } else {
        Client::new(policy, limits)?;
    }
    Ok((profile, limits))
}
impl StoredHttpEndpoint {
    /// Hold the original Store authority pin while resolving both records.
    /// UTC is a trusted, bounded, non-reentrant callback; backwards time or expiry
    /// permanently invalidates this resolution. Loading never enables a plugin.
    pub fn resolve(
        store: &mut Store,
        endpoint_reference: &[u8; 32],
        mut clock: impl FnMut() -> u64 + Send + 'static,
    ) -> Result<Self> {
        let lease = store.pin_service_authority().map_err(|_| Error::Denied)?;
        let endpoint = store
            .load_outbound_authority(endpoint_reference)
            .map_err(|_| Error::Denied)?
            .ok_or(Error::Denied)?;
        let now = clock();
        if now == 0 {
            return Err(Error::Denied);
        }
        current(&endpoint, now)?;
        let Some(Kind::Endpoint(value)) = endpoint.value().kind.as_ref() else {
            return Err(Error::Denied);
        };
        let credential = if value.credential_reference.is_empty() {
            None
        } else {
            let key = value
                .credential_reference
                .as_slice()
                .try_into()
                .map_err(|_| Error::Denied)?;
            let record = store
                .load_outbound_authority(key)
                .map_err(|_| Error::Denied)?
                .ok_or(Error::Denied)?;
            if !matches!(record.value().kind, Some(Kind::Credential(_))) {
                return Err(Error::Denied);
            }
            current(&record, now)?;
            Some(record)
        };
        let mut created = endpoint.value().created_ms;
        let mut expires = endpoint.value().expires_ms;
        if let Some(record) = &credential {
            created = created.max(record.value().created_ms);
            expires = expires.min(record.value().expires_ms);
        }
        let mut dependencies = vec![morrow_core::store::ServiceAuthorityResource::Outbound(
            *endpoint_reference,
        )];
        if let Some(record) = &credential {
            dependencies.push(morrow_core::store::ServiceAuthorityResource::Outbound(
                record.reference(),
            ));
        }
        let lease = store
            .narrow_service_authority(&lease, &dependencies)
            .map_err(|_| Error::Denied)?;
        let live = Live {
            lease,
            clock: Arc::new(Mutex::new(Clock {
                sample: Box::new(clock),
                high_water: now,
                failed: false,
            })),
            created,
            expires,
            deadline: Instant::now()
                .checked_add(Duration::from_millis(
                    expires.checked_sub(now).ok_or(Error::Denied)?,
                ))
                .ok_or(Error::Denied)?,
        };
        live.check()?;
        Ok(Self {
            endpoint,
            credential,
            live,
        })
    }
    /// Lowercase hexadecimal credential reference for the existing IO wire field.
    pub fn credential_reference(&self) -> Option<Vec<u8>> {
        self.credential.as_ref().map(reference)
    }
    pub fn revision(&self) -> u64 {
        self.endpoint.value().revision
    }
    /// Restrict a service's immutable selected-resource scope, including cached
    /// results, using this exact original Store lease and expiry clock.
    pub fn service_dependency(
        &self,
        store: &Store,
    ) -> Result<morrow_plugin_runtime::service_authority::AuthorityDependency> {
        let live = self.live.clone();
        morrow_plugin_runtime::service_authority::AuthorityDependency::new(
            store,
            live.lease.clone(),
            move || live.check().is_ok(),
        )
        .map_err(|_| Error::Denied)
    }
    /// Bind service replay to the complete selected records, including protected
    /// credential revision/expiry. Order is irrelevant; duplicates are rejected.
    /// This is an invocation identity only, never an approval or a live check.
    pub fn selection_digest(endpoints: &[Self]) -> Result<Option<[u8; 32]>> {
        if endpoints.len() > MAX_SERVICE_ENDPOINTS {
            return Err(Error::Limit);
        }
        if endpoints.is_empty() {
            return Ok(None);
        }
        let mut ordered = BTreeMap::new();
        for endpoint in endpoints {
            if ordered
                .insert(endpoint.endpoint.reference(), endpoint)
                .is_some()
            {
                return Err(Error::Invalid);
            }
        }
        let mut hash = Sha256::new();
        hash.update(b"morrow.service.outbound-selection.v1\0");
        hash.update((ordered.len() as u64).to_le_bytes());
        for (reference, endpoint) in ordered {
            hash.update(reference);
            hash.update(endpoint.endpoint.canonical_digest());
            if let Some(credential) = &endpoint.credential {
                hash.update([1]);
                hash.update(credential.canonical_digest());
            } else {
                hash.update([0]);
            }
        }
        Ok(Some(hash.finalize().into()))
    }
    /// The explicit provider callback is trusted host code. It runs only after
    /// original Store/managed owner/package/capabilities and policy validation.
    /// A missing credential never calls it. Unknown providers have no fallback.
    #[allow(clippy::too_many_arguments)]
    pub fn approve(
        self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        host_secret: [u8; 32],
        now: u64,
        resolve_secret: impl FnOnce(&Record) -> Result<Credential>,
    ) -> Result<HttpEndpoint> {
        self.approve_common(
            manager,
            host,
            instance,
            binding,
            host_secret,
            now,
            resolve_secret,
            None,
        )
    }
    /// Stable guest reference with a fresh live grant and approval epoch. Knowing
    /// a saved reference cannot authorize a call or revive a previous instance.
    #[allow(clippy::too_many_arguments)]
    pub fn approve_persistent(
        self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        host_secret: [u8; 32],
        now: u64,
        resolve_secret: impl FnOnce(&Record) -> Result<Credential>,
    ) -> Result<HttpEndpoint> {
        let reference = self.endpoint.reference();
        self.approve_common(
            manager,
            host,
            instance,
            binding,
            host_secret,
            now,
            resolve_secret,
            Some(reference),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn approve_common(
        self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        host_secret: [u8; 32],
        now: u64,
        resolve_secret: impl FnOnce(&Record) -> Result<Credential>,
        wire_reference: Option<[u8; 32]>,
    ) -> Result<HttpEndpoint> {
        host.store_local()
            .validate_service_authority(&self.live.lease)
            .map_err(|_| Error::Denied)?;
        self.live.check()?;
        let Some(Kind::Endpoint(value)) = self.endpoint.value().kind.as_ref() else {
            return Err(Error::Denied);
        };
        let package = instance.package().package();
        if value.package_id != package.manifest().package_id
            || value.package_sha256 != package.digest()
        {
            return Err(Error::Denied);
        }
        HttpGrant::preflight(
            manager,
            host,
            instance,
            binding,
            self.credential.is_some(),
            now,
        )
        .map_err(|_| Error::Denied)?;
        if host_secret == [0; 32] {
            return Err(Error::Limit);
        }
        let (profile, limits) = validated_transport_policy(value)?;
        self.live.check()?;
        let credential = if let Some(record) = &self.credential {
            let credential = resolve_secret(record)?;
            if credential.reference() != reference(record) {
                return Err(Error::Denied);
            }
            Some(credential)
        } else {
            None
        };
        self.live.check()?;
        let live = self.live;
        HttpEndpoint::approve_guarded(
            manager,
            host,
            instance,
            binding,
            EndpointApproval {
                origin: value.origin.clone(),
                methods: value.methods.clone(),
                profile,
                limits,
                response_frame_limit: value.max_frame_bytes,
                credential,
                root_certificate: (!value.root_certificate.is_empty())
                    .then(|| value.root_certificate.clone()),
            },
            host_secret,
            now,
            Some(Box::new(move || live.check().is_ok())),
            wire_reference,
        )
    }
    /// Windows-only DPAPI provider; raw plaintext and unsupported providers never
    /// fall back to files, environment variables or guest-supplied header values.
    #[cfg(target_os = "windows")]
    #[allow(clippy::too_many_arguments)]
    pub fn approve_windows(
        self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        host_secret: [u8; 32],
        now: u64,
    ) -> Result<HttpEndpoint> {
        self.approve(
            manager,
            host,
            instance,
            binding,
            host_secret,
            now,
            |record| {
                let secret = morrow_audit::credentials::open(record).map_err(|_| Error::Denied)?;
                Credential::header(reference(record), secret.header_name(), secret.value())
            },
        )
    }
    #[cfg(target_os = "windows")]
    #[allow(clippy::too_many_arguments)]
    pub fn approve_persistent_windows(
        self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        host_secret: [u8; 32],
        now: u64,
    ) -> Result<HttpEndpoint> {
        self.approve_persistent(
            manager,
            host,
            instance,
            binding,
            host_secret,
            now,
            |record| {
                let secret = morrow_audit::credentials::open(record).map_err(|_| Error::Denied)?;
                Credential::header(reference(record), secret.header_name(), secret.value())
            },
        )
    }
}
