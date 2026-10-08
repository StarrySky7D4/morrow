//! Fixed trusted administration borrowing the original Manager and Core runtime.
#![deny(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

mod file;
use morrow_agent_catalog_admin_v1 as admin;
use morrow_agent_process_control_v1::Capabilities as ProcessCapabilities;
use morrow_agent_session_exec_v1_r2::{
    Error, Result,
    authority::{Capabilities as SessionCapabilities, SessionExecHost},
};
use morrow_agent_session_process_v1_host::catalog::{
    Approval, Catalog, CatalogManagedPackage, Review, Revisions,
};
use morrow_core::dispatch::HostRuntime;
use morrow_plugin_runtime::manager::{Manager, ManagerError};
use std::{collections::BTreeSet, path::Path};

pub const MAX_MUTATING_REQUESTS: usize = 256;

/// Moves with the complete application owner. No Store, runtime, Manager, or
/// restored live connection is held here; each operation borrows the originals.
pub struct CatalogOwner {
    catalog: Catalog,
    reserved: BTreeSet<String>,
    mutations: BTreeSet<[u8; 16]>,
    uncertain: bool,
}
fn revisions(value: admin::Revisions) -> Revisions {
    Revisions {
        catalog: value.catalog,
        manager: value.manager,
    }
}
fn session_bits(c: SessionCapabilities) -> u16 {
    u16::from(c.session_read)
        | u16::from(c.session_write) << 1
        | u16::from(c.propose) << 2
        | u16::from(c.execute) << 3
        | u16::from(c.retire) << 4
}
fn process_bits(c: ProcessCapabilities) -> u16 {
    u16::from(c.read)
        | u16::from(c.events) << 1
        | u16::from(c.write) << 2
        | u16::from(c.close_input) << 3
        | u16::from(c.interrupt) << 4
        | u16::from(c.terminate) << 5
        | u16::from(c.resize_pty) << 6
}
fn approval(value: &admin::Approval) -> Approval {
    Approval {
        session: SessionCapabilities {
            session_read: value.session_bits & 1 != 0,
            session_write: value.session_bits & 2 != 0,
            propose: value.session_bits & 4 != 0,
            execute: value.session_bits & 8 != 0,
            retire: value.session_bits & 16 != 0,
        },
        process: ProcessCapabilities {
            read: value.process_bits & 1 != 0,
            events: value.process_bits & 2 != 0,
            write: value.process_bits & 4 != 0,
            close_input: value.process_bits & 8 != 0,
            interrupt: value.process_bits & 16 != 0,
            terminate: value.process_bits & 32 != 0,
            resize_pty: value.process_bits & 64 != 0,
        },
        sessions: value.sessions.clone(),
        execution_domain: value.domain.clone(),
    }
}
fn public_approval(value: Approval) -> admin::Approval {
    admin::Approval {
        session_bits: session_bits(value.session),
        process_bits: process_bits(value.process),
        sessions: value.sessions,
        domain: value.execution_domain,
    }
}
fn public_review(value: Review) -> admin::Review {
    admin::Review {
        id: value.id,
        version: value.version,
        full_sha256: value.wrapper_sha256,
        base_sha256: value.base_sha256,
        session_schema: value.session_schema,
        process_schema: value.process_schema,
        session_bits: session_bits(value.declaration.session),
        process_bits: process_bits(value.declaration.process),
        sessions: value.declaration.sessions,
        domain: value.declaration.execution_domain,
    }
}
fn status(error: Error) -> admin::Status {
    use admin::Status;
    match error {
        Error::Invalid | Error::Contract => Status::Invalid,
        Error::Conflict => Status::Conflict,
        Error::Denied => Status::Denied,
        Error::NotFound => Status::NotFound,
        Error::Limit => Status::Limit,
        Error::CommitUnknown => Status::Unknown,
        Error::Storage => Status::Storage,
        _ => Status::Unsupported,
    }
}
fn manager_status(error: ManagerError) -> admin::Status {
    use admin::Status;
    match error {
        ManagerError::Core(morrow_core::Error::CommitUnknown) => Status::Unknown,
        ManagerError::Core(morrow_core::Error::RevisionConflict) => Status::Conflict,
        ManagerError::Core(morrow_core::Error::NotFound) => Status::NotFound,
        ManagerError::Core(morrow_core::Error::Limit) => Status::Limit,
        ManagerError::Core(morrow_core::Error::StorageBusy) => Status::Busy,
        ManagerError::Core(
            morrow_core::Error::Storage | morrow_core::Error::StorageFull | morrow_core::Error::Io,
        ) => Status::Storage,
        ManagerError::Core(morrow_core::Error::Invalid(_) | morrow_core::Error::Integrity) => {
            Status::Denied
        }
        _ => Status::Unsupported,
    }
}
impl CatalogOwner {
    /// Explicit host-only R2 profile inspection. Administrative wire remains process-only.
    pub fn inspect_session_exec(&mut self, manager: &Manager, bytes: &[u8]) -> Result<Review> {
        self.ready(manager).map_err(owner_error)?;
        let review = Catalog::inspect_session_exec(bytes)?;
        self.allowed(&review.id).map_err(owner_error)?;
        Ok(review)
    }
    /// One explicit confirmation installs an R2 wrapper; it does not select or approve it.
    pub fn install_session_exec(&mut self, manager: &mut Manager, request_id: [u8; 16],
        bytes: &[u8], full_sha256: [u8; 32], expected: admin::Revisions) -> Result<Review> {
        self.ready(manager).map_err(owner_error)?;
        self.check(manager, expected).map_err(owner_error)?;
        if request_id == [0; 16] { return Err(Error::Invalid); }
        if self.mutations.contains(&request_id) { return Err(Error::Conflict); }
        if self.mutations.len() >= MAX_MUTATING_REQUESTS { return Err(Error::Limit); }
        let review = Catalog::inspect_session_exec(bytes)?;
        self.allowed(&review.id).map_err(owner_error)?;
        if review.wrapper_sha256 != full_sha256 { return Err(Error::Denied); }
        // Reserve before any durable effect; Unknown cannot replay this confirmation.
        self.mutations.insert(request_id);
        let result = self.catalog.install_session_exec(bytes, full_sha256, manager, revisions(expected));
        self.observe(result).map_err(owner_error)
    }
    /// Explicit trusted R2 profile connection. No process-profile fallback or new Core owner.
    #[allow(clippy::too_many_arguments)]
    pub fn connect_session_exec(&mut self, manager: &mut Manager, runtime: &mut HostRuntime,
        host: &SessionExecHost, id: &str, full_sha256: [u8; 32], expected: admin::Revisions,
        expires: u64, now: u64)
        -> Result<morrow_agent_session_process_v1_host::catalog::CatalogManagedSessionExecPackage> {
        self.ready(manager).map_err(owner_error)?;
        self.check(manager, expected).map_err(owner_error)?;
        self.allowed(id).map_err(owner_error)?;
        let review = self.installed(full_sha256).map_err(owner_error)?;
        if review.id != id { return Err(Error::Denied); }
        self.selected_base(manager, &review).map_err(owner_error)?;
        let result = self.catalog.connect_session_exec(id, full_sha256, manager, runtime, host,
            revisions(expected), expires, now);
        self.observe(result).map_err(owner_error)
    }
    pub fn new(catalog: Catalog) -> Self {
        Self {
            catalog,
            reserved: BTreeSet::new(),
            mutations: BTreeSet::new(),
            uncertain: false,
        }
    }
    pub fn with_reserved(catalog: Catalog, reserved: Vec<String>) -> Result<Self> {
        if reserved.len() > 64
            || reserved.iter().any(|id| {
                id.is_empty()
                    || id.len() > admin::MAX_ID_BYTES
                    || id
                        .chars()
                        .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
            })
        {
            return Err(Error::Invalid);
        }
        let count = reserved.len();
        let reserved: BTreeSet<_> = reserved.into_iter().collect();
        if reserved.len() != count {
            return Err(Error::Invalid);
        }
        let mut owner = Self::new(catalog);
        owner.reserved = reserved;
        Ok(owner)
    }
    pub fn open(root: &Path, create: bool, reserved: Vec<String>) -> Result<Self> {
        Self::with_reserved(Catalog::open(root, create)?, reserved)
    }
    pub fn revisions(&self, manager: &Manager) -> admin::Revisions {
        admin::Revisions {
            catalog: self.catalog.revision(),
            manager: manager.revision(),
        }
    }
    /// One-way trusted veto; live original connections stop before any reopen.
    pub fn invalidate(&mut self) {
        self.uncertain = true;
        self.catalog.invalidate();
    }
    fn observe<T>(&mut self, result: Result<T>) -> std::result::Result<T, admin::Status> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                if matches!(
                    error,
                    Error::CommitUnknown | Error::Storage | Error::Contract
                ) {
                    self.invalidate();
                }
                Err(status(error))
            }
        }
    }
    fn observe_manager<T>(
        &mut self,
        result: morrow_plugin_runtime::manager::Result<T>,
    ) -> std::result::Result<T, admin::Status> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let mapped = manager_status(error);
                if matches!(mapped, admin::Status::Unknown | admin::Status::Storage) {
                    self.invalidate();
                }
                Err(mapped)
            }
        }
    }
    fn ready(&mut self, manager: &Manager) -> std::result::Result<(), admin::Status> {
        if self.uncertain {
            return Err(admin::Status::Unknown);
        }
        // This original Registry call observes its CommitUnknown latch, unlike
        // reading a stale in-memory selection or revision alone.
        self.observe_manager(manager.persisted_snapshot())?;
        let result = self.catalog.page(None, 1, self.catalog.revision());
        self.observe(result)?;
        Ok(())
    }
    fn check(
        &self,
        manager: &Manager,
        expected: admin::Revisions,
    ) -> std::result::Result<(), admin::Status> {
        if expected != self.revisions(manager) {
            return Err(admin::Status::Conflict);
        }
        Ok(())
    }
    fn allowed(&self, id: &str) -> std::result::Result<(), admin::Status> {
        if self.reserved.contains(id) {
            Err(admin::Status::Denied)
        } else {
            Ok(())
        }
    }
    fn installed(&mut self, full_sha: [u8; 32]) -> std::result::Result<Review, admin::Status> {
        let mut cursor = None;
        for _ in 0..4 {
            let result = self
                .catalog
                .page(cursor.as_deref(), 16, self.catalog.revision());
            let page = self.observe(result)?;
            if let Some(entry) = page
                .entries
                .into_iter()
                .find(|entry| entry.review.wrapper_sha256 == full_sha)
            {
                return Ok(entry.review);
            }
            cursor = page.next;
            if cursor.is_none() {
                break;
            }
        }
        Err(admin::Status::NotFound)
    }
    fn selected_base(
        &mut self,
        manager: &Manager,
        review: &Review,
    ) -> std::result::Result<(), admin::Status> {
        self.allowed(&review.id)?;
        let selection = manager
            .selection(&review.id)
            .ok_or(admin::Status::NotFound)?;
        if selection.digest != review.base_sha256 {
            return Err(admin::Status::Conflict);
        }
        if !selection.approved.is_empty() || !selection.approved_io.is_empty() {
            return Err(admin::Status::Denied);
        }
        let base = self.observe_manager(manager.installed_package(review.base_sha256))?;
        if base.manifest().package_id != review.id
            || !base.capabilities().is_empty()
            || !base.io_capabilities().is_empty()
        {
            return Err(admin::Status::Denied);
        }
        Ok(())
    }
    /// Execute the exact typed request once. No dispatch retry or implicit base
    /// enable/approval is performed. Caller owns availability/maintenance gating.
    pub fn execute(&mut self, manager: &mut Manager, request: &admin::Request) -> admin::Outcome {
        if let Err(error) = request.validate() {
            let value = match error {
                admin::Error::Limit => admin::Status::Limit,
                _ => admin::Status::Invalid,
            };
            return admin::Outcome::error(value, self.revisions(manager));
        }
        if let Err(error) = self.ready(manager) {
            return admin::Outcome::error(error, self.revisions(manager));
        }
        let mutation = !matches!(
            &request.action,
            admin::Action::State | admin::Action::Inspect { .. } | admin::Action::Page { .. }
        );
        if mutation {
            if self.mutations.contains(&request.id) {
                return admin::Outcome::error(admin::Status::Conflict, self.revisions(manager));
            }
            if self.mutations.len() >= MAX_MUTATING_REQUESTS {
                return admin::Outcome::error(admin::Status::Limit, self.revisions(manager));
            }
            // Retain even failure IDs; changing a request body cannot spend the
            // same confirmation again. The bound never evicts an old tombstone.
            self.mutations.insert(request.id);
        }
        match self.dispatch(manager, &request.action) {
            Ok(body) => admin::Outcome::ok(self.revisions(manager), body),
            Err(error) => admin::Outcome::error(error, self.revisions(manager)),
        }
    }
    fn dispatch(
        &mut self,
        manager: &mut Manager,
        action: &admin::Action,
    ) -> std::result::Result<admin::Body, admin::Status> {
        use admin::{Action, Body, Status};
        match action {
            Action::State => Ok(Body::None),
            Action::Inspect { path } => {
                let bytes = file::read(path)?;
                let review = Catalog::inspect(&bytes).map_err(status)?;
                self.allowed(&review.id)?;
                Ok(Body::Review(public_review(review)))
            }
            Action::Install {
                path,
                full_sha256,
                revisions: expected,
            } => {
                self.check(manager, *expected)?;
                // Never trust a previous inspection buffer/path identity.
                let bytes = file::read(path)?;
                if morrow_agent_session_exec_v1_r2::hash(&bytes) != *full_sha256 {
                    return Err(Status::Denied);
                }
                let review = Catalog::inspect(&bytes).map_err(status)?;
                self.allowed(&review.id)?;
                let result =
                    self.catalog
                        .install(&bytes, *full_sha256, manager, revisions(*expected));
                Ok(Body::Review(public_review(self.observe(result)?)))
            }
            Action::BaseSelect {
                full_sha256,
                revisions: expected,
            } => {
                self.check(manager, *expected)?;
                let review = self.installed(*full_sha256)?;
                self.allowed(&review.id)?;
                if manager.selection(&review.id).is_some_and(|s| {
                    s.digest == review.base_sha256
                        && (!s.approved.is_empty() || !s.approved_io.is_empty())
                }) {
                    return Err(Status::Denied);
                }
                self.observe_manager(manager.select_digest(review.base_sha256, expected.manager))?;
                Ok(Body::None)
            }
            Action::BaseEnable {
                id,
                full_sha256,
                enabled,
                revisions: expected,
            } => {
                self.check(manager, *expected)?;
                self.allowed(id)?;
                let review = self.installed(*full_sha256)?;
                if &review.id != id {
                    return Err(Status::Denied);
                }
                self.selected_base(manager, &review)?;
                self.observe_manager(manager.set_enabled(
                    id,
                    review.base_sha256,
                    *enabled,
                    expected.manager,
                ))?;
                Ok(Body::None)
            }
            Action::WrapperSelect {
                full_sha256,
                revisions: expected,
            } => {
                self.check(manager, *expected)?;
                let review = self.installed(*full_sha256)?;
                self.selected_base(manager, &review)?;
                let result = self
                    .catalog
                    .select(*full_sha256, manager, revisions(*expected));
                self.observe(result)?;
                Ok(Body::None)
            }
            Action::Approve {
                id,
                full_sha256,
                approval: value,
                revisions: expected,
            } => {
                self.check(manager, *expected)?;
                self.allowed(id)?;
                let review = self.installed(*full_sha256)?;
                if &review.id != id {
                    return Err(Status::Denied);
                }
                self.selected_base(manager, &review)?;
                let result = self.catalog.approve(
                    id,
                    *full_sha256,
                    approval(value),
                    manager,
                    revisions(*expected),
                );
                self.observe(result)?;
                Ok(Body::None)
            }
            Action::WrapperEnable {
                id,
                full_sha256,
                enabled,
                revisions: expected,
            } => {
                self.check(manager, *expected)?;
                self.allowed(id)?;
                let review = self.installed(*full_sha256)?;
                if &review.id != id {
                    return Err(Status::Denied);
                }
                self.selected_base(manager, &review)?;
                let result = self.catalog.set_enabled(
                    id,
                    *full_sha256,
                    *enabled,
                    manager,
                    revisions(*expected),
                );
                self.observe(result)?;
                Ok(Body::None)
            }
            Action::Remove {
                id,
                full_sha256,
                revisions: expected,
            } => {
                self.check(manager, *expected)?;
                self.allowed(id)?;
                let review = self.installed(*full_sha256)?;
                if &review.id != id {
                    return Err(Status::Denied);
                }
                self.selected_base(manager, &review)?;
                let result = self
                    .catalog
                    .remove(id, *full_sha256, manager, revisions(*expected));
                self.observe(result)?;
                Ok(Body::None)
            }
            Action::Page {
                after,
                limit,
                revisions: expected,
            } => {
                self.check(manager, *expected)?;
                let result = self
                    .catalog
                    .page(after.as_deref(), *limit, expected.catalog);
                let page = self.observe(result)?;
                let entries = page
                    .entries
                    .into_iter()
                    .map(|entry| {
                        let selection = manager
                            .selection(&entry.review.id)
                            .filter(|s| s.digest == entry.review.base_sha256);
                        admin::Entry {
                            base_selected: selection.is_some(),
                            base_enabled: selection.is_some_and(|s| s.enabled),
                            review: public_review(entry.review),
                            selected: entry.selected,
                            enabled: entry.enabled,
                            approval: entry.approval.map(public_approval),
                        }
                    })
                    .collect();
                Ok(Body::Page {
                    entries,
                    next: page.next,
                })
            }
        }
    }
    /// Trusted borrowed execution preparation, absent from administrative wire.
    /// The returned package retains the original managed Arc<Connection>.
    #[allow(clippy::too_many_arguments)]
    pub fn connect(
        &mut self,
        manager: &mut Manager,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        id: &str,
        full_sha256: [u8; 32],
        expected: admin::Revisions,
        expires: u64,
        now: u64,
    ) -> Result<CatalogManagedPackage> {
        let check = (|| {
            self.ready(manager)?;
            self.check(manager, expected)?;
            self.allowed(id)?;
            let review = self.installed(full_sha256)?;
            if review.id != id {
                return Err(admin::Status::Denied);
            }
            self.selected_base(manager, &review)
        })();
        check.map_err(|value| match value {
            admin::Status::Unknown => Error::CommitUnknown,
            admin::Status::Conflict => Error::Conflict,
            admin::Status::NotFound => Error::NotFound,
            admin::Status::Limit => Error::Limit,
            admin::Status::Storage => Error::Storage,
            _ => Error::Denied,
        })?;
        let result = self.catalog.connect(
            id,
            full_sha256,
            manager,
            runtime,
            host,
            revisions(expected),
            expires,
            now,
        );
        if result
            .as_ref()
            .is_err_and(|e| matches!(e, Error::CommitUnknown | Error::Storage | Error::Contract))
        {
            self.invalidate();
        }
        result
    }
}

fn owner_error(value: admin::Status) -> Error {
    match value {
        admin::Status::Unknown => Error::CommitUnknown,
        admin::Status::Conflict => Error::Conflict,
        admin::Status::NotFound => Error::NotFound,
        admin::Status::Limit => Error::Limit,
        admin::Status::Storage => Error::Storage,
        admin::Status::Invalid => Error::Invalid,
        _ => Error::Denied,
    }
}
