//! Durable wrapper ceilings. This is neither a Core Store nor restorable live authority.
use crate::*;
use morrow_core::lifecycle::Revocation;
use morrow_plugin_runtime::manager::Manager;
use prost::Message;
use std::{
    cell::Cell,
    sync::{Weak, atomic::AtomicBool},
};
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::{MAX_ARCHIVE_TOTAL_BYTES, MAX_WRAPPER_ARCHIVE_BYTES, NativeCatalogStorage};

pub const MAX_ENTRIES: usize = 64;
pub const MAX_RAW_SNAPSHOT: usize = 384 * 1024;
const MAGIC: &[u8; 8] = b"MROWAC01";
const HEADER: usize = 50;
/// Trusted exclusive storage. Full legal wrapper archives are separate from the compact snapshot.
pub trait WrapperCatalogStorage: Send {
    fn read(&self) -> Result<Option<Vec<u8>>>;
    fn publish(&mut self, bytes: &[u8]) -> Result<()>;
    fn load(&self, sha: [u8; 32]) -> Result<AgentProcessPackage>;
    fn install(&self, package: &AgentProcessPackage) -> Result<()>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Revisions {
    pub catalog: u64,
    pub manager: u64,
}
#[derive(Clone)]
pub struct Approval {
    pub session: SessionCapabilities,
    pub process: ProcessCapabilities,
    pub sessions: Vec<String>,
    pub execution_domain: String,
}
#[derive(Clone)]
pub struct Review {
    pub id: String,
    pub version: String,
    pub wrapper_sha256: [u8; 32],
    pub base_sha256: [u8; 32],
    pub session_schema: [u8; 32],
    pub process_schema: [u8; 32],
    pub declaration: Declaration,
}
pub struct Entry {
    pub review: Review,
    pub selected: bool,
    pub enabled: bool,
    pub approval: Option<Approval>,
}
pub struct Page {
    pub revision: u64,
    pub entries: Vec<Entry>,
    pub next: Option<String>,
}
#[derive(Clone)]
struct Selection {
    sha: [u8; 32],
    enabled: bool,
    approval: Option<Approval>,
}
#[derive(Clone, Default)]
struct State {
    revision: u64,
    installed: BTreeMap<[u8; 32], Review>,
    selections: BTreeMap<String, Selection>,
}
#[derive(Clone, PartialEq, Message)]
struct Snapshot {
    #[prost(uint32, tag = "1")]
    version: u32,
    #[prost(uint64, tag = "2")]
    revision: u64,
    #[prost(message, repeated, tag = "3")]
    packages: Vec<PackageRecord>,
    #[prost(message, repeated, tag = "4")]
    selections: Vec<SelectionRecord>,
}
#[derive(Clone, PartialEq, Message)]
struct PackageRecord {
    #[prost(bytes = "vec", tag = "1")]
    wrapper_sha: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    base_sha: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    session_schema: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    process_schema: Vec<u8>,
    #[prost(string, tag = "5")]
    id: String,
    #[prost(string, tag = "6")]
    version: String,
    #[prost(uint32, tag = "7")]
    session: u32,
    #[prost(uint32, tag = "8")]
    process: u32,
    #[prost(string, repeated, tag = "9")]
    sessions: Vec<String>,
    #[prost(string, tag = "10")]
    domain: String,
}
#[derive(Clone, PartialEq, Message)]
struct SelectionRecord {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(bytes = "vec", tag = "2")]
    sha: Vec<u8>,
    #[prost(bool, tag = "3")]
    enabled: bool,
    #[prost(message, optional, tag = "4")]
    approval: Option<ApprovalRecord>,
}
#[derive(Clone, PartialEq, Message)]
struct ApprovalRecord {
    #[prost(uint32, tag = "1")]
    session: u32,
    #[prost(uint32, tag = "2")]
    process: u32,
    #[prost(string, repeated, tag = "3")]
    sessions: Vec<String>,
    #[prost(string, tag = "4")]
    domain: String,
}
fn review(p: &AgentProcessPackage) -> Review {
    Review {
        id: p.base().manifest().package_id.clone(),
        version: p.base().manifest().package_version.clone(),
        wrapper_sha256: p.review_sha256(),
        base_sha256: p.base_sha256(),
        session_schema: morrow_agent_session_exec_v1_r2::schema_digest(),
        process_schema: morrow_agent_process_control_v1::schema_digest(),
        declaration: p.declaration().clone(),
    }
}
fn record(r: &Review) -> PackageRecord {
    PackageRecord {
        wrapper_sha: r.wrapper_sha256.to_vec(),
        base_sha: r.base_sha256.to_vec(),
        session_schema: r.session_schema.to_vec(),
        process_schema: r.process_schema.to_vec(),
        id: r.id.clone(),
        version: r.version.clone(),
        session: package::session_bits(r.declaration.session),
        process: package::process_bits(r.declaration.process),
        sessions: r.declaration.sessions.clone(),
        domain: r.declaration.execution_domain.clone(),
    }
}
fn approval_record(a: &Approval) -> ApprovalRecord {
    ApprovalRecord {
        session: package::session_bits(a.session),
        process: package::process_bits(a.process),
        sessions: a.sessions.clone(),
        domain: a.execution_domain.clone(),
    }
}
fn check_approval(a: &Approval, r: &Review) -> Result<()> {
    if !a.session.session_read
        || !subset_session(a.session, r.declaration.session)
        || !subset_process(a.process, r.declaration.process)
        || a.execution_domain != r.declaration.execution_domain
        || a.sessions.is_empty()
        || a.sessions.len() > 16
        || a.sessions.windows(2).any(|s| s[0] >= s[1])
        || a.sessions
            .iter()
            .any(|s| !r.declaration.sessions.contains(s))
    {
        return Err(Error::Denied);
    }
    Ok(())
}
fn decode_approval(a: ApprovalRecord, r: &Review) -> Result<Approval> {
    let session = r.declaration.session;
    let process = r.declaration.process;
    let out = Approval {
        session: SessionCapabilities {
            session_read: a.session & 1 != 0,
            session_write: a.session & 2 != 0,
            propose: a.session & 4 != 0,
            execute: a.session & 8 != 0,
            retire: a.session & 16 != 0,
        },
        process: ProcessCapabilities {
            read: a.process & 1 != 0,
            events: a.process & 2 != 0,
            write: a.process & 4 != 0,
            close_input: a.process & 8 != 0,
            interrupt: a.process & 16 != 0,
            terminate: a.process & 32 != 0,
            resize_pty: a.process & 64 != 0,
        },
        sessions: a.sessions,
        execution_domain: a.domain,
    };
    if a.session & !package::session_bits(session) != 0
        || a.process & !package::process_bits(process) != 0
    {
        return Err(Error::Contract);
    }
    check_approval(&out, r)?;
    Ok(out)
}
fn encode(state: &State) -> Result<Vec<u8>> {
    let raw = Snapshot {
        version: 1,
        revision: state.revision,
        packages: state.installed.values().map(record).collect(),
        selections: state
            .selections
            .iter()
            .map(|(id, s)| SelectionRecord {
                id: id.clone(),
                sha: s.sha.to_vec(),
                enabled: s.enabled,
                approval: s.approval.as_ref().map(approval_record),
            })
            .collect(),
    }
    .encode_to_vec();
    if raw.len() > MAX_RAW_SNAPSHOT {
        return Err(Error::Limit);
    }
    let compressed = lz4_flex::block::compress(&raw);
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    out.extend_from_slice(&morrow_agent_session_exec_v1_r2::hash(&raw));
    out.extend_from_slice(&compressed);
    if out.len() > morrow_core::plugin_package::registry::MAX_CONTAINER {
        return Err(Error::Limit);
    }
    Ok(out)
}
fn load(storage: &dyn WrapperCatalogStorage, bytes: &[u8]) -> Result<State> {
    if bytes.len() < HEADER
        || bytes.len() > morrow_core::plugin_package::registry::MAX_CONTAINER
        || &bytes[..8] != MAGIC
        || bytes[8..10] != 1u16.to_le_bytes()
    {
        return Err(Error::Contract);
    }
    let size = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    if size > MAX_RAW_SNAPSHOT
        || u32::from_le_bytes(bytes[14..18].try_into().unwrap()) as usize != bytes.len() - HEADER
    {
        return Err(Error::Limit);
    }
    let raw = lz4_flex::block::decompress(&bytes[HEADER..], size).map_err(|_| Error::Contract)?;
    if raw.len() != size || morrow_agent_session_exec_v1_r2::hash(&raw).as_slice() != &bytes[18..50]
    {
        return Err(Error::Contract);
    }
    let snapshot = Snapshot::decode(raw.as_slice()).map_err(|_| Error::Contract)?;
    if snapshot.encode_to_vec() != raw
        || snapshot.version != 1
        || snapshot.revision == 0
        || snapshot.packages.len() > MAX_ENTRIES
        || snapshot.selections.len() > MAX_ENTRIES
        || snapshot
            .packages
            .windows(2)
            .any(|w| w[0].wrapper_sha >= w[1].wrapper_sha)
        || snapshot.selections.windows(2).any(|w| w[0].id >= w[1].id)
    {
        return Err(Error::Contract);
    }
    let mut state = State {
        revision: snapshot.revision,
        ..State::default()
    };
    for saved in snapshot.packages {
        let sha: [u8; 32] = saved
            .wrapper_sha
            .as_slice()
            .try_into()
            .map_err(|_| Error::Contract)?;
        let p = storage.load(sha)?;
        let r = review(&p);
        if record(&r) != saved || r.wrapper_sha256 != sha {
            return Err(Error::Contract);
        }
        state.installed.insert(sha, r);
    }
    for saved in snapshot.selections {
        let sha = saved
            .sha
            .as_slice()
            .try_into()
            .map_err(|_| Error::Contract)?;
        let r = state.installed.get(&sha).ok_or(Error::Contract)?;
        if saved.id != r.id || saved.enabled && saved.approval.is_none() {
            return Err(Error::Contract);
        }
        let approval = saved.approval.map(|a| decode_approval(a, r)).transpose()?;
        state.selections.insert(
            saved.id,
            Selection {
                sha,
                enabled: saved.enabled,
                approval,
            },
        );
    }
    Ok(state)
}
struct Life {
    active: AtomicBool,
    epoch: AtomicU64,
}
struct InstanceLease {
    catalog: Weak<Life>,
    epoch: u64,
    core: Revocation,
    cancel: Cancellation,
    active: AtomicBool,
}
impl InstanceLease {
    fn live(&self) -> bool {
        self.active.load(Ordering::Acquire)
            && !self.core.is_revoked()
            && self.catalog.upgrade().is_some_and(|c| {
                c.active.load(Ordering::Acquire) && c.epoch.load(Ordering::Acquire) == self.epoch
            })
    }
    fn stop(&self) {
        self.active.store(false, Ordering::Release);
        self.core.revoke();
        self.cancel.cancel();
    }
}
pub struct Catalog {
    storage: Box<dyn WrapperCatalogStorage>,
    state: State,
    poisoned: Cell<bool>,
    life: Arc<Life>,
    instances: Vec<Weak<InstanceLease>>,
}
impl Catalog {
    pub fn from_storage(storage: Box<dyn WrapperCatalogStorage>) -> Result<Self> {
        let state = match storage.read()? {
            Some(bytes) => load(storage.as_ref(), &bytes)?,
            None => State::default(),
        };
        Ok(Self {
            storage,
            state,
            poisoned: Cell::new(false),
            life: Arc::new(Life {
                active: AtomicBool::new(true),
                epoch: AtomicU64::new(1),
            }),
            instances: vec![],
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(root: &std::path::Path, create: bool) -> Result<Self> {
        Self::from_storage(Box::new(NativeCatalogStorage::open(root, create)?))
    }
    pub fn revision(&self) -> u64 {
        self.state.revision
    }
    /// A trusted owner observed an uncertain adjacent Manager operation.
    /// Stop original live authority without publishing or replaying any effect.
    pub fn invalidate(&self) {
        self.poison();
    }
    pub fn inspect(bytes: &[u8]) -> Result<Review> {
        let prepared =
            PreparedPackage::new(AgentProcessPackage::decode(bytes)?, Limits::default())?;
        Ok(review(prepared.package()))
    }
    /// Explicit single-R2-import inspection. The default process profile is unchanged.
    /// Profile identity is bound by the complete immutable module/wrapper bytes;
    /// this never infers or falls back from a failed process inspection.
    pub fn inspect_session_exec(bytes: &[u8]) -> Result<Review> {
        let prepared = PreparedSessionExecPackage::new(
            AgentProcessPackage::decode(bytes)?, Limits::default())?;
        Ok(review(prepared.package()))
    }
    fn ready(&self) -> Result<()> {
        if self.poisoned.get() {
            Err(Error::CommitUnknown)
        } else {
            Ok(())
        }
    }
    fn check(&self, manager: &Manager, expected: Revisions) -> Result<()> {
        self.ready()?;
        if expected.catalog != self.revision() || expected.manager != manager.revision() {
            return Err(Error::Conflict);
        }
        Ok(())
    }
    fn current_base(&self, manager: &Manager, r: &Review) -> Result<()> {
        if manager
            .selection(&r.id)
            .is_none_or(|s| s.digest != r.base_sha256)
        {
            return Err(Error::Denied);
        }
        let base = match manager.installed_package(r.base_sha256) {
            Ok(base) => base,
            Err(error) => {
                self.poison();
                return Err(match error {
                    morrow_plugin_runtime::manager::ManagerError::Core(
                        morrow_core::Error::CommitUnknown,
                    ) => Error::CommitUnknown,
                    morrow_plugin_runtime::manager::ManagerError::Core(
                        morrow_core::Error::Invalid(_) | morrow_core::Error::Integrity,
                    ) => Error::Contract,
                    _ => Error::Storage,
                });
            }
        };
        let p = self.observe_storage(self.storage.load(r.wrapper_sha256))?;
        if base.archive() != p.base().archive() || record(&review(&p)) != record(r) {
            self.poison();
            return Err(Error::Contract);
        }
        Ok(())
    }
    fn stop_all(&self) {
        for lease in self.instances.iter().filter_map(Weak::upgrade) {
            lease.stop();
        }
    }
    fn poison(&self) {
        self.poisoned.set(true);
        self.life.active.store(false, Ordering::Release);
        self.stop_all();
    }
    fn observe_storage<T>(&self, result: Result<T>) -> Result<T> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                self.poison();
                Err(error)
            }
        }
    }
    fn publish(&mut self, mut next: State) -> Result<()> {
        next.revision = self.state.revision.checked_add(1).ok_or(Error::Limit)?;
        let bytes = encode(&next)?;
        self.stop_all();
        self.life.epoch.fetch_add(1, Ordering::AcqRel);
        if let Err(error) = self.storage.publish(&bytes) {
            self.poison();
            return Err(error);
        }
        self.state = next;
        Ok(())
    }
    pub fn install(
        &mut self,
        bytes: &[u8],
        expected_sha: [u8; 32],
        manager: &mut Manager,
        expected: Revisions,
    ) -> Result<Review> {
        self.check(manager, expected)?;
        let r = Self::inspect(bytes)?;
        self.install_reviewed(bytes, expected_sha, manager, r)
    }
    /// Explicit R2 profile installation. It neither approves nor selects the wrapper.
    pub fn install_session_exec(&mut self, bytes: &[u8], expected_sha: [u8; 32],
        manager: &mut Manager, expected: Revisions) -> Result<Review> {
        self.check(manager, expected)?;
        let r = Self::inspect_session_exec(bytes)?;
        self.install_reviewed(bytes, expected_sha, manager, r)
    }
    fn install_reviewed(&mut self, bytes: &[u8], expected_sha: [u8; 32],
        manager: &mut Manager, r: Review) -> Result<Review> {
        if r.wrapper_sha256 != expected_sha {
            return Err(Error::Denied);
        }
        if self.state.installed.contains_key(&expected_sha) {
            let p = self.observe_storage(self.storage.load(expected_sha))?;
            if p.archive() != bytes {
                self.poison();
                return Err(Error::Contract);
            }
            return Ok(r);
        }
        if self.state.installed.len() >= MAX_ENTRIES {
            return Err(Error::Limit);
        }
        let p = AgentProcessPackage::decode(bytes)?;
        self.observe_storage(self.storage.install(&p))?; // Full-file quota includes immutable orphans.
        if let Err(error) = manager.install_package(p.base().archive()) {
            self.poison();
            return Err(match error {
                morrow_plugin_runtime::manager::ManagerError::Core(
                    morrow_core::Error::CommitUnknown,
                ) => Error::CommitUnknown,
                _ => Error::Storage,
            });
        }
        let mut next = self.state.clone();
        next.installed.insert(expected_sha, r.clone());
        self.publish(next)?;
        Ok(r)
    }
    fn selected(&self, id: &str, sha: [u8; 32]) -> Result<(&Selection, &Review)> {
        let s = self.state.selections.get(id).ok_or(Error::NotFound)?;
        if s.sha != sha {
            return Err(Error::Conflict);
        }
        Ok((s, self.state.installed.get(&sha).ok_or(Error::Contract)?))
    }
    fn revoke_base(&self, manager: &mut Manager, r: &Review, expected: Revisions) -> Result<()> {
        self.current_base(manager, r)?;
        manager
            .revoke_agent_session_process(&r.id, r.base_sha256, expected.manager)
            .map_err(|_| Error::Denied)
    }
    pub fn select(
        &mut self,
        sha: [u8; 32],
        manager: &mut Manager,
        expected: Revisions,
    ) -> Result<()> {
        self.check(manager, expected)?;
        let r = self
            .state
            .installed
            .get(&sha)
            .ok_or(Error::NotFound)?
            .clone();
        self.revoke_base(manager, &r, expected)?;
        let mut next = self.state.clone();
        next.selections.insert(
            r.id,
            Selection {
                sha,
                enabled: false,
                approval: None,
            },
        );
        self.publish(next)
    }
    pub fn approve(
        &mut self,
        id: &str,
        sha: [u8; 32],
        approval: Approval,
        manager: &mut Manager,
        expected: Revisions,
    ) -> Result<()> {
        self.check(manager, expected)?;
        let (_, r) = self.selected(id, sha)?;
        check_approval(&approval, r)?;
        let r = r.clone();
        self.revoke_base(manager, &r, expected)?;
        let mut next = self.state.clone();
        let s = next.selections.get_mut(id).ok_or(Error::NotFound)?;
        s.approval = Some(approval);
        s.enabled = false;
        self.publish(next)
    }
    pub fn set_enabled(
        &mut self,
        id: &str,
        sha: [u8; 32],
        enabled: bool,
        manager: &mut Manager,
        expected: Revisions,
    ) -> Result<()> {
        self.check(manager, expected)?;
        let (s, r) = self.selected(id, sha)?;
        if enabled && (s.approval.is_none() || manager.selection(id).is_none_or(|s| !s.enabled)) {
            return Err(Error::Denied);
        }
        let r = r.clone();
        self.revoke_base(manager, &r, expected)?;
        let mut next = self.state.clone();
        next.selections.get_mut(id).ok_or(Error::NotFound)?.enabled = enabled;
        self.publish(next)
    }
    pub fn remove(
        &mut self,
        id: &str,
        sha: [u8; 32],
        manager: &mut Manager,
        expected: Revisions,
    ) -> Result<()> {
        self.check(manager, expected)?;
        let (_, r) = self.selected(id, sha)?;
        let r = r.clone();
        self.revoke_base(manager, &r, expected)?;
        let mut next = self.state.clone();
        next.selections.remove(id);
        self.publish(next)
    }
    pub fn page(&self, after: Option<&str>, limit: u16, expected_revision: u64) -> Result<Page> {
        self.ready()?;
        if expected_revision != self.revision() {
            return Err(Error::Conflict);
        }
        if limit == 0 || limit > 16 {
            return Err(Error::Limit);
        }
        let mut entries = vec![];
        let mut next = None;
        if after.is_some_and(|s| {
            s.len() != 64
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }) {
            return Err(Error::Invalid);
        }
        for (sha, r) in &self.state.installed {
            let cursor = hex(sha);
            if after.is_some_and(|s| cursor.as_str() <= s) {
                continue;
            }
            if entries.len() == usize::from(limit) {
                next = entries
                    .last()
                    .map(|e: &Entry| hex(&e.review.wrapper_sha256));
                break;
            }
            let p = self.observe_storage(self.storage.load(*sha))?;
            if record(&review(&p)) != record(r) {
                self.poison();
                return Err(Error::Contract);
            }
            let s = self.state.selections.get(&r.id).filter(|s| s.sha == *sha);
            entries.push(Entry {
                review: r.clone(),
                selected: s.is_some(),
                enabled: s.is_some_and(|s| s.enabled),
                approval: s.and_then(|s| s.approval.clone()),
            });
        }
        Ok(Page {
            revision: self.revision(),
            entries,
            next,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn connect(
        &mut self,
        id: &str,
        sha: [u8; 32],
        manager: &mut Manager,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        expected: Revisions,
        expires: u64,
        now: u64,
    ) -> Result<CatalogManagedPackage> {
        self.check(manager, expected)?;
        let (s, r) = self.selected(id, sha)?;
        if !s.enabled {
            return Err(Error::Denied);
        }
        let a = s.approval.clone().ok_or(Error::Denied)?;
        self.current_base(manager, r)?;
        self.instances
            .retain(|w| w.upgrade().is_some_and(|l| l.live()));
        if self.instances.len() >= 128 {
            return Err(Error::Limit);
        }
        let p = self.observe_storage(self.storage.load(sha))?;
        let bridge = ManagedPreparedPackage::connect(
            p,
            manager,
            runtime,
            host,
            expected.manager,
            sha,
            a.session,
            a.process,
            a.sessions,
            expires,
            now,
        )?;
        let core = match runtime.revocation(&bridge.shared_connection()) {
            Ok(core) => core,
            Err(_) => {
                bridge.request_stop();
                let _ = bridge.instance().close(runtime);
                return Err(Error::Denied);
            }
        };
        let lease = Arc::new(InstanceLease {
            catalog: Arc::downgrade(&self.life),
            epoch: self.life.epoch.load(Ordering::Acquire),
            core,
            cancel: bridge.instance().cancellation(),
            active: AtomicBool::new(true),
        });
        self.instances.push(Arc::downgrade(&lease));
        Ok(CatalogManagedPackage {
            bridge: RefCell::new(bridge),
            lease,
            native_opened: AtomicBool::new(false),
            native_preparation_debt: RefCell::new(None),
        })
    }
    /// Reusable R2 guest profile, separately approved under complete wrapper ceilings.
    #[allow(clippy::too_many_arguments)]
    pub fn connect_session_exec(&mut self, id: &str, sha: [u8; 32],
        manager: &mut Manager, runtime: &mut HostRuntime, host: &SessionExecHost,
        expected: Revisions, expires: u64, now: u64) -> Result<CatalogManagedSessionExecPackage> {
        self.check(manager, expected)?;
        let (selection, review) = self.selected(id, sha)?;
        if !selection.enabled { return Err(Error::Denied); }
        let approval = selection.approval.clone().ok_or(Error::Denied)?;
        self.current_base(manager, review)?;
        self.instances.retain(|w| w.upgrade().is_some_and(|l| l.live()));
        if self.instances.len() >= 128 { return Err(Error::Limit); }
        let package = self.observe_storage(self.storage.load(sha))?;
        let bridge = ManagedPreparedSessionExecPackage::connect(package, manager, runtime,
            host, expected.manager, sha, approval.session, approval.sessions, expires, now)?;
        let core = match runtime.revocation(&bridge.shared_connection()) {
            Ok(core) => core,
            Err(_) => { bridge.request_stop(); let _ = bridge.close(runtime, host);
                return Err(Error::Denied); }
        };
        let lease = Arc::new(InstanceLease { catalog: Arc::downgrade(&self.life),
            epoch: self.life.epoch.load(Ordering::Acquire), core,
            cancel: bridge.instance().cancellation(), active: AtomicBool::new(true) });
        self.instances.push(Arc::downgrade(&lease));
        Ok(CatalogManagedSessionExecPackage { bridge, lease })
    }
}
impl Drop for Catalog {
    fn drop(&mut self) {
        self.life.active.store(false, Ordering::Release);
        self.stop_all();
    }
}
pub struct CatalogManagedPackage {
    bridge: RefCell<ManagedPreparedPackage>,
    lease: Arc<InstanceLease>,
    native_opened: AtomicBool,
    native_preparation_debt: RefCell<Option<crate::native_session::NativeSessionLease>>,
}
impl CatalogManagedPackage {
    /// Native session-only transport on this already approved original connection.
    /// It grants no process, proposal, execute or archive authority.
    /// Pure validation of an already approved session-only package for original-owner attachment.
    /// This does not open a factory, mint an admission, or change its one-shot latch.
    pub fn validate_native_session_package(&self, manager: &Manager, runtime: &HostRuntime,
        host: &SessionExecHost, now: u64) -> Result<()> {
        self.check()?;
        self.bridge.borrow().validate_native_session(manager, runtime, host, now)?;
        self.check()
    }
    pub fn open_native_session(&self, manager: &Manager, runtime: &HostRuntime,
        host: Arc<SessionExecHost>, now: u64) -> Result<crate::native_session::NativeSessionLease> {
        self.check()?;
        self.bridge.borrow().validate_native_session(manager, runtime, &host, now)?;
        if self.native_opened.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            return Err(Error::Denied);
        }
        let lease = self.lease.clone();
        let (prepared, result) = self.bridge.borrow().open_native_session(manager, runtime, host, now,
            Arc::new(move || lease.live()))?;
        if let Err(error) = result {
            *self.native_preparation_debt.borrow_mut() = Some(prepared);
            return Err(error);
        }
        Ok(prepared)
    }
    fn close_native_preparation(&self) -> Result<()> {
        let mut debt = self.native_preparation_debt.borrow_mut();
        if let Some(lease) = debt.as_ref() { lease.close()?; }
        debt.take();
        Ok(())
    }
    pub fn native_session_writer(&self, manager: &Manager, runtime: &HostRuntime,
        lease: &mut crate::native_session::NativeSessionLease, host: &SessionExecHost,
        session_id: &str, now: u64) -> Result<crate::native_session::NativeEndpointAuthority> {
        self.check()?;
        let bridge = self.bridge.borrow();
        bridge.validate_native_session(manager, runtime, host, now)?;
        if !lease.belongs_to(&bridge.shared_connection()) { return Err(Error::Denied); }
        let authority = lease.admit_writer(runtime, session_id, now)?;
        bridge.validate_native_session(manager, runtime, host, now)?;
        self.check()?;
        Ok(authority)
    }
    pub fn exchange_native_session(&self, manager: &Manager, runtime: &mut HostRuntime,
        lease: &crate::native_session::NativeSessionLease, host: &SessionExecHost,
        endpoint: crate::native_session::NativeEndpointId, bytes: &[u8],
        mut clock: impl FnMut() -> u64) -> Result<Vec<u8>> {
        self.check()?;
        let bridge = self.bridge.borrow();
        bridge.validate_native_session(manager, runtime, host, clock())?;
        if !lease.belongs_to(&bridge.shared_connection()) { return Err(Error::Denied); }
        let mut raw = lease.exchange(runtime, endpoint, bytes, &mut clock)?;
        if bridge.validate_native_session(manager, runtime, host, clock()).is_err()
            || self.check().is_err() {
            let mutation = Request::decode(bytes).is_ok_and(|r|
                !matches!(r.action(), Action::List | Action::Snapshot { .. }));
            if mutation { lease.mark_unknown(); }
            raw.fill(0);
            return Err(if mutation { Error::CommitUnknown } else { Error::Denied });
        }
        Ok(raw)
    }
    pub fn request_stop(&self) {
        self.lease.stop();
    }
    /// Only the original lease's revocation and cancellation cross threads.
    /// The RefCell bridge and original manager/Core remain exclusively owned.
    pub fn stop_handle(&self) -> Arc<dyn Fn() + Send + Sync> {
        let lease = self.lease.clone();
        Arc::new(move || lease.stop())
    }
    pub fn shared_connection(&self) -> Arc<Connection> {
        self.bridge.borrow().shared_connection()
    }
    pub fn generation(&self) -> u64 {
        self.bridge.borrow().generation()
    }
    pub fn limits(&self) -> Limits {
        self.bridge.borrow().limits()
    }
    pub fn package(&self) -> std::cell::Ref<'_, AgentProcessPackage> {
        std::cell::Ref::map(self.bridge.borrow(), |bridge| bridge.package())
    }
    pub fn close_session(&self, runtime: &mut HostRuntime, host: &SessionExecHost) -> Result<()> {
        self.request_stop();
        let native = self.close_native_preparation();
        let managed = self.bridge.borrow().close_session(runtime, host);
        native.and(managed)
    }
    pub fn session_capabilities(&self) -> SessionCapabilities {
        self.bridge.borrow().session_capabilities()
    }
    pub fn revocation(&self, runtime: &HostRuntime) -> Result<Revocation> {
        self.bridge.borrow().revocation(runtime)
    }
    pub fn validate_tool_observation(
        &self,
        manager: &Manager,
        runtime: &HostRuntime,
        host: &SessionExecHost,
        operation: &str,
        proposal_sha256: [u8; 32],
        now: u64,
    ) -> Result<morrow_agent_session_exec_v1_r2::safe_exec::ToolObservation> {
        self.check()?;
        let observation = self.bridge.borrow().validate_tool_observation(
            manager,
            runtime,
            host,
            operation,
            proposal_sha256,
            now,
        )?;
        self.check()?;
        Ok(observation)
    }
    /// R2-only dispatch over the combined import. It never binds a ProcessHost.
    pub fn run_session(
        &self,
        manager: &Manager,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> Result<TaskRun> {
        self.check()?;
        let run = self
            .bridge
            .borrow()
            .run_session(manager, runtime, host, input, || {
                if !self.lease.live() {
                    self.lease.stop();
                }
                let now = clock();
                if !self.lease.live() {
                    self.lease.stop();
                }
                now
            })?;
        Ok(self.finish_session_run(run))
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_session_owned<O: morrow_plugin_runtime::io_jobs::ManagedHostOwner>(
        &self,
        owner: &mut O,
        host: &SessionExecHost,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> Result<TaskRun> {
        self.check()?;
        let run = self
            .bridge
            .borrow()
            .run_session_owned(owner, host, input, || {
                if !self.lease.live() {
                    self.lease.stop();
                }
                let now = clock();
                if !self.lease.live() {
                    self.lease.stop();
                }
                now
            })?;
        Ok(self.finish_session_run(run))
    }
    fn finish_session_run(&self, mut run: TaskRun) -> TaskRun {
        if self.check().is_err() {
            if let Some(bytes) = &mut run.completion {
                bytes.fill(0);
            }
            run.completion = None;
            if run.report.outcome.is_ok() {
                run.report.outcome = Err(morrow_plugin_runtime::Fault::InactiveConnection);
            }
        }
        run
    }

    fn check(&self) -> Result<()> {
        if self.lease.live() {
            Ok(())
        } else {
            self.lease.stop();
            Err(Error::Denied)
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
        manager: &Manager,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        processes: &mut ProcessHost,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> Result<TaskRun> {
        self.check()?;
        let result = self
            .bridge
            .borrow()
            .run(manager, runtime, host, processes, input, || {
                if !self.lease.live() {
                    self.lease.stop();
                }
                let now = clock();
                if !self.lease.live() {
                    self.lease.stop();
                }
                now
            });
        match result {
            Ok(mut run) => {
                if self.check().is_err() {
                    run.completion = None;
                    if run.report.outcome.is_ok() {
                        run.report.outcome = Err(morrow_plugin_runtime::Fault::InactiveConnection);
                    }
                }
                Ok(run)
            }
            Err(error) => Err(error),
        }
    }
    /// Maintains the same protected owner before every imported frame.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    pub fn run_owned<O: morrow_plugin_runtime::io_jobs::ManagedHostOwner>(
        &self,
        owner: &mut O,
        host: &SessionExecHost,
        processes: &mut ProcessHost,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> Result<TaskRun> {
        self.check()?;
        let result = self
            .bridge
            .borrow()
            .run_owned(owner, host, processes, input, || {
                if !self.lease.live() {
                    self.lease.stop();
                }
                let now = clock();
                if !self.lease.live() {
                    self.lease.stop();
                }
                now
            });
        match result {
            Ok(mut run) => {
                if self.check().is_err() {
                    if let Some(bytes) = &mut run.completion {
                        bytes.fill(0);
                    }
                    run.completion = None;
                    if run.report.outcome.is_ok() {
                        run.report.outcome = Err(morrow_plugin_runtime::Fault::InactiveConnection);
                    }
                }
                Ok(run)
            }
            Err(error) => Err(error),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn register_process(
        &mut self,
        manager: &Manager,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        processes: &mut ProcessHost,
        identity: ToolIdentity,
        executor_connection: Arc<Connection>,
        executor_admission: &Admission,
        capabilities: ProcessCapabilities,
        budget: Budget,
        provider: Box<dyn ProcessProvider>,
        mut clock: impl FnMut() -> u64,
    ) -> Result<Handle> {
        self.check()?;
        let handle = self.bridge.borrow_mut().register_process(
            manager,
            runtime,
            host,
            processes,
            identity,
            executor_connection,
            executor_admission,
            capabilities,
            budget,
            provider,
            || {
                if !self.lease.live() {
                    self.lease.stop();
                }
                let now = clock();
                if !self.lease.live() {
                    self.lease.stop();
                }
                now
            },
        )?;
        if self.check().is_err() {
            let _ = processes.trusted_cleanup(handle);
            return Err(Error::Denied);
        }
        Ok(handle)
    }
    pub fn owned_handles(&self, runtime: &HostRuntime) -> Result<Vec<Handle>> {
        self.bridge.borrow().owned_handles(runtime)
    }
    pub fn close(
        &self,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        processes: &mut ProcessHost,
    ) -> Result<()> {
        self.request_stop();
        let native = self.close_native_preparation();
        let managed = self.bridge.borrow().close(runtime, host, processes);
        native.and(managed)
    }
}
impl Drop for CatalogManagedPackage {
    fn drop(&mut self) {
        self.request_stop();
    }
}
/// Original R2 managed authority; persistable ceilings do not restore this lease.
pub struct CatalogManagedSessionExecPackage {
    bridge: ManagedPreparedSessionExecPackage,
    lease: Arc<InstanceLease>,
}
impl CatalogManagedSessionExecPackage {
    pub fn request_stop(&self) { self.lease.stop(); self.bridge.request_stop(); }
    pub fn stop_handle(&self) -> Arc<dyn Fn() + Send + Sync> {
        let lease = self.lease.clone(); Arc::new(move || lease.stop())
    }
    pub fn shared_connection(&self) -> Arc<Connection> { self.bridge.shared_connection() }
    pub fn generation(&self) -> u64 { self.bridge.generation() }
    pub fn limits(&self) -> Limits { self.bridge.limits() }
    pub fn package(&self) -> &AgentProcessPackage { self.bridge.package() }
    pub fn session_capabilities(&self) -> SessionCapabilities { self.bridge.session_capabilities() }
    pub fn revocation(&self, runtime: &HostRuntime) -> Result<Revocation> { self.bridge.revocation(runtime) }
    pub fn validate_tool_observation(&self, manager: &Manager, runtime: &HostRuntime,
        host: &SessionExecHost, operation: &str, proposal_sha256: [u8; 32], now: u64)
        -> Result<morrow_agent_session_exec_v1_r2::safe_exec::ToolObservation> {
        self.check()?;
        let observation = self.bridge.validate_tool_observation(manager, runtime, host,
            operation, proposal_sha256, now)?;
        self.check()?;
        Ok(observation)
    }
    fn check(&self) -> Result<()> {
        if self.lease.live() { Ok(()) }
        else { self.request_stop(); Err(Error::Denied) }
    }
    fn finish(&self, mut run: TaskRun) -> TaskRun {
        if self.check().is_err() {
            if let Some(bytes) = &mut run.completion { bytes.fill(0); }
            run.completion = None;
            if run.report.outcome.is_ok() {
                run.report.outcome = Err(morrow_plugin_runtime::Fault::InactiveConnection);
            }
        }
        run
    }
    pub fn run(&self, manager: &Manager, runtime: &mut HostRuntime,
        host: &SessionExecHost, input: &[u8], mut clock: impl FnMut() -> u64) -> Result<TaskRun> {
        self.check()?;
        let run = self.bridge.run(manager, runtime, host, input, || {
            if !self.lease.live() { self.lease.stop(); }
            let now = clock();
            if !self.lease.live() { self.lease.stop(); }
            now
        })?;
        Ok(self.finish(run))
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_owned<O: morrow_plugin_runtime::io_jobs::ManagedHostOwner>(&self,
        owner: &mut O, host: &SessionExecHost, input: &[u8], mut clock: impl FnMut() -> u64)
        -> Result<TaskRun> {
        self.check()?;
        let run = self.bridge.run_owned(owner, host, input, || {
            if !self.lease.live() { self.lease.stop(); }
            let now = clock();
            if !self.lease.live() { self.lease.stop(); }
            now
        })?;
        Ok(self.finish(run))
    }
    pub fn close(&self, runtime: &mut HostRuntime, host: &SessionExecHost) -> Result<()> {
        self.request_stop(); self.bridge.close(runtime, host)
    }
}
impl Drop for CatalogManagedSessionExecPackage {
    fn drop(&mut self) { self.request_stop(); }
}
fn hex(sha: &[u8; 32]) -> String {
    sha.iter().map(|b| format!("{b:02x}")).collect()
}
