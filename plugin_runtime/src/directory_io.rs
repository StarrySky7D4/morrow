//! Immediate, observed listing of a trusted already-selected directory object.
//! This is not channel::Directory, a picker, atomic filesystem snapshot, path
//! grant, public guest import, recursive listing or read/mutation authority.
//! The original managed owner must call this synchronously off the UI thread;
//! io_jobs manages the owner queue; protected production selection remains separate.
use crate::{
    io_binding::{self, IoBinding, IoResourceLease},
    manager::{ManagedInstance, Manager},
};
use morrow_core::{dispatch::HostRuntime, plugin_package::io::IoCapability};
use morrow_fs_directory_v1::{DirectoryEntry, EntryKind, FsDirectoryPage, NameEncoding};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
};

// Only the private OS query module may use unsafe. No guest memory reaches it.
#[allow(unsafe_code)]
mod native_windows;

pub const MAX_ENTRIES: usize = 1024;
pub const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_SELECTIONS: usize = 8;
const ROOT_QUERY_BYTES: u64 = 32; // FILE_ATTRIBUTE_TAG_INFO + FILE_ID_INFO

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Admission(io_binding::Error),
    NotDirectory,
    ReparsePoint,
    UnsupportedNativeQuery,
    InvalidNativeRecord,
    SourceChanged,
    InvalidCursor,
    Cancelled,
    Limit,
    Allocation,
    Io(std::io::ErrorKind),
    Codec,
}
impl From<io_binding::Error> for Error {
    fn from(v: io_binding::Error) -> Self {
        Self::Admission(v)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "selected directory: {self:?}")
    }
}
impl std::error::Error for Error {}

/// Trusted original-clock adapter. Each step must sample and validate under the
/// same original clock guard. This supplies no approval or replacement clock.
/// Only admission, liveness and accounting belong inside a step; native queries,
/// parsing, encoding, cancellation callbacks and resource drops stay outside it.
/// A slow trusted clock closure can still delay callers sharing that clock.
pub trait DirectoryClock {
    fn with<T>(&mut self, step: impl FnOnce(u64) -> Result<T, Error>) -> Result<T, Error>;
}

struct LegacyClock<F>(F);
impl<F: FnMut() -> u64> DirectoryClock for LegacyClock<F> {
    fn with<T>(&mut self, step: impl FnOnce(u64) -> Result<T, Error>) -> Result<T, Error> {
        step((self.0)())
    }
}

/// Eligibility only, for trusted owner maintenance. Private fixed-capacity IDs
/// carry no roots, leases or allocation. Release on this same broker after the
/// original clock step returns, with exclusive mutable broker access retained.
pub(crate) struct ReclaimPlan {
    references: [Option<[u8; 32]>; MAX_SELECTIONS],
}

/// Trusted host ceilings, always subordinate to the original IoBinding budget.
#[derive(Clone, Copy, Debug)]
pub struct CaptureLimits {
    pub max_entries: usize,
    pub max_metadata_bytes: u64,
    pub max_job_bytes: u64,
}
impl Default for CaptureLimits {
    fn default() -> Self {
        Self {
            max_entries: MAX_ENTRIES,
            max_metadata_bytes: MAX_METADATA_BYTES,
            max_job_bytes: morrow_core::plugin_package::io::MAX_JOB_BYTES,
        }
    }
}
impl CaptureLimits {
    fn validate(self) -> Result<Self, Error> {
        if self.max_entries > MAX_ENTRIES
            || self.max_metadata_bytes > MAX_METADATA_BYTES
            || self.max_job_bytes == 0
            || self.max_job_bytes > morrow_core::plugin_package::io::MAX_JOB_BYTES
        {
            return Err(Error::Limit);
        }
        Ok(self)
    }
}

/// Opaque observation reference, not a grant to any entry or native path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedDirectory {
    pub reference: [u8; 32],
    pub selection_epoch: [u8; 32],
    pub entries: usize,
    pub metadata_bytes: u64,
}
/// Host API cursor only. No import or old Core IO request decoder is changed.
#[derive(Clone, Copy, Debug)]
pub struct PageRequest {
    pub reference: [u8; 32],
    pub selection_epoch: [u8; 32],
    pub page_sequence: u64,
    pub after_entry_id: Option<[u8; 32]>,
}
impl SelectedDirectory {
    pub fn first_page(self) -> PageRequest {
        PageRequest {
            reference: self.reference,
            selection_epoch: self.selection_epoch,
            page_sequence: 1,
            after_entry_id: None,
        }
    }
}

