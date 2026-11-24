//! Hold only the disabled successor of a reviewed credential. The original
//! worker owns the Store and acknowledges the compare-and-swap itself.
use super::{AccountState, Accounts, Error, Phase, Result, Selection, Status, Work, Workbench};
use morrow_core::{
    outbound_authority::Record,
    store::{ServiceAuthorityLease, ServiceAuthorityResource, Store},
};
use morrow_plugin_runtime::{io_jobs::ServiceUpdateHandle, manager::AccountRevision};

pub(super) struct Reviewed {
    selection: Selection,
    service: [u8; 32],
    witness: AccountRevision,
    expected: u64,
    marker: Record,
    record_lease: Option<ServiceAuthorityLease>,
}
impl Reviewed {
    pub(super) fn new(
        selection: &Selection,
        service: [u8; 32],
        witness: AccountRevision,
        expected: u64,
        marker: Record,
    ) -> Self {
        Self {
            selection: selection.clone(),
            service,
            witness,
            expected,
            marker,
            record_lease: None,
        }
    }
    pub(super) fn is_current(&self) -> bool {
        self.witness.is_current()
    }
}
impl Accounts {
    /// Called after a network task has pinned the authority Store. Simply listing
    /// accounts never locks another workbench out of an otherwise idle library.
    pub(crate) fn prepare_withdrawals(&mut self, store: &mut Store) -> Result<()> {
        if self.reviewed.is_empty() {
            return Ok(());
        }
        let guard = store.pin_service_authority()?;
        self.reviewed.retain(|reference, review| {
            if !review.is_current() {
                return false;
            }
            if let Some(lease) = &review.record_lease {
                return lease.check().is_ok();
            }
            // Another workbench may have rotated this account since it was listed.
            if !store
                .load_outbound_authority(reference)
                .is_ok_and(|r| r.is_some_and(|v| v.value().revision == review.expected))
            {
                return false;
            }
            review.record_lease = store
                .narrow_service_authority(&guard, &[ServiceAuthorityResource::Outbound(*reference)])
                .ok();
            review.record_lease.is_some()
        });
        Ok(())
    }
}
pub(super) struct Withdrawal {
    selection: Selection,
    service: [u8; 32],
    view: Work,
    pending: Option<ServiceUpdateHandle>,
    signed_out: Status,
    witness: AccountRevision,
}
impl Withdrawal {
    pub(super) fn key(&self) -> [u8; 32] {
        self.view.key
    }
    pub(super) fn poll(&mut self, sel: &Selection, service: [u8; 32]) -> Result<Work> {
        if &self.selection != sel || self.service != service {
            return Err(Error::NotApproved.into());
        }
        self.read_receipt();
        if !self.witness.is_current() {
            return Ok(Work {
                key: self.view.key,
                account: None,
                phase: Phase::Unconfirmed,
                browser_url: None,
                status: None,
            });
        }
        Ok(self.view.clone())
    }
    fn read_receipt(&mut self) {
        if let Some(handle) = &mut self.pending {
            match handle.read() {
                Ok(None) => return,
                Ok(Some(Ok(()))) => {
                    self.view.phase = Phase::Done;
                    self.view.status = Some(self.signed_out);
                }
                _ => self.view.phase = Phase::Unconfirmed,
            }
            self.pending = None;
        }
    }
}

impl Workbench {
    pub fn oauth_withdraw(
        &mut self,
        sel: &Selection,
        service: [u8; 32],
        reference: [u8; 32],
        revision: u64,
        key: [u8; 32],
    ) -> Result<Work> {
        self.product_gate().check()?;
        self.oauth_accounts.service(sel, service)?;
        if let Some(previous) = &mut self.oauth_accounts.withdrawal {
            previous.read_receipt();
            if previous.pending.is_some() {
                return Err(Error::Busy.into());
            }
        }
        self.oauth_accounts.remember(key)?;
        let (signed_out, pending, witness) = if let Ok(state) = self.local_state() {
            let witness = state
                .manager
                .as_ref()
                .ok_or(Error::NotApproved)?
                .account_revision();
            (
                self.oauth_sign_out(sel, service, reference, revision)?,
                None,
                witness,
            )
        } else {
            let reviewed = self
                .oauth_accounts
                .reviewed
                .get(&reference)
                .filter(|r| {
                    &r.selection == sel
                        && r.service == service
                        && r.expected == revision
                        && r.is_current()
                        && r.record_lease
                            .as_ref()
                            .is_some_and(|lease| lease.check().is_ok())
                })
                .ok_or(Error::NotApproved)?;
            let handle = self
                .state
                .withdraw_windows_account(reviewed.marker.clone(), revision)?;
            let status = Status {
                reference,
                revision: reviewed.marker.value().revision,
                state: AccountState::SignedOut,
                access_expires_ms: None,
            };
            let witness = reviewed.witness.clone();
            if self
                .oauth_accounts
                .job
                .as_ref()
                .is_some_and(|j| j.view.account == Some(reference))
            {
                self.oauth_accounts.stop();
            }
            (status, Some(handle), witness)
        };
        self.oauth_accounts.reviewed.remove(&reference);
        let ready = pending.is_none();
        let view = Work {
            key,
            account: Some(reference),
            phase: if ready { Phase::Done } else { Phase::Waiting },
            status: ready.then_some(signed_out),
            browser_url: None,
        };
        self.oauth_accounts.withdrawal = Some(Withdrawal {
            selection: sel.clone(),
            service,
            signed_out,
            view: view.clone(),
            pending,
            witness,
        });
        Ok(view)
    }
}
