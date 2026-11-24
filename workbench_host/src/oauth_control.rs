//! Account controls on the original Windows workbench. Only trusted host
//! provisioning adds an approved service. The private UI wire carries IDs.
use crate::{
    Result, Workbench, WorkbenchState,
    oauth_accounts::{
        Error, Provider,
        accounts::{Account, Enrollment, Refresh, Status, Update},
    },
    product_gate::StopRegistration,
};
use morrow_core::{
    outbound_authority::{WINDOWS_OAUTH_PROVIDER, proto::record::Kind},
    plugin_package::io::IoCapability,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct Service {
    key: [u8; 32],
    name: String,
    package: String,
    digest: [u8; 32],
    provider: Provider,
}
pub struct ServiceReview {
    pub key: [u8; 32],
    pub name: String,
    pub issuer: String,
    pub authorize: String,
    pub token: String,
    pub resource: String,
    pub client_id: String,
    pub scopes: String,
}
#[derive(Clone)]
pub struct Selection {
    pub package: String,
    pub digest: [u8; 32],
    pub revision: u64,
}
#[derive(Clone)]
pub struct Work {
    pub key: [u8; 32],
    pub account: Option<[u8; 32]>,
    pub phase: Phase,
    pub browser_url: Option<String>,
    pub status: Option<Status>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Waiting,
    Done,
    Cancelled,
    SignInRequired,
    Unconfirmed,
}
struct Job {
    selection: Selection,
    service: [u8; 32],
    view: Work,
    cancel: CancellationToken,
    claim: Option<(Account, u64)>,
    worker: Option<tokio::task::JoinHandle<crate::oauth_accounts::Result<Update>>>,
    _stop: Option<StopRegistration>,
}
impl Job {
    fn abort(&mut self, runtime: Option<&Runtime>) {
        self.cancel.cancel();
        if let Some(worker) = self.worker.take() {
            worker.abort();
            if let Some(rt) = runtime {
                // The worker belongs to our private multithreaded runtime.
                if tokio::runtime::Handle::try_current().is_ok() {
                    std::thread::scope(|scope| {
                        let _ = scope.spawn(|| rt.block_on(worker)).join();
                    });
                } else {
                    let _ = rt.block_on(worker);
                }
            }
            self.view.phase = Phase::Cancelled;
        }
        self._stop.take();
    }
}
#[derive(Default)]
pub(crate) struct Accounts {
    services: BTreeMap<[u8; 32], Service>,
    seen: BTreeSet<[u8; 32]>,
    job: Option<Job>,
    runtime: Option<Runtime>,
}
fn wall() -> Result<u64> {
    Ok(u64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}
fn selected(state: &WorkbenchState, selection: &Selection, active: bool) -> Result<()> {
    crate::channel_binding::require_owner(state.local_channel_owner)?;
    let manager = state.manager.as_ref().ok_or(Error::NotApproved)?;
    let choice = manager
        .selection(&selection.package)
        .ok_or(Error::NotApproved)?;
    if manager.revision() != selection.revision
        || choice.digest != selection.digest
        || (active
            && (!choice.enabled
                || ![IoCapability::HttpRequest, IoCapability::CredentialUse]
                    .iter()
                    .all(|v| choice.approved_io.contains(v))))
    {
        return Err(Error::NotApproved.into());
    }
    Ok(())
}
impl Service {
    fn review(&self) -> ServiceReview {
        let (issuer, authorize, token, resource, client_id, scopes) = self.provider.review_fields();
        ServiceReview {
            key: self.key,
            name: self.name.clone(),
            issuer: issuer.into(),
            authorize: authorize.into(),
            token: token.into(),
            resource: resource.into(),
            client_id: client_id.into(),
            scopes,
        }
    }
    fn account(&self, reference: [u8; 32]) -> Result<Account> {
        Ok(Account::bind(
            self.provider.clone(),
            self.digest,
            reference,
        )?)
    }
}
impl Accounts {
    fn runtime(&mut self) -> Result<&Runtime> {
        if self.runtime.is_none() {
            self.runtime = Some(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()?,
            );
        }
        Ok(self.runtime.as_ref().expect("initialized"))
    }
    fn service(&self, sel: &Selection, key: [u8; 32]) -> Result<Service> {
        Ok(self
            .services
            .get(&key)
            .filter(|s| s.package == sel.package && s.digest == sel.digest)
            .ok_or(Error::NotApproved)?
            .clone())
    }
    fn reserve(&mut self, nonce: [u8; 32]) -> Result<()> {
        if nonce == [0; 32] || self.seen.len() >= 256 || self.seen.contains(&nonce) {
            return Err(Error::Limit.into());
        }
        if self.job.as_ref().is_some_and(|j| j.worker.is_some()) {
            return Err(Error::Busy.into());
        }
        self.seen.insert(nonce);
        Ok(())
    }
    pub(crate) fn stop(&mut self) {
        if let Some(job) = &mut self.job {
            job.abort(self.runtime.as_ref());
        }
    }
}
impl Drop for Accounts {
    fn drop(&mut self) {
        self.stop();
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}
impl Workbench {
    /// Trusted callers supply a previously approved provider. No UI/guest RPC
    /// can register an arbitrary issuer or replace an existing service URL.
    pub fn bind_approved_oauth_service(
        &mut self,
        sel: Selection,
        name: &str,
        provider: Provider,
    ) -> Result<[u8; 32]> {
        self.product_gate().check()?;
        let state = self.local_state()?;
        selected(state, &sel, true)?;
        if name.is_empty() || name.len() > 100 || name.chars().any(char::is_control) {
            return Err(Error::Invalid.into());
        }
        let key =
            provider.identity(&state.host.store_local().tls_store_identity()?, &sel.digest)?;
        if !self.oauth_accounts.services.contains_key(&key)
            && (self.oauth_accounts.services.len() >= 32
                || self
                    .oauth_accounts
                    .services
                    .values()
                    .filter(|s| s.package == sel.package && s.digest == sel.digest)
                    .count()
                    >= 8)
        {
            return Err(Error::Limit.into());
        }
        self.oauth_accounts.services.insert(
            key,
            Service {
                key,
                name: name.into(),
                package: sel.package,
                digest: sel.digest,
                provider,
            },
        );
        Ok(key)
    }
    pub fn oauth_services(&self, sel: &Selection) -> Result<Vec<ServiceReview>> {
        self.product_gate().check()?;
        selected(self.local_state()?, sel, false)?;
        Ok(self
            .oauth_accounts
            .services
            .values()
            .filter(|s| s.package == sel.package && s.digest == sel.digest)
            .map(Service::review)
            .collect())
    }
    pub fn oauth_account_page(
        &mut self,
        sel: &Selection,
        key: [u8; 32],
        after: Option<[u8; 32]>,
        snapshot: Option<[u8; 32]>,
    ) -> Result<(Vec<Status>, [u8; 32], Option<[u8; 32]>)> {
        self.product_gate().check()?;
        selected(self.local_state()?, sel, false)?;
        let service = self.oauth_accounts.service(sel, key)?;
        let store = self.state.local_mut()?.host.store_local_mut();
        let page = store.list_outbound_authorities_local(after, snapshot, 16)?;
        let now = wall()?;
        let mut entries = vec![];
        for record in page.records {
            if !matches!(record.value().kind.as_ref(),Some(Kind::Credential(c)) if c.provider==WINDOWS_OAUTH_PROVIDER)
            {
                continue;
            }
            match service.account(record.reference())?.status(store, now) {
                Ok(status) => entries.push(status),
                Err(Error::NotApproved) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok((entries, page.snapshot, page.next))
    }
    pub fn oauth_start(
        &mut self,
        sel: &Selection,
        key: [u8; 32],
        reference: Option<[u8; 32]>,
        nonce: [u8; 32],
        days: u16,
    ) -> Result<Work> {
        let gate = self.product_gate();
        gate.check()?;
        selected(self.local_state()?, sel, true)?;
        let service = self.oauth_accounts.service(sel, key)?;
        if reference.is_none() && !(1..=30).contains(&days) {
            return Err(Error::Invalid.into());
        }
        if self
            .oauth_accounts
            .job
            .as_ref()
            .is_some_and(|job| job.cancel.is_cancelled() && job.worker.is_some())
        {
            self.stop_oauth_account_job();
        }
        self.oauth_accounts.reserve(nonce)?;
        let now = wall()?;
        let cancel = CancellationToken::new();
        let fence = cancel.clone();
        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || fence.cancel());
        let stop = gate.register_scoped(callback.clone())?;
        self.state.require_writable()?;
        let state = self.state.local_mut()?;
        state
            .manager
            .as_mut()
            .ok_or(Error::NotApproved)?
            .watch_account_revision(sel.revision, &callback)?;
        let rt = self.oauth_accounts.runtime()?;
        let (account, browser_url, claim, worker) = if let Some(reference) = reference {
            state.host.prepare_write()?;
            match service
                .account(reference)?
                .begin_refresh(state.host.store_local_mut(), now)?
            {
                Refresh::Current(status) => {
                    let view = Work {
                        key: nonce,
                        account: Some(reference),
                        phase: Phase::Done,
                        browser_url: None,
                        status: Some(status),
                    };
                    self.oauth_accounts.job = Some(Job {
                        selection: sel.clone(),
                        service: key,
                        view: view.clone(),
                        cancel,
                        claim: None,
                        worker: None,
                        _stop: None,
                    });
                    return Ok(view);
                }
                Refresh::Pending(_) => return Err(Error::Busy.into()),
                Refresh::Start(rotation) => {
                    let token = cancel.clone();
                    let claim = rotation.claim_owner();
                    (
                        reference,
                        None,
                        Some(claim),
                        rt.spawn(async move { Ok(rotation.exchange(now, token).await) }),
                    )
                }
            }
        } else {
            let enrollment = rt.block_on(Enrollment::new(
                state.host.store_local(),
                service.provider,
                sel.digest,
                now,
                u64::from(days) * 86400000,
            ))?;
            let reference = enrollment.account.reference();
            let url = enrollment.browser_url().to_string();
            (
                reference,
                Some(url),
                None,
                rt.spawn(enrollment.receive(cancel.clone())),
            )
        };
        let mut view = Work {
            key: nonce,
            account: Some(account),
            phase: Phase::Waiting,
            browser_url: None,
            status: None,
        };
        self.oauth_accounts.job = Some(Job {
            selection: sel.clone(),
            service: key,
            view: view.clone(),
            cancel,
            claim,
            worker: Some(worker),
            _stop: Some(stop),
        });
        view.browser_url = browser_url;
        Ok(view)
    }
    pub fn oauth_poll(
        &mut self,
        sel: &Selection,
        service: [u8; 32],
        key: [u8; 32],
        cancel: bool,
    ) -> Result<Work> {
        let gate = self.product_gate();
        let authorized = gate.check().is_ok() && selected(self.local_state()?, sel, true).is_ok();
        let accounts = &mut self.oauth_accounts;
        if accounts.job.as_ref().is_none_or(|j| j.view.key != key) {
            // Losing an owner's reply cannot prove whether an earlier operation committed.
            // Only the current selection may query its own account list to reconcile it.
            accounts.service(sel, service)?;
            gate.check()?;
            selected(self.local_state()?, sel, false)?;
            return Ok(Work {
                key,
                account: None,
                phase: Phase::Unconfirmed,
                browser_url: None,
                status: None,
            });
        }
        let job = accounts
            .job
            .as_mut()
            .expect("matched original account request");
        if job.service != service
            || job.selection.package != sel.package
            || job.selection.digest != sel.digest
            || job.selection.revision != sel.revision
        {
            return Err(Error::NotApproved.into());
        }
        if cancel || !authorized || job.cancel.is_cancelled() {
            job.abort(accounts.runtime.as_ref());
            if let Some((account, revision)) = job.claim.take() {
                // An interrupted rotation may already have reached the issuer. Never resend it.
                if let Ok(state) = self.state.local_mut() {
                    if state.host.prepare_write().is_ok() {
                        let _ = account.interrupt_rotation(state.host.store_local_mut(), revision);
                    }
                }
                job.view.phase = Phase::Unconfirmed;
            }
            job._stop.take();
        } else if job.worker.as_ref().is_some_and(|task| task.is_finished()) {
            // Establish writable ownership before consuming the one network result.
            self.state.require_writable()?;
            let state = self.state.local_mut()?;
            state.host.prepare_write()?;
            let result = accounts
                .runtime
                .as_ref()
                .ok_or(Error::Unknown)?
                .block_on(job.worker.take().expect("completed account worker"));
            let result = result
                .map_err(|_| Error::Unknown)
                .and_then(|v| v)
                .and_then(|v| {
                    v.commit(
                        state.host.store_local_mut(),
                        wall().map_err(|_| Error::Unknown)?,
                    )
                });
            job.claim.take();
            match result {
                Ok(status) => {
                    job.view.phase = Phase::Done;
                    job.view.status = Some(status);
                }
                Err(Error::Reauthorize | Error::Expired) => job.view.phase = Phase::SignInRequired,
                Err(_) => job.view.phase = Phase::Unconfirmed,
            };
            job._stop.take();
        }
        Ok(job.view.clone())
    }
    pub fn oauth_sign_out(
        &mut self,
        sel: &Selection,
        service: [u8; 32],
        reference: [u8; 32],
        revision: u64,
    ) -> Result<Status> {
        self.product_gate().check()?;
        // A user may still withdraw an existing account after disabling the plugin.
        selected(self.local_state()?, sel, false)?;
        let account = self
            .oauth_accounts
            .service(sel, service)?
            .account(reference)?;
        let state = self.local_state_mut()?;
        state.host.prepare_write()?;
        let result = account.sign_out(state.host.store_local_mut(), revision, wall()?)?;
        if self
            .oauth_accounts
            .job
            .as_ref()
            .is_some_and(|job| job.view.account == Some(reference))
        {
            self.oauth_accounts.stop();
        }
        Ok(result)
    }
    pub(crate) fn stop_oauth_account_job(&mut self) {
        self.oauth_accounts.stop();
        let claim = self
            .oauth_accounts
            .job
            .as_mut()
            .and_then(|job| job.claim.take());
        if let Some((account, revision)) = claim
            && let Ok(state) = self.local_state_mut()
            && state.host.prepare_write().is_ok()
        {
            let _ = account.interrupt_rotation(state.host.store_local_mut(), revision);
        }
    }
}
