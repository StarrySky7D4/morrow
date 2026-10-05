//! Windows trusted-host selection of existing files and directory-bound create targets.
//! Retains the opened object, never reopens a guest path. Selection is not
//! dispatch authority or durable identity. Deletion uses a separate persistent
//! one-way claim before native effects; conditional replacement remains unsupported.
use crate::{
    io_binding::{self, IoBinding, IoResourceLease},
    manager::{ManagedInstance, Manager},
};
use morrow_core::{
    dispatch::HostRuntime,
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    file_path::RelativeFilePath,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{File, Metadata, OpenOptions},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, Prefix},
    sync::atomic::{AtomicU64, Ordering},
};
const MAX_TARGETS: usize = 128;
static NEXT_BROKER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Admission(io_binding::Error),
    /// The durable transaction succeeded, but live result delivery was revoked.
    /// Read original history/receipt; this is not a rollback or resend permission.
    CommittedButDeliveryDenied(io_binding::Error),
    /// A previous native close has uncertain ownership; restart the process.
    RestartRequired,
    /// Another native effect owns the process gate; no dispatch was claimed.
    Busy,
    /// Dispatch was claimed; no automatic replay is allowed.
    OutcomeUnknown,
    /// The operation already crossed its persistent dispatch boundary.
    AlreadyDispatched,
    /// This backend cannot atomically bind replacement to the expected target.
    /// No dispatch was claimed and no file effect was attempted. Do not retry
    /// with weaker sharing, a pathname-only overwrite, or an in-place truncate.
    UnsupportedConditionalReplacement,
    /// This call stopped before its durable dispatch/commit boundary. Existing
    /// preparation may remain; this does not cancel unrelated operations.
    CancelledBeforeDispatch,
    /// A durable preparation/staging/observation committed, but cancellation
    /// suppressed delivery. Inspect original history; do not infer rollback.
    CommittedButDeliveryCancelled,
    InvalidSelection,
    Missing,
    Mismatch,
    Changed,
    Io(std::io::ErrorKind),
    Limit,
    Persistence(morrow_core::Error),
}
impl From<io_binding::Error> for Error {
    fn from(value: io_binding::Error) -> Self {
        Self::Admission(value)
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.kind())
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "selected mutation target: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<morrow_core::Error> for Error {
    fn from(value: morrow_core::Error) -> Self {
        Self::Persistence(value)
    }
}
type Result<T> = std::result::Result<T, Error>;
mod control;
pub use control::TargetControl;
use control::{LocalControl, controlled, delivery, running};
mod creation;
mod creation_selection;
mod deletion;
mod preparation;
mod replacement;
pub use creation_selection::SelectedCreateTarget;
// Native File ownership boundary; all other target code retains deny(unsafe_code).
#[allow(unsafe_code)]
mod native_windows;

/// Internal trusted-host readonly directory open, relative to a retained File.
/// `listing` chooses fixed LIST rights, never an approval. Root/all ancestors,
/// original FileList authority, limits and same-handle checks remain caller duties.
pub(crate) fn open_relative_directory(
    parent: &File,
    component: &[u16],
    listing: bool,
) -> std::io::Result<File> {
    native_windows::open_relative_directory(parent, component, listing)
}

