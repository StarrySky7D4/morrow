//! Native UI management borrows the original authoritative owner. No guest lane.
use crate::{Result, Workbench, WorkbenchState};
use morrow_agent_catalog_admin_v1 as admin;
use morrow_agent_catalog_owner_v1::CatalogOwner;
use morrow_plugin_runtime::manager::Manager;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
};

/// Lazy sidecar metadata; opening settings does not duplicate a content runtime.
pub(crate) struct Slot {
    root: PathBuf,
    owner: Option<CatalogOwner>,
    failure: Option<admin::Status>,
}
impl Slot {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            owner: None,
            failure: None,
        }
    }
    pub(crate) fn invalidate(&mut self) {
        // Catalog Drop stops every lease it issued. This is no native exit receipt.
        self.owner = None;
        self.failure = Some(admin::Status::Unknown);
    }
    /// Same lazily opened persistent catalog, only to trusted sibling modules.
    pub(crate) fn opened(&mut self, reserved: Vec<String>) -> Result<&mut CatalogOwner> {
        if self.failure.is_some() {
            return Err("agent catalog is unavailable or Unknown".into());
        }
        if self.owner.is_none() {
            match CatalogOwner::open(&self.root, true, reserved) {
                Ok(owner) => self.owner = Some(owner),
                Err(_) => {
                    self.failure = Some(admin::Status::Storage);
                    return Err("agent catalog storage unavailable".into());
                }
            }
        }
        self.owner
            .as_mut()
            .ok_or_else(|| "agent catalog owner unavailable".into())
    }
    /// Trusted execution preparation on this same lazy catalog and original Core.
    /// Administrative approval remains configuration; this creates no executor grant.
    #[cfg(windows)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn connect(
        &mut self,
        manager: &mut Manager,
        runtime: &mut morrow_core::dispatch::HostRuntime,
        host: &morrow_agent_session_exec_v1_r2::authority::SessionExecHost,
        id: &str,
        full_sha256: [u8; 32],
        expected: admin::Revisions,
        expires: u64,
        now: u64,
        reserved: Vec<String>,
    ) -> morrow_agent_session_exec_v1_r2::Result<
        morrow_agent_session_process_v1_host::catalog::CatalogManagedPackage,
    > {
        use morrow_agent_session_exec_v1_r2::Error;
        if let Some(status) = self.failure {
            return Err(if status == admin::Status::Storage {
                Error::Storage
            } else {
                Error::CommitUnknown
            });
        }
        if self.owner.is_none() {
            match CatalogOwner::open(&self.root, true, reserved) {
                Ok(owner) => self.owner = Some(owner),
                Err(_) => {
                    self.failure = Some(admin::Status::Storage);
                    return Err(Error::Storage);
                }
            }
        }
        let connected = catch_unwind(AssertUnwindSafe(|| {
            self.owner.as_mut().expect("opened above").connect(
                manager,
                runtime,
                host,
                id,
                full_sha256,
                expected,
                expires,
                now,
            )
        }));
        match connected {
            Ok(result) => result,
            Err(_) => {
                self.invalidate();
                Err(Error::CommitUnknown)
            }
        }
    }
    fn execute(
        &mut self,
        manager: &mut Manager,
        request: &admin::Request,
        reserved: Vec<String>,
    ) -> admin::Outcome {
        if let Some(status) = self.failure {
            return failure(status, manager.revision());
        }
        if self.owner.is_none() {
            match CatalogOwner::open(&self.root, true, reserved) {
                Ok(owner) => self.owner = Some(owner),
                Err(_) => {
                    // Do not retry opening or replace the failed persistent owner.
                    self.failure = Some(admin::Status::Storage);
                    return failure(admin::Status::Storage, manager.revision());
                }
            }
        }
        let executed = catch_unwind(AssertUnwindSafe(|| {
            self.owner
                .as_mut()
                .expect("opened above")
                .execute(manager, request)
        }));
        match executed {
            Ok(outcome) => outcome,
            Err(_) => {
                self.invalidate();
                failure(admin::Status::Unknown, manager.revision())
            }
        }
    }
}
impl Workbench {
    /// Explicit single-R2 profile inspection, without the process-profile wire
    /// fallback. Review data alone does not select, enable or approve anything.
    pub fn inspect_agent_session_exec(
        &mut self,
        bytes: &[u8],
    ) -> Result<(
        morrow_agent_session_process_v1_host::catalog::Review,
        admin::Revisions,
    )> {
        self.session_exec_catalog(bytes, None)
    }
    /// One trusted confirmation installs only. Existing explicit native admin
    /// steps still select the base/wrapper, approve the ceiling and enable it.
    pub fn install_agent_session_exec(
        &mut self,
        request_id: [u8; 16],
        bytes: &[u8],
        full_sha256: [u8; 32],
        expected: admin::Revisions,
    ) -> Result<(
        morrow_agent_session_process_v1_host::catalog::Review,
        admin::Revisions,
    )> {
        self.session_exec_catalog(bytes, Some((request_id, full_sha256, expected)))
    }
    fn session_exec_catalog(
        &mut self,
        bytes: &[u8],
        install: Option<([u8; 16], [u8; 32], admin::Revisions)>,
    ) -> Result<(
        morrow_agent_session_process_v1_host::catalog::Review,
        admin::Revisions,
    )> {
        self.state.try_reclaim()?;
        self.state.require_writable()?;
        let gate = self.state.product_gate.clone();
        gate.check()?;
        let state = self.state.local_mut()?;
        if install.is_some() {
            state.host.prepare_write()?;
        }
        gate.check()?;
        let mut reserved = vec!["org.morrow.workbench".to_owned()];
        if let Some(bundle) = &state.bundle {
            reserved.push(bundle.manifest().package_id.clone());
        }
        reserved.sort();
        reserved.dedup();
        let manager = state.manager.as_mut().ok_or("plugin manager unavailable")?;
        let slot = state
            .agent_catalog
            .as_mut()
            .ok_or("agent catalog unavailable")?;
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<_> {
            let owner = slot.opened(reserved)?;
            let review = match install {
                Some((id, digest, expected)) => {
                    owner.install_session_exec(manager, id, bytes, digest, expected)
                }
                None => owner.inspect_session_exec(manager, bytes),
            }
            .map_err(|e| format!("original session-exec catalog: {e:?}"))?;
            Ok((review, owner.revisions(manager)))
        }));
        if gate.closed() {
            slot.invalidate();
            return Err("agent catalog delivery lost; Unknown, no replay".into());
        }
        match result {
            Ok(value) => value,
            Err(_) => {
                slot.invalidate();
                Err("agent catalog panicked; Unknown, no replay".into())
            }
        }
    }
}
fn failure(status: admin::Status, manager: u64) -> admin::Outcome {
    admin::Outcome {
        status,
        revisions: admin::Revisions {
            catalog: 0,
            manager,
        },
        body: admin::Body::None,
    }
}
impl WorkbenchState {
    fn agent_catalog_request(
        &mut self,
        request: &admin::Request,
        gate: &crate::product_gate::ProductGate,
    ) -> admin::Outcome {
        let manager_revision = self.manager.as_ref().map_or(0, Manager::revision);
        if self.agent_catalog.is_none() {
            return failure(admin::Status::Unsupported, manager_revision);
        }
        if request.is_mutation() && self.host.prepare_write().is_err() {
            return failure(admin::Status::RecoveryRequired, manager_revision);
        }
        if gate.closed() {
            if let Some(slot) = self.agent_catalog.as_mut() {
                slot.invalidate();
            }
            return failure(admin::Status::Unknown, manager_revision);
        }
        let mut reserved = vec!["org.morrow.workbench".to_owned()];
        if let Some(bundle) = &self.bundle {
            reserved.push(bundle.manifest().package_id.clone());
        }
        reserved.sort();
        reserved.dedup();
        let Some(manager) = self.manager.as_mut() else {
            return failure(admin::Status::OwnerUnavailable, manager_revision);
        };
        self.agent_catalog
            .as_mut()
            .expect("checked above")
            .execute(manager, request, reserved)
    }
}
struct NativeTarget<'a>(&'a mut Workbench);
impl admin::Target for NativeTarget<'_> {
    fn execute(&mut self, request: &admin::Request) -> admin::Outcome {
        let gate = self.0.state.product_gate.clone();
        if gate.closed() {
            return failure(admin::Status::Unknown, 0);
        }
        let state = match self.0.local_state_mut() {
            Ok(state) => state,
            Err(error) => {
                use crate::io_tasks::AccessError;
                let status = match error.downcast_ref::<AccessError>() {
                    Some(AccessError::Busy | AccessError::UnacknowledgedTask) => {
                        admin::Status::Busy
                    }
                    Some(AccessError::RecoveryRequired) => admin::Status::RecoveryRequired,
                    Some(AccessError::OwnerUnavailable) => admin::Status::OwnerUnavailable,
                    Some(AccessError::StaleTask) => admin::Status::Conflict,
                    None if gate.closed() => admin::Status::Unknown,
                    None => admin::Status::Storage,
                };
                return failure(status, 0);
            }
        };
        let outcome = state.agent_catalog_request(request, &gate);
        // The supervisor can revoke during a file read or durable publication.
        // A lost controller never receives a success acknowledgement of an effect.
        if gate.closed() {
            if let Some(slot) = state.agent_catalog.as_mut() {
                slot.invalidate();
            }
            return failure(admin::Status::Unknown, outcome.revisions.manager);
        }
        outcome
    }
}
pub(crate) fn respond(host: &mut Workbench, bytes: &[u8]) -> Result<Vec<u8>> {
    let gate = host.state.product_gate.clone();
    match admin::respond(&mut NativeTarget(host), bytes) {
        Ok(reply) => {
            // Final delivery gate follows encoding. Never run a submitted action again.
            if gate.closed() {
                if let Ok(state) = host.state.local_mut() {
                    if let Some(slot) = state.agent_catalog.as_mut() {
                        slot.invalidate();
                    }
                }
                let request = admin::Request::decode(bytes)?;
                return Ok(
                    admin::Reply::new(&request, failure(admin::Status::Unknown, 0))?
                        .encode_for(&request)?,
                );
            }
            Ok(reply)
        }
        Err(_) => {
            if let Ok(reply) = admin::reject_frame(bytes, admin::Revisions::default()) {
                return Ok(reply);
            }
            // An unidentifiable frame cannot receive a correlated admin receipt.
            // Keep the original pipe alive with a bounded generic failure envelope;
            // the profile client rejects it and fences any unconfirmed mutation.
            let mut output = capnp::message::Builder::new_default();
            let mut reply = output.init_root::<crate::host_capnp::response::Builder>();
            reply.set_version(1);
            reply.set_digest(&crate::host_protocol_digest());
            reply.set_error("Invalid native agent administration frame");
            Ok(capnp::serialize::write_message_to_words(&output))
        }
    }
}
