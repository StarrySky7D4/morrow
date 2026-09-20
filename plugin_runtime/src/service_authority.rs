//! Restore explicit host approvals on the original Store. Persisted data does
//! not replace registry approval, managed-instance checks or content grants.
use crate::{
    io_binding::{Error, IoBinding, Result},
    manager::{ManagedInstance, Manager},
    service_content::{ContentScope, ServiceContentPolicy},
    service_history::ServiceJournal,
    service_io::{ListenerGrant, ServiceGrant},
};
use morrow_core::{
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    service_authority::proto::{Publication, record::Kind},
    service_config::Config,
    store::{ServiceAuthorityLease, Store},
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

struct Clock {
    sample: Box<dyn FnMut() -> u64 + Send>,
    high_water: u64,
    failed: bool,
}
/// Native host probe, deliberately neither serializable nor constructible by a guest.
#[derive(Clone)]
pub(crate) struct LiveAuthority {
    lease: ServiceAuthorityLease,
    clock: Arc<Mutex<Clock>>,
    created: u64,
    expires: u64,
    deadline: Instant,
}
impl LiveAuthority {
    fn now(&self) -> Result<u64> {
        self.lease.check().map_err(|_| Error::Denied)?;
        let mut clock = self.clock.lock().map_err(|_| Error::Clock)?;
        if clock.failed {
            return Err(Error::Clock);
        }
        let now = (clock.sample)();
        if now == 0 || now < clock.high_water || now < self.created {
            clock.failed = true;
            return Err(Error::Clock);
        }
        clock.high_water = now;
        if now >= self.expires || Instant::now() >= self.deadline {
            clock.failed = true;
            return Err(Error::Expired);
        }
        self.lease.check().map_err(|_| Error::Denied)?;
        Ok(now)
    }
    pub(crate) fn check(&self) -> Result<()> {
        self.now().map(|_| ())
    }
}

/// A digest is useful for inbound verification, never an outbound credential.
/// No Debug implementation: token verifiers are omitted from diagnostics.
#[derive(Clone)]
pub struct ConfiguredPrincipal {
    id: String,
    verifier: [u8; 32],
    scopes: Vec<ContentScope>,
}
impl ConfiguredPrincipal {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn verifier(&self) -> [u8; 32] {
        self.verifier
    }
    pub fn scopes(&self) -> &[ContentScope] {
        &self.scopes
    }
}
pub struct ResolvedService {
    config: Config,
    publication: Publication,
    principals: Vec<ConfiguredPrincipal>,
    live: LiveAuthority,
}
impl ResolvedService {
    /// Pin the Store's authority writer lock before loading any references.
    /// The clock is trusted UTC milliseconds, bounded and non-reentrant. A
    /// clock regression permanently invalidates this resolution. Memory-only
    /// and unsupported platform stores cannot restore live publication.
    pub fn resolve(
        store: &mut Store,
        config_id: &str,
        approval_reference: &[u8; 32],
        mut clock: impl FnMut() -> u64 + Send + 'static,
    ) -> Result<Self> {
        let lease = store.pin_service_authority().map_err(|_| Error::Denied)?;
        let config = store
            .load_service_config(config_id)
            .map_err(|_| Error::Denied)?
            .ok_or(Error::Denied)?;
        if config.value().disabled
            || config.value().principals.is_empty()
            || !config
                .value()
                .approval_references
                .iter()
                .any(|v| v == approval_reference)
        {
            return Err(Error::Denied);
        }
        let approval = store
            .load_service_authority(approval_reference)
            .map_err(|_| Error::Denied)?
            .ok_or(Error::Denied)?;
        let now = clock();
        if now == 0 {
            return Err(Error::Clock);
        }
        approval.check_time(now).map_err(|_| Error::Denied)?;
        let Some(Kind::Publication(publication)) = approval.value().kind.as_ref() else {
            return Err(Error::Denied);
        };
        if publication.config_id != config.value().id
            || publication.config_sha256 != config.digest()
        {
            return Err(Error::Denied);
        }
        let mut created = approval.value().created_ms;
        let mut expires = approval.value().expires_ms;
        let mut principals = Vec::with_capacity(config.value().principals.len());
        for principal in &config.value().principals {
            let reference = principal
                .authentication_reference
                .as_slice()
                .try_into()
                .map_err(|_| Error::Denied)?;
            let authentication = store
                .load_service_authority(reference)
                .map_err(|_| Error::Denied)?
                .ok_or(Error::Denied)?;
            authentication.check_time(now).map_err(|_| Error::Denied)?;
            let Some(Kind::Authentication(value)) = authentication.value().kind.as_ref() else {
                return Err(Error::Denied);
            };
            if value.principal_id != principal.id {
                return Err(Error::Denied);
            }
            created = created.max(authentication.value().created_ms);
            expires = expires.min(authentication.value().expires_ms);
            let scopes = principal
                .content_scopes
                .iter()
                .map(|scope| {
                    Ok(ContentScope {
                        kind: match scope.kind {
                            1 => GrantKind::Rename,
                            2 => GrantKind::ReadSummary,
                            3 => GrantKind::QueryOperation,
                            4 => GrantKind::ReadAttachment,
                            5 => GrantKind::CreateContent,
                            6 => GrantKind::EditContent,
                            7 => GrantKind::ReadContent,
                            _ => return Err(Error::Denied),
                        },
                        card_id: scope.card_id.clone(),
                        attachment_id: (!scope.attachment_id.is_empty())
                            .then(|| scope.attachment_id.clone()),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            principals.push(ConfiguredPrincipal {
                id: principal.id.clone(),
                verifier: value
                    .token_sha256
                    .as_slice()
                    .try_into()
                    .map_err(|_| Error::Denied)?,
                scopes,
            });
        }
        let live = LiveAuthority {
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
                    expires.checked_sub(now).ok_or(Error::Expired)?,
                ))
                .ok_or(Error::Clock)?,
        };
        live.check()?;
        Ok(Self {
            config,
            publication: publication.clone(),
            principals,
            live,
        })
    }
    /// Freshly issue native resources after rechecking the actual managed
    /// instance and original Store; no persisted object is itself a grant.
    pub fn issue(
        self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        now: u64,
    ) -> Result<ConfiguredService> {
        host.store_local()
            .validate_service_authority(&self.live.lease)
            .map_err(|_| Error::Denied)?;
        self.live.check()?;
        let grant = ServiceGrant::issue(
            manager,
            host,
            instance,
            binding,
            &self.config.value().service,
            &self.config.value().handler,
            now,
        )?
        .with_authority(self.live.clone())?;
        grant.validate_config(&self.config)?;
        let listener = ListenerGrant::issue(manager, host, instance, binding, now)?
            .with_authority(self.live.clone())?;
        let mut scopes = Vec::new();
        for principal in &self.principals {
            for scope in &principal.scopes {
                if !scopes.contains(scope) {
                    scopes.push(scope.clone());
                }
            }
        }
        let content_policy = ServiceContentPolicy::issue(host, instance, &grant, scopes, now)?;
        let journal_live = self.live.clone();
        let journal = ServiceJournal::from_config(&self.config, &grant, move || {
            journal_live.now().unwrap_or(0)
        })
        .map_err(|_| Error::Denied)?;
        self.live.check()?;
        Ok(ConfiguredService {
            config: self.config,
            publication: self.publication,
            principals: self.principals,
            live: self.live,
            grant,
            listener,
            content_policy,
            journal,
        })
    }
}

/// Fully checked host wiring, held by native publication and every live principal.
#[derive(Clone)]
pub struct ConfiguredService {
    config: Config,
    publication: Publication,
    principals: Vec<ConfiguredPrincipal>,
    live: LiveAuthority,
    grant: ServiceGrant,
    listener: ListenerGrant,
    content_policy: ServiceContentPolicy,
    journal: ServiceJournal,
}
impl ConfiguredService {
    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn publication(&self) -> &Publication {
        &self.publication
    }
    pub fn principals(&self) -> &[ConfiguredPrincipal] {
        &self.principals
    }
    pub fn grant(&self) -> &ServiceGrant {
        &self.grant
    }
    pub fn listener(&self) -> &ListenerGrant {
        &self.listener
    }
    pub fn content_policy(&self) -> &ServiceContentPolicy {
        &self.content_policy
    }
    pub fn journal(&self) -> &ServiceJournal {
        &self.journal
    }
    pub fn check(&self) -> Result<()> {
        self.live.check()
    }
}