struct Observation {
    lease: IoResourceLease,
    root: File,
    identity: [u8; 24],
    epoch: [u8; 32],
    entries: Vec<DirectoryEntry>,
    metadata_bytes: u64,
    cursor: usize,
    sequence: u64,
    after_entry_id: Option<[u8; 32]>,
}
/// Secret is supplied/generated only by the trusted host per broker.
pub struct DirectoryBroker {
    secret: [u8; 32],
    sequence: u64,
    observations: BTreeMap<[u8; 32], Observation>,
}
impl DirectoryBroker {
    pub fn new(secret: [u8; 32]) -> Self {
        Self {
            secret,
            sequence: 0,
            observations: BTreeMap::new(),
        }
    }
    /// Diagnostic resident capacity, never proof of join or live authority.
    pub fn usage(&self) -> (usize, u64) {
        (
            self.observations.len(),
            self.observations.values().map(|o| o.metadata_bytes).sum(),
        )
    }
    /// Original owner maintenance is required while idle. Stop signals do not
    /// execute this function or imply that an owner/producer thread joined.
    pub fn reap(&mut self, now: u64) {
        let plan = self.reclaimable_at(now);
        self.release_reclaimed(plan);
    }
    pub(crate) fn reclaimable_at(&self, now: u64) -> ReclaimPlan {
        let mut plan = ReclaimPlan {
            references: [None; MAX_SELECTIONS],
        };
        let mut next = 0;
        for (reference, observation) in &self.observations {
            if observation.lease.reclaimable(now) {
                plan.references[next] = Some(*reference);
                next += 1;
            }
        }
        plan
    }
    pub(crate) fn release_reclaimed(&mut self, plan: ReclaimPlan) {
        for reference in plan.references.into_iter().flatten() {
            self.observations.remove(&reference);
        }
    }
    fn token(&self, domain: &[u8], next: u64, identity: &[u8; 24]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(domain);
        h.update(self.secret);
        h.update(next.to_le_bytes());
        h.update(identity);
        h.finalize().into()
    }

    /// Consumes an already-open, trusted selected directory File. Caller owns
    /// selection and exclusive enumeration cursor. Bare File proves no ancestor
    /// policy; picker/ancestor selection proofs must remain with the original
    /// trusted adapter. Broker never examines, reopens or resolves a path.
    /// Retains the root, checks its object identity/attributes around every batch
    /// and delivery, rejects root/child reparse and never follows child entries.
    /// Listing may observe concurrent edits; no atomic snapshot is promised.
    /// An OS synchronous query cannot be preempted by cooperative cancellation.
    #[allow(clippy::too_many_arguments)]
    pub fn grant_open_directory(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        root: File,
        limits: CaptureLimits,
        clock: impl FnMut() -> u64,
    ) -> Result<SelectedDirectory, Error> {
        self.grant_open_directory_with_cancel(
            manager,
            host,
            instance,
            binding,
            root,
            limits,
            clock,
            || false,
        )
    }

    /// Per-command cooperative veto only. A true result grants no authority and
    /// cannot replace the original approval, lease, budget or clock. Checks
    /// surround existing native boundary operations; synchronous OS queries and
    /// their bounded private parsing cannot be preempted by this callback.
    #[allow(clippy::too_many_arguments)]
    pub fn grant_open_directory_with_cancel(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        root: File,
        limits: CaptureLimits,
        clock: impl FnMut() -> u64,
        cancelled: impl FnMut() -> bool,
    ) -> Result<SelectedDirectory, Error> {
        self.grant_open_directory_with_clock_and_cancel(
            manager,
            host,
            instance,
            binding,
            root,
            limits,
            &mut LegacyClock(clock),
            cancelled,
        )
    }

