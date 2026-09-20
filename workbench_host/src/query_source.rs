#![cfg(test)]
//! One owned SQLite read view supplies every candidate and body for a live query.
//! Snapshot completeness is local evidence; durable recording/independent source proof is a
//! separate adapter. No latest-store body lookup is allowed after this snapshot is pinned.
use crate::{Result, Workbench, WorkbenchState, now, query_plan};
use morrow_core::{
    lifecycle::{GrantKind, InstancePhase},
    store::{CardReadSnapshot, Census, FrozenCard},
};
use morrow_workbench_plugin::{Idea, Request, Response};

struct Live<'a> {
    workbench: &'a mut WorkbenchState,
    snapshot: CardReadSnapshot,
    pending: std::vec::IntoIter<FrozenCard>,
    finished: bool,
}
impl query_plan::Backend for Live<'_> {
    fn next_candidate(&mut self) -> Result<Option<Idea>> {
        loop {
            if let Some(entry) = self.pending.next() {
                // All types participate in the snapshot census. Only this owned type is
                // projected into the workbench guest; unsupported versions fail explicitly.
                if entry.card().summary().type_id != "org.morrow.idea" {
                    continue;
                }
                self.workbench.grant(entry.id(), GrantKind::ReadContent)?;
                let start = self.workbench.start;
                let result = self.workbench.host.read_snapshot_content(
                    self.workbench
                        .pool
                        .root(self.workbench.plugin.as_ref().ok_or("plugin unavailable")?)?
                        .connection(),
                    &self.snapshot,
                    &entry,
                    || now(start),
                );
                self.workbench.revoke(entry.id(), GrantKind::ReadContent)?;
                return Ok(Some(WorkbenchState::decode(&result?)?.idea));
            }
            if self.finished {
                return Ok(None);
            }
            // This controls only page allocation, not total library/query capacity. It can
            // accommodate the original maximum CardRecord even for skipped content types.
            let page = self.snapshot.next_page(128, 16 * 1024 * 1024)?;
            self.finished = page.done;
            self.pending = page.entries.into_iter();
        }
    }
    fn invoke(&mut self, _phase: query_plan::Phase, request: Request) -> Result<Response> {
        self.workbench.run(request)
    }
}
impl WorkbenchState {
    fn query_snapshot(
        &mut self,
        snapshot: CardReadSnapshot,
        conditions: &query_plan::Conditions,
    ) -> Result<(Vec<String>, Census)> {
        // Check ownership even for an empty library, before policy clocks or actual guest work.
        self.host.store_local().validate_card_snapshot(&snapshot)?;
        let manager = self.manager.as_ref().ok_or("plugin manager unavailable")?;
        self.pool.maintain(manager, &mut self.host)?;
        let root = self
            .pool
            .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?;
        let bundle = self.bundle.as_ref().ok_or("plugin unavailable")?;
        let selection = manager
            .selection(&bundle.manifest().package_id)
            .ok_or("plugin unavailable")?;
        if !selection.enabled
            || selection.digest != bundle.digest()
            || !selection.approved.contains(&GrantKind::ReadContent)
            || self.host.connection_phase(root.connection())? != InstancePhase::Ready
        {
            return Err("query read capability unavailable".into());
        }
        let mut source = Live {
            workbench: self,
            snapshot,
            pending: vec![].into_iter(),
            finished: false,
        };
        let result = query_plan::execute(conditions, &mut source)?;
        let census = source.snapshot.finish()?;
        // Explicit success close reports any failure, while all early returns unwind the owned
        // read connection. Release the WAL view before any result transport or later journaling.
        source.snapshot.close()?;
        Ok((result, census))
    }
}

impl Workbench {
    fn query_snapshot(
        &mut self,
        snapshot: CardReadSnapshot,
        conditions: &query_plan::Conditions,
    ) -> Result<(Vec<String>, Census)> {
        self.local_state_mut()?.query_snapshot(snapshot, conditions)
    }
}

#[cfg(test)]
mod tests;
