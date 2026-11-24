//! One reviewed future page. Waiting retains a bounded selection on the
//! original owner. Status reads cannot prepare a source or start a guest.
use super::{
    AccessError, ChangesSelection, Instant, MAX_HOST_JOBS, Prepare, Result, State, Workbench,
};
use morrow_core::{
    changes_metadata::{BookmarkState, ChangesCursor},
    store::ChangesStoreBinding,
};
use morrow_plugin_runtime::manager::SelectionRevision;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangesWaitPhase {
    Waiting,
    Available,
    Prepared,
    Cancelled,
    Expired,
    Changed,
    Unconfirmed,
}
#[derive(Clone, Copy)]
pub struct ChangesWaitView {
    pub submission: [u8; 32],
    pub phase: ChangesWaitPhase,
    pub prepared_key: Option<[u8; 32]>,
}
struct Grant {
    request: Prepare,
    selection: ChangesSelection,
    compatibility: bool,
    head: Option<ChangesCursor>,
    manager: SelectionRevision,
    library: ChangesStoreBinding,
}
pub(super) struct Wait {
    view: ChangesWaitView,
    deadline: Instant,
    grant: Option<Grant>,
}
impl Wait {
    pub(super) fn stop(&mut self) {
        if self.grant.take().is_some() {
            self.view.phase = ChangesWaitPhase::Cancelled;
        }
    }
}
impl Workbench {
    pub fn start_changes_wait(
        &mut self,
        request: Prepare,
        mut selection: ChangesSelection,
        compatibility: bool,
    ) -> Result<ChangesWaitView> {
        self.channel_tasks.touch(false)?;
        let started = Instant::now();
        Self::validate_reviewed_page(&request, &selection)?;
        request.budget.validate()?;
        if request.submission == [0; 32]
            || self.channel_tasks.seen.contains(&request.submission)
            || self.channel_tasks.seen.len() >= MAX_HOST_JOBS
            || request.lifetime_ms == 0
            || request.lifetime_ms > 30_000
            || u64::from(request.lifetime_ms) > request.budget.max_duration_ms
            || u64::from(selection.page_size) > request.budget.max_messages
            || self
                .channel_tasks
                .changes_wait
                .as_ref()
                .is_some_and(|old| old.grant.is_some() && started < old.deadline)
            || self
                .channel_tasks
                .job
                .as_ref()
                .is_some_and(|job| !job.disconnected)
        {
            return Err("reviewed changes wait is invalid, already used, or busy".into());
        }
        let progress = self.inspect_changes_checkpoint(
            &request.package_id,
            request.package_digest,
            request.registry_revision,
            &selection.cards,
            selection.previous_cursor,
        )?;
        if !matches!(
            progress.bookmark,
            BookmarkState::NotSupplied | BookmarkState::Current
        ) {
            return Err("the reviewed bookmark is no longer current".into());
        }
        let state = self.local_state()?;
        let manager = state.manager.as_ref().ok_or("plugin manager unavailable")?;
        let package = manager.installed_package(request.package_digest)?;
        let budget = package
            .channel_declaration()
            .and_then(|p| p.budget.as_ref())
            .ok_or("missing changes budget")?;
        if u64::from(request.lifetime_ms) > budget.max_duration_ms
            || u64::from(selection.page_size) > budget.max_messages
        {
            return Err("changes wait exceeds the selected package budget".into());
        }
        let deadline = started
            .checked_add(Duration::from_millis(u64::from(request.lifetime_ms)))
            .ok_or("invalid wait deadline")?;
        let view = ChangesWaitView {
            submission: request.submission,
            prepared_key: None,
            phase: if progress.pending {
                ChangesWaitPhase::Available
            } else {
                ChangesWaitPhase::Waiting
            },
        };
        selection.previous_cursor = progress.latest;
        let grant = Grant {
            request,
            selection,
            compatibility,
            head: progress.latest,
            manager: manager.selection_revision(),
            library: state.host.store_local().changes_store_binding()?,
        };
        self.channel_tasks.seen.insert(view.submission);
        self.channel_tasks.changes_wait = Some(Wait {
            view,
            deadline,
            grant: Some(grant),
        });
        Ok(view)
    }

    pub fn changes_wait_status(&mut self, key: [u8; 32], cancel: bool) -> Result<ChangesWaitView> {
        self.channel_tasks.touch(cancel)?;
        let Some(wait) = self
            .channel_tasks
            .changes_wait
            .as_ref()
            .filter(|w| w.view.submission == key)
        else {
            return Ok(ChangesWaitView {
                submission: key,
                phase: ChangesWaitPhase::Unconfirmed,
                prepared_key: None,
            });
        };
        let Some(grant) = &wait.grant else {
            return Ok(wait.view);
        };
        let phase = if cancel {
            ChangesWaitPhase::Cancelled
        } else if Instant::now() >= wait.deadline {
            ChangesWaitPhase::Expired
        } else if self.product_gate().closed()
            || !grant.manager.is_current()
            || grant.library.check_live().is_err()
        {
            ChangesWaitPhase::Changed
        } else {
            match self.local_state() {
                Err(error)
                    if matches!(error.downcast_ref::<AccessError>(), Some(AccessError::Busy)) =>
                {
                    ChangesWaitPhase::Waiting
                }
                Err(_) => ChangesWaitPhase::Unconfirmed,
                Ok(state)
                    if state
                        .host
                        .store_local()
                        .validate_changes_store_binding(&grant.library)
                        .is_err() =>
                {
                    ChangesWaitPhase::Changed
                }
                Ok(_) => match self.inspect_changes_checkpoint(
                    &grant.request.package_id,
                    grant.request.package_digest,
                    grant.request.registry_revision,
                    &grant.selection.cards,
                    grant.head,
                ) {
                    Ok(progress) if progress.latest != grant.head => ChangesWaitPhase::Changed,
                    Ok(progress) if progress.pending => ChangesWaitPhase::Available,
                    Ok(_) => ChangesWaitPhase::Waiting,
                    Err(_) => ChangesWaitPhase::Unconfirmed,
                },
            }
        };
        let wait = self
            .channel_tasks
            .changes_wait
            .as_mut()
            .expect("original wait");
        wait.view.phase = phase;
        if !matches!(
            phase,
            ChangesWaitPhase::Waiting | ChangesWaitPhase::Available
        ) {
            wait.grant.take();
        }
        Ok(wait.view)
    }

    pub fn claim_changes_wait(&mut self, key: [u8; 32]) -> Result<State> {
        if self.changes_wait_status(key, false)?.phase != ChangesWaitPhase::Available {
            return Err("changes are not available on the original reviewed wait".into());
        }
        let wait = self
            .channel_tasks
            .changes_wait
            .as_mut()
            .expect("original wait");
        let grant = wait
            .grant
            .take()
            .ok_or("original changes review already consumed")?;
        let deadline = wait.deadline;
        // Mark the single admission consumed before doing anything that may fail.
        wait.view.phase = ChangesWaitPhase::Unconfirmed;
        let state = self.prepare_channel_inner(
            grant.request,
            grant.compatibility,
            Some(grant.selection),
            Some(deadline),
        )?;
        let wait = self
            .channel_tasks
            .changes_wait
            .as_mut()
            .expect("original wait");
        wait.view.phase = ChangesWaitPhase::Prepared;
        wait.view.prepared_key = Some(state.key);
        Ok(state)
    }
}