    /// Atomic original-clock steps, without holding their guard across native
    /// work. The cancellation callback remains a veto at the existing boundaries.
    #[allow(clippy::too_many_arguments)]
    pub fn grant_open_directory_with_clock_and_cancel(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        root: File,
        limits: CaptureLimits,
        clock: &mut impl DirectoryClock,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<SelectedDirectory, Error> {
        let limits = limits.validate()?;
        let plan = clock.with(|now| {
            binding.preflight_capability(manager, host, instance, IoCapability::FileList, now)?;
            Ok(self.reclaimable_at(now))
        })?;
        self.release_reclaimed(plan);
        if self.observations.len() >= MAX_SELECTIONS {
            return Err(Error::Limit);
        }
        let next = self.sequence.checked_add(1).ok_or(Error::Limit)?;
        // Burn the serial even on failure: no failed observation may reuse an epoch.
        self.sequence = next;
        let lease = clock.with(|now| {
            binding
                .admit_resource(manager, host, instance, &[IoCapability::FileList], now)
                .map_err(Error::from)
        })?;
        let job = clock.with(|now| {
            binding
                .admit_job_authenticated(0, limits.max_job_bytes, now)
                .map_err(Error::from)
        })?;
        let check = |now| {
            lease
                .check(manager, host, instance, now)
                .map_err(Error::from)
        };
        clock.with(check)?;
        clock.with(|now| {
            job.charge(&[IoCapability::FileList], ROOT_QUERY_BYTES, now)
                .map_err(Error::from)
        })?;
        cancellation_veto(&mut cancelled)?;
        let identity = native_windows::identity(&root)?;
        cancellation_veto(&mut cancelled)?;
        clock.with(check)?;
        let epoch = self.token(b"morrow.fs-directory.epoch.v1", next, &identity);
        let reference = self.token(b"morrow.fs-directory.reference.v1", next, &identity);
        if epoch == [0; 32] || reference == [0; 32] || self.observations.contains_key(&reference) {
            return Err(Error::Limit);
        }
        // The aligned native query buffer is charged before allocation. Each
        // batch additionally reserves an upper bound for returned metadata/name
        // storage before native parsing allocates anything derived from it.
        clock.with(|now| {
            job.charge(
                &[IoCapability::FileList],
                native_windows::BUFFER_BYTES as u64,
                now,
            )
            .map_err(Error::from)
        })?;
        cancellation_veto(&mut cancelled)?;
        let mut buffer = native_windows::Buffer::new()?;
        let mut entries = Vec::new();
        let mut names = BTreeSet::new();
        let mut metadata_bytes = 0u64;
        let mut restart = true;
        loop {
            clock.with(check)?;
            clock.with(|now| {
                job.charge(&[IoCapability::FileList], ROOT_QUERY_BYTES, now)
                    .map_err(Error::from)
            })?;
            cancellation_veto(&mut cancelled)?;
            if native_windows::identity(&root)? != identity {
                return Err(Error::SourceChanged);
            }
            cancellation_veto(&mut cancelled)?;
            clock.with(check)?;
            clock.with(|now| {
                job.charge(
                    &[IoCapability::FileList],
                    (native_windows::BUFFER_BYTES * 2) as u64,
                    now,
                )
                .map_err(Error::from)
            })?;
            cancellation_veto(&mut cancelled)?;
            let batch = buffer.next(&root, restart)?;
            cancellation_veto(&mut cancelled)?;
            restart = false;
            clock.with(check)?;
            clock.with(|now| {
                job.charge(&[IoCapability::FileList], ROOT_QUERY_BYTES, now)
                    .map_err(Error::from)
            })?;
            cancellation_veto(&mut cancelled)?;
            if native_windows::identity(&root)? != identity {
                return Err(Error::SourceChanged);
            }
            cancellation_veto(&mut cancelled)?;
            clock.with(check)?;
            let Some(batch) = batch else {
                break;
            };
            for item in batch {
                clock.with(check)?;
                // Includes entry fields/container allowance and duplicate-name
                // guard copy. Charge before retaining either names or entries.
                let cost = 128u64
                    .checked_add(
                        (item.name.len() as u64)
                            .checked_mul(2)
                            .ok_or(Error::Limit)?,
                    )
                    .ok_or(Error::Limit)?;
                let total = metadata_bytes.checked_add(cost).ok_or(Error::Limit)?;
                if entries.len() >= limits.max_entries || total > limits.max_metadata_bytes {
                    return Err(Error::Limit);
                }
                clock.with(|now| {
                    job.charge(&[IoCapability::FileList], cost, now)
                        .map_err(Error::from)
                })?;
                cancellation_veto(&mut cancelled)?;
                if !names.insert(item.name.clone()) {
                    return Err(Error::SourceChanged);
                }
                entries.try_reserve(1).map_err(|_| Error::Allocation)?;
                let mut h = Sha256::new();
                h.update(b"morrow.fs-directory.entry.v1");
                h.update(epoch);
                h.update((entries.len() as u64).to_le_bytes());
                h.update(item.id);
                h.update(&item.name);
                let entry_id = h.finalize().into();
                let kind = if item.directory {
                    EntryKind::Directory
                } else if item.other {
                    EntryKind::Other
                } else {
                    EntryKind::File
                };
                entries.push(DirectoryEntry {
                    entry_id,
                    name: item.name,
                    encoding: NameEncoding::Utf16Le,
                    kind,
                    logical_length: (kind == EntryKind::File).then_some(item.length),
                });
                metadata_bytes = total;
            }
        }
        clock.with(check)?;
        clock.with(|now| {
            job.charge(&[IoCapability::FileList], ROOT_QUERY_BYTES, now)
                .map_err(Error::from)
        })?;
        cancellation_veto(&mut cancelled)?;
        if native_windows::identity(&root)? != identity {
            return Err(Error::SourceChanged);
        }
        cancellation_veto(&mut cancelled)?;
        clock.with(check)?;
        // No partial ref on any error; failed capture drops the File and original
        // resource/job leases, while cumulative admitted bytes remain spent.
        let selected = SelectedDirectory {
            reference,
            selection_epoch: epoch,
            entries: entries.len(),
            metadata_bytes,
        };
        cancellation_veto(&mut cancelled)?;
        self.observations.insert(
            reference,
            Observation {
                lease,
                root,
                identity,
                epoch,
                entries,
                metadata_bytes,
                cursor: 0,
                sequence: 1,
                after_entry_id: None,
            },
        );
        Ok(selected)
    }

    /// Sequential delivery under the original held approval. Entry IDs are only
    /// cursor consistency, never a handle or capability for child access.
    #[allow(clippy::too_many_arguments)]
    pub fn next_page(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        request: PageRequest,
        clock: impl FnMut() -> u64,
    ) -> Result<FsDirectoryPage, Error> {
        self.next_page_with_cancel(manager, host, instance, binding, request, clock, || false)
    }

    /// Per-command veto checked only after exact original owner and cursor
    /// validation. Foreign/invalid callers cannot retire another observation.
    /// Authorized cancellation drops its retained root/spool and capacity, with
    /// no cumulative-byte refund and no claim that any owner thread joined.
    #[allow(clippy::too_many_arguments)]
    pub fn next_page_with_cancel(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        request: PageRequest,
        clock: impl FnMut() -> u64,
        cancelled: impl FnMut() -> bool,
    ) -> Result<FsDirectoryPage, Error> {
        self.next_page_with_clock_and_cancel(
            manager,
            host,
            instance,
            binding,
            request,
            &mut LegacyClock(clock),
            cancelled,
        )
    }

    /// Exact owner/cursor validation and each original clock-dependent check are
    /// atomic steps. Encoding, native checks, vetoes and retirement run between them.
    #[allow(clippy::too_many_arguments)]
    pub fn next_page_with_clock_and_cancel(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        request: PageRequest,
        clock: &mut impl DirectoryClock,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<FsDirectoryPage, Error> {
        clock.with(|now| {
            binding
                .preflight_capability(manager, host, instance, IoCapability::FileList, now)
                .map_err(Error::from)
        })?;
        let observed = self
            .observations
            .get(&request.reference)
            .ok_or(Error::InvalidCursor)?;
        observed.lease.binding().validate_same_owner(binding)?;
        clock.with(|now| {
            observed
                .lease
                .preflight(manager, host, instance, now)
                .map_err(Error::from)
        })?;
        if request.selection_epoch != observed.epoch
            || request.page_sequence != observed.sequence
            || request.after_entry_id != observed.after_entry_id
        {
            return Err(Error::InvalidCursor);
        }
        let result = (|| {
            clock.with(|now| {
                observed
                    .lease
                    .check(manager, host, instance, now)
                    .map_err(Error::from)
            })?;
            // Fixed host cursor metadata (ref/epoch/sequence/presence/optional ID)
            // plus original response allocation ceiling, never a claimed import.
            let request_bytes = 73
                + if request.after_entry_id.is_some() {
                    32
                } else {
                    0
                };
            let job = clock.with(|now| {
                observed
                    .lease
                    .binding()
                    .admit(
                        manager,
                        host,
                        instance,
                        IoCapability::FileList,
                        0,
                        request_bytes + 65536 + ROOT_QUERY_BYTES * 2,
                        now,
                    )
                    .map_err(Error::from)
            })?;
            cancellation_veto(&mut cancelled)?;
            if native_windows::identity(&observed.root)? != observed.identity {
                return Err(Error::SourceChanged);
            }
            cancellation_veto(&mut cancelled)?;
            clock.with(|now| job.check(manager, host, instance, now).map_err(Error::from))?;
            let mut end = observed.cursor;
            let mut name_bytes = 0usize;
            while end < observed.entries.len() && end - observed.cursor < 32 {
                let next = name_bytes
                    .checked_add(observed.entries[end].name.len())
                    .ok_or(Error::Limit)?;
                if next > 16 * 1024 {
                    break;
                }
                name_bytes = next;
                end += 1;
            }
            if end == observed.cursor && end != observed.entries.len() {
                return Err(Error::Limit);
            }
            cancellation_veto(&mut cancelled)?;
            let page = FsDirectoryPage {
                selection_epoch: observed.epoch,
                page_sequence: observed.sequence,
                entries: observed.entries[observed.cursor..end].to_vec(),
                terminal: end == observed.entries.len(),
            };
            let wire = page.encode().map_err(|_| Error::Codec)?;
            cancellation_veto(&mut cancelled)?;
            if wire.len() > 65536 {
                return Err(Error::Limit);
            }
            // Every emitted page is independently codec validated; no native
            // pointer/ID is exposed beyond the opaque epoch-scoped digest.
            page.validate().map_err(|_| Error::Codec)?;
            clock.with(|now| job.check(manager, host, instance, now).map_err(Error::from))?;
            cancellation_veto(&mut cancelled)?;
            if native_windows::identity(&observed.root)? != observed.identity {
                return Err(Error::SourceChanged);
            }
            cancellation_veto(&mut cancelled)?;
            clock.with(|now| {
                observed
                    .lease
                    .check(manager, host, instance, now)
                    .map_err(Error::from)
            })?;
            cancellation_veto(&mut cancelled)?;
            Ok((page, end))
        })();
        match result {
            Ok((page, end)) => {
                if page.terminal {
                    self.observations.remove(&request.reference);
                } else {
                    let o = self.observations.get_mut(&request.reference).unwrap();
                    let next_sequence = o.sequence.checked_add(1).ok_or(Error::Limit)?;
                    o.cursor = end;
                    o.sequence = next_sequence;
                    o.after_entry_id = page.entries.last().map(|e| e.entry_id);
                }
                Ok(page)
            }
            Err(error) => {
                self.observations.remove(&request.reference);
                Err(error)
            }
        }
    }

    /// Cleanup authenticates the exact owner before removing another selection.
    /// Expired/Stopped owners are instead removed by trusted idle reap/Drop.
    /// This host cleanup performs no query, encoding or byte delivery and adds
    /// no byte reservation; it releases only capacity, never cumulative usage.
    pub fn cancel(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        selected: SelectedDirectory,
        now: u64,
    ) -> Result<(), Error> {
        self.validate_cancel(manager, host, instance, binding, selected, now)?;
        self.observations.remove(&selected.reference);
        Ok(())
    }

    /// Authenticate cleanup in one original-clock step, then drop the retained
    /// root/spool/lease after that guard returns. Adds no delivery or byte refund.
    #[allow(clippy::too_many_arguments)]
    pub fn cancel_with_clock(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        selected: SelectedDirectory,
        clock: &mut impl DirectoryClock,
    ) -> Result<(), Error> {
        clock.with(|now| self.validate_cancel(manager, host, instance, binding, selected, now))?;
        self.observations.remove(&selected.reference);
        Ok(())
    }

    fn validate_cancel(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        selected: SelectedDirectory,
        now: u64,
    ) -> Result<(), Error> {
        binding.preflight_capability(manager, host, instance, IoCapability::FileList, now)?;
        let o = self
            .observations
            .get(&selected.reference)
            .ok_or(Error::InvalidCursor)?;
        o.lease.binding().validate_same_owner(binding)?;
        o.lease.check(manager, host, instance, now)?;
        if selected.selection_epoch != o.epoch {
            return Err(Error::InvalidCursor);
        }
        Ok(())
    }
}

fn cancellation_veto(cancelled: &mut impl FnMut() -> bool) -> Result<(), Error> {
    if cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