/// Supplied by the trusted selection/approval flow, never deserialized from a
/// guest. The digest correlates that approval; the original live lease enforces it.
#[derive(Clone, Debug)]
pub struct SelectionScope {
    pub subject: String,
    pub approval_sha256: [u8; 32],
    pub disposition: Disposition,
}
/// Valid only while this broker retains the original selection. Neither field
/// can restore authority after restart, reselection, release or revocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedTarget {
    pub reference: [u8; 32],
    pub expected_identity: [u8; 32],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    created: u64,
    modified: u64,
    length: u64,
    attributes: u32,
}
impl Stamp {
    fn read(metadata: Metadata) -> Result<Self> {
        // FILE_ATTRIBUTE_REPARSE_POINT; do not follow a final symlink/junction.
        if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
            return Err(Error::InvalidSelection);
        }
        Ok(Self {
            created: metadata.creation_time(),
            modified: metadata.last_write_time(),
            length: metadata.file_size(),
            attributes: metadata.file_attributes(),
        })
    }
    fn identity(self, reference: [u8; 32]) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"morrow.selected-mutation-object.v1\0");
        hash.update(reference);
        hash.update(self.created.to_le_bytes());
        hash.update(self.modified.to_le_bytes());
        hash.update(self.length.to_le_bytes());
        hash.update(self.attributes.to_le_bytes());
        hash.finalize().into()
    }
}
struct Entry {
    file: File,
    lease: IoResourceLease,
    scope: SelectionScope,
    package_sha256: [u8; 32],
    stamp: Stamp,
    selected: SelectedTarget,
}
struct Spent {
    binding: IoBinding,
    request_sha256: [u8; 32],
    claimed: bool,
}
/// Original-owner target handles and bounded spent-selection tombstones. Idle
/// maintenance must call `reap` to release expired/revoked handles and tombstones.
/// Native close uncertainty blocks new selections until process restart.
pub struct TargetBroker {
    secret: [u8; 32],
    identity: u64,
    next: u64,
    entries: BTreeMap<[u8; 32], Entry>,
    spent: BTreeMap<[u8; 32], Spent>,
    creates: BTreeMap<[u8; 32], creation_selection::CreateEntry>,
}
impl TargetBroker {
    /// Fresh unpredictable host secret per process/session; never accept a guest seed.
    pub fn new(secret: [u8; 32]) -> Result<Self> {
        if secret == [0; 32] {
            return Err(Error::InvalidSelection);
        }
        let identity = NEXT_BROKER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| Error::Limit)?;
        Ok(Self {
            secret,
            identity,
            next: 0,
            entries: BTreeMap::new(),
            spent: BTreeMap::new(),
            creates: BTreeMap::new(),
        })
    }
    pub fn len(&self) -> usize {
        self.entries.len() + self.creates.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.creates.is_empty()
    }
    pub fn reap(&mut self, now: u64) {
        self.entries
            .retain(|_, entry| !entry.lease.reclaimable(now));
        self.creates.retain(|_, entry| !entry.reclaimable(now));
        self.spent
            .retain(|_, spent| !spent.binding.reclaimable(now));
    }
    fn retained_slots(&self) -> usize {
        self.entries.len()
            + self.spent.len()
            + self
                .creates
                .values()
                .map(|entry| entry.handles.len())
                .sum::<usize>()
    }
    fn target_lease(&self, request: &RequestRecord) -> Result<&IoResourceLease> {
        let value = request.request();
        if value.disposition == Disposition::Create {
            self.creates
                .get(&value.target.reference)
                .map(|entry| entry.lease())
        } else {
            self.entries
                .get(&value.target.reference)
                .map(|entry| &entry.lease)
        }
        .ok_or(Error::Missing)
    }
    /// Build a canonical draft from the live original selection. The caller
    /// supplies only operation identity and content metadata; target, approval,
    /// package and disposition remain broker-owned. No Store or OS effect runs.
    #[allow(clippy::too_many_arguments)]
    pub fn build_request_controlled(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        reference: [u8; 32],
        operation_id: String,
        content_length: u64,
        content_sha256: Option<[u8; 32]>,
        control: &mut impl TargetControl,
    ) -> Result<RequestRecord> {
        let (scope, package_sha256, relative_path, expected_identity) =
            if let Some(entry) = self.creates.get(&reference) {
                control.with(|now, cancelled| {
                    entry.lease().preflight(manager, host, instance, now)?;
                    running(cancelled)
                })?;
                (
                    &entry.scope,
                    entry.package_sha256,
                    Some(entry.relative.clone()),
                    None,
                )
            } else {
                let entry = self.entries.get(&reference).ok_or(Error::Missing)?;
                control.with(|now, cancelled| {
                    entry.lease.preflight(manager, host, instance, now)?;
                    running(cancelled)
                })?;
                (
                    &entry.scope,
                    entry.package_sha256,
                    None,
                    Some(entry.selected.expected_identity),
                )
            };
        let request = RequestRecord::new(MutationRequest {
            operation_id,
            subject: scope.subject.clone(),
            package_sha256,
            approval_sha256: scope.approval_sha256,
            target: Target {
                reference,
                relative_path,
            },
            disposition: scope.disposition,
            expected_identity,
            content_length,
            content_sha256,
        })?;
        self.validate_request_controlled(manager, host, instance, &request, control)?;
        Ok(request)
    }
    /// Open a host-selected absolute disk path without modifying any bytes.
    /// Windows exclusive sharing prevents ordinary concurrent read/write/delete
    /// opens while retained. A final reparse point is opened itself and rejected.
    /// Parent resolution is the trusted picker boundary, NOT directory traversal
    /// authority: no guest-relative path is accepted by this broker.
    ///
    /// Metadata identity is selection-local, not a stable filesystem file ID or
    /// content digest. Actual effects still require a conditional platform backend
    /// and a committed dispatch claim; this API does neither.
    #[allow(clippy::too_many_arguments)]
    pub fn select_existing(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        path: &Path,
        scope: SelectionScope,
        clock: impl FnMut() -> u64,
    ) -> Result<SelectedTarget> {
        self.select_existing_controlled(
            manager,
            host,
            instance,
            binding,
            path,
            scope,
            &mut LocalControl(clock),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn select_existing_controlled(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        path: &Path,
        scope: SelectionScope,
        control: &mut impl TargetControl,
    ) -> Result<SelectedTarget> {
        if binding.is_mutation_history() {
            return Err(Error::Admission(io_binding::Error::Denied));
        }
        let capability = scope.disposition.capability();
        control.with(|now, cancelled| {
            binding.preflight_capability(manager, host, instance, capability, now)?;
            running(cancelled)
        })?;
        if !native_windows::available() {
            return Err(Error::RestartRequired);
        }
        if scope.disposition == Disposition::Create
            || scope.subject.is_empty()
            || scope.subject.len() > 256
            || scope
                .subject
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
            || scope.approval_sha256 == [0; 32]
        {
            return Err(Error::InvalidSelection);
        }
        validate_path(path)?;
        let now = controlled(control, Ok)?;
        self.reap(now);
        if self.retained_slots() >= MAX_TARGETS {
            return Err(Error::Limit);
        }
        let next = self.next.checked_add(1).ok_or(Error::Limit)?;
        let mut hash = Sha256::new();
        hash.update(b"morrow.selected-mutation-target.v1\0");
        hash.update(self.secret);
        hash.update(self.identity.to_le_bytes());
        hash.update(next.to_le_bytes());
        let reference = hash.finalize().into();
        let lease = controlled(control, |now| {
            Ok(binding.admit_resource(manager, host, instance, &[capability], now)?)
        })?;
        controlled(
            control,
            |now| Ok(lease.check(manager, host, instance, now)?),
        )?;
        // GENERIC_READ | DELETE, plus GENERIC_WRITE for the replacement selection.
        // No CREATE/TRUNCATE/DELETE_ON_CLOSE: opening must never perform an effect.
        let access = 0x8000_0000
            | 0x0001_0000
            | if scope.disposition == Disposition::Replace {
                0x4000_0000
            } else {
                0
            };
        let file = OpenOptions::new()
            .access_mode(access)
            .share_mode(0)
            .custom_flags(0x0020_0000)
            .open(path)?; // FILE_FLAG_OPEN_REPARSE_POINT
        let stamp = Stamp::read(file.metadata()?)?;
        controlled(
            control,
            |now| Ok(lease.check(manager, host, instance, now)?),
        )?;
        let selected = SelectedTarget {
            reference,
            expected_identity: stamp.identity(reference),
        };
        self.entries.insert(
            reference,
            Entry {
                file,
                lease,
                scope,
                package_sha256: instance.package().package().digest(),
                stamp,
                selected,
            },
        );
        self.next = next;
        Ok(selected)
    }
    /// Check exact immutable plan against the retained selection and current
    /// original owner. No path is read from the plan and no dispatch permit is
    /// returned. A future backend must recheck immediately at its effect boundary.
    pub fn validate_request(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        clock: impl FnMut() -> u64,
    ) -> Result<()> {
        self.validate_request_controlled(manager, host, instance, request, &mut LocalControl(clock))
    }
    pub fn validate_request_controlled(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        control: &mut impl TargetControl,
    ) -> Result<()> {
        if self
            .creates
            .contains_key(&request.request().target.reference)
        {
            return self.validate_create_controlled(manager, host, instance, request, control);
        }
        let request = request.request();
        let entry = self
            .entries
            .get(&request.target.reference)
            .ok_or(Error::Missing)?;
        control.with(|now, cancelled| {
            entry.lease.preflight(manager, host, instance, now)?;
            running(cancelled)
        })?;
        if request.subject != entry.scope.subject
            || request.approval_sha256 != entry.scope.approval_sha256
            || request.package_sha256 != entry.package_sha256
            || request.disposition != entry.scope.disposition
            || request.target.relative_path.is_some()
            || request.expected_identity != Some(entry.selected.expected_identity)
        {
            return Err(Error::Mismatch);
        }
        controlled(control, |now| {
            Ok(entry.lease.check(manager, host, instance, now)?)
        })?;
        if Stamp::read(entry.file.metadata()?)? != entry.stamp {
            return Err(Error::Changed);
        }
        controlled(control, |now| {
            Ok(entry.lease.check(manager, host, instance, now)?)
        })?;
        Ok(())
    }
    /// Explicit close by the original live owner. Expired/stopped selections are
    /// reclaimed by host maintenance or broker Drop, never by a foreign caller.
    pub fn release(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        reference: [u8; 32],
        now: u64,
    ) -> Result<()> {
        self.release_controlled(
            manager,
            host,
            instance,
            reference,
            &mut LocalControl(|| now),
        )
    }
    pub fn release_controlled(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        reference: [u8; 32],
        control: &mut impl TargetControl,
    ) -> Result<()> {
        if let Some(entry) = self.creates.get(&reference) {
            control.with(|now, cancelled| {
                entry.lease().check(manager, host, instance, now)?;
                running(cancelled)
            })?;
            self.creates.remove(&reference);
        } else {
            let entry = self.entries.get(&reference).ok_or(Error::Missing)?;
            control.with(|now, cancelled| {
                entry.lease.check(manager, host, instance, now)?;
                running(cancelled)
            })?;
            self.entries.remove(&reference);
        }
        Ok(())
    }
}
fn validate_path(path: &Path) -> Result<()> {
    // Reject device namespaces, UNC/remote spellings and alternate data streams.
    // A drive may still be backed by a redirector: this is not a local-volume proof.
    let mut parts = path.components();
    if !matches!(parts.next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
        || !matches!(parts.next(), Some(Component::RootDir))
    {
        return Err(Error::InvalidSelection);
    }
    // components() normalizes interior dots, so validate the ORIGINAL spelling.
    let raw = path.to_str().ok_or(Error::InvalidSelection)?;
    if raw.encode_utf16().count() > 32_760 {
        return Err(Error::InvalidSelection);
    }
    let prefix_len = if raw.starts_with(r"\\?\") { 7 } else { 3 };
    let relative = raw.get(prefix_len..).ok_or(Error::InvalidSelection)?;
    for segment in relative.split(['\\', '/']) {
        RelativeFilePath::parse(segment).map_err(|_| Error::InvalidSelection)?;
    }
    Ok(())
}
