//! Exact original live upload approval. No path, serialized grant, retry or restored owner.
use crate::{
    Limits,
    blob_upload::Spec,
    manager::{Control, ManagedInstance, Manager},
    monotonic::Instant,
};
use morrow_core::{
    dispatch::{ConnectionBinding, HostBinding, HostRuntime},
    lifecycle::BlobUploadReceiverLiveness,
    plugin_package::proto::BlobUploadDeclaration,
    store::BlobStageStoreBinding,
};
use std::{
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};

pub const MAX_LIFETIME: Duration = Duration::from_secs(3600);
/// Additional host ceiling on cumulative Store work, independent of wire size.
/// A small Inspect request can verify the entire original object.
pub const MAX_NATIVE_WORK_BYTES: u64 = 128 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Denied,
    Expired,
    Limit,
    AlreadyBound,
}
pub type Result<T> = std::result::Result<T, Error>;

/// Cumulative instance ledger. It survives dropping the only issued approval.
pub(crate) struct UploadContext {
    claimed: AtomicBool,
    max_object_bytes: u64,
    max_calls: u32,
    max_wire_bytes: u64,
    calls: AtomicU32,
    wire_bytes: AtomicU64,
    native_work_bytes: AtomicU64,
}
impl UploadContext {
    pub(crate) fn new(d: &BlobUploadDeclaration, limits: Limits) -> Self {
        Self {
            claimed: AtomicBool::new(false),
            max_object_bytes: d.max_object_bytes,
            max_calls: d.max_calls.min(limits.host_calls),
            max_wire_bytes: d.max_wire_bytes,
            calls: AtomicU32::new(0),
            wire_bytes: AtomicU64::new(0),
            native_work_bytes: AtomicU64::new(0),
        }
    }
    fn charge_bytes(&self, bytes: u64) -> Result<()> {
        self.wire_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(bytes)
                    .filter(|next| *next <= self.max_wire_bytes)
            })
            .map(|_| ())
            .map_err(|_| Error::Limit)
    }
}
struct State {
    manager: Weak<()>,
    revision_lease: Arc<AtomicBool>,
    revision: u64,
    host: HostBinding,
    connection: ConnectionBinding,
    control: Weak<Control>,
    receiver: BlobUploadReceiverLiveness,
    store: BlobStageStoreBinding,
    package: [u8; 32],
    nomination: [u8; 32],
    spec: Spec,
    deadline: Instant,
    revoked: AtomicBool,
    context: Arc<UploadContext>,
}
impl State {
    // No Store, queue, clock mutex or callback is entered at a final commit guard.
    fn check(&self) -> Result<()> {
        if self.manager.upgrade().is_none()
            || self.revision_lease.load(Ordering::Acquire)
            || self.revoked.load(Ordering::Acquire)
            || self.receiver.check().is_err()
            || self.store.check_live().is_err()
            || self.control.upgrade().is_none_or(|c| !c.changes_active())
        {
            return Err(Error::Denied);
        }
        if Instant::now() >= self.deadline {
            return Err(Error::Expired);
        }
        Ok(())
    }
}
/// Trusted host chooses the exact operation and fixed lifetime. These values alone
/// are not permission; issue also requires the exact enabled package/Manager approval.
pub struct ApprovalRequest {
    pub expected_revision: u64,
    pub nomination_ref: [u8; 32],
    pub spec: Spec,
    pub lifetime: Duration,
}
/// Nonclone, nonserializable approval for exactly one original instance/Store/operation.
/// Its owner must be moved into the dedicated worker, never recreated from a receipt.
pub struct UploadApproval {
    state: Arc<State>,
}
/// Restriction only. Cloning it cannot execute, replenish, rebind or renew an upload.
#[derive(Clone)]
pub struct UploadRevocation {
    state: Arc<State>,
}
impl UploadRevocation {
    pub fn revoke(&self) {
        self.state.revoked.store(true, Ordering::Release);
    }
    /// Additional veto for historical execution metadata after intentional
    /// disconnect. False is not live receiver/Store authority or a new grant.
    pub(crate) fn completed_delivery_vetoed(&self) -> bool {
        self.state.revoked.load(Ordering::Acquire)
            || self.state.revision_lease.load(Ordering::Acquire)
            || self.state.manager.upgrade().is_none()
            || self.state.store.check_live().is_err()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Usage {
    pub calls: u32,
    pub wire_bytes: u64,
    pub native_work_bytes: u64,
}
impl UploadApproval {
    pub fn issue(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        request: ApprovalRequest,
    ) -> Result<Self> {
        manager
            .validate_instance(host, instance)
            .map_err(|_| Error::Denied)?;
        let package = instance.package().package();
        let declaration = package.blob_upload_declaration().ok_or(Error::Denied)?;
        let selection = manager
            .selection(&package.manifest().package_id)
            .ok_or(Error::Denied)?;
        if request.expected_revision != manager.revision()
            || !selection.enabled
            || !selection.approved_blob_upload
            || selection.digest != package.digest()
            || instance.connection().package_digest() != Some(package.digest())
            || request.nomination_ref == [0; 32]
        {
            return Err(Error::Denied);
        }
        request.spec.validate().map_err(|_| Error::Limit)?;
        if request.lifetime.is_zero() || request.lifetime > MAX_LIFETIME {
            return Err(Error::Limit);
        }
        let deadline = Instant::now()
            .checked_add(request.lifetime)
            .ok_or(Error::Limit)?;
        let context = instance
            .io_control()
            .blob_upload
            .as_ref()
            .ok_or(Error::Denied)?;
        if request.spec.byte_length > context.max_object_bytes
            || context.max_calls == 0
            || request.spec.limits.max_payload_bytes > context.max_object_bytes
            || request.spec.limits.max_wire_bytes > context.max_wire_bytes
            || request.spec.limits.max_requests > u64::from(declaration.max_calls)
            || request.spec.limits.max_receipts > u64::from(declaration.max_calls)
        {
            return Err(Error::Limit);
        }
        let state = Arc::new(State {
            manager: manager.identity(),
            revision_lease: manager.upload_revision_guard(),
            revision: request.expected_revision,
            host: host.binding(),
            connection: instance.connection().binding(),
            control: Arc::downgrade(instance.io_control()),
            receiver: host
                .blob_upload_receiver_liveness(instance.connection())
                .map_err(|_| Error::Denied)?,
            store: host.store_local().blob_stage_store_binding(),
            package: package.digest(),
            nomination: request.nomination_ref,
            spec: request.spec,
            deadline,
            revoked: AtomicBool::new(false),
            context: context.clone(),
        });
        state.check()?;
        context
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::AlreadyBound)?;
        // Revocation concurrent with issuance cannot restore the one-shot claim.
        state.check()?;
        Ok(Self { state })
    }
    pub fn revocation(&self) -> UploadRevocation {
        UploadRevocation {
            state: self.state.clone(),
        }
    }
    /// Atomic and monotonic checks only. Not proof of Store effects or worker return.
    pub fn check(&self) -> Result<()> {
        self.state.check()
    }
    /// Run before Store/queue transactions. An observed original-Store substitution
    /// permanently revokes this approval even if that old Store is later put back.
    pub fn validate_owner(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
    ) -> Result<()> {
        let s = &self.state;
        if !Weak::ptr_eq(&s.manager, &manager.identity())
            || s.revision != manager.revision()
            || s.host != host.binding()
            || s.connection != instance.connection().binding()
            || !Weak::ptr_eq(&s.control, &Arc::downgrade(instance.io_control()))
            || s.package != instance.package().package().digest()
            || instance.connection().package_digest() != Some(s.package)
        {
            return Err(Error::Denied);
        }
        manager
            .validate_instance(host, instance)
            .map_err(|_| Error::Denied)?;
        if host
            .store_local()
            .validate_blob_stage_store_binding(&s.store)
            .is_err()
        {
            s.revoked.store(true, Ordering::Release);
            return Err(Error::Denied);
        }
        s.check()
    }
    /// Bounded immutable metadata for the trusted dispatcher. None is a guest grant.
    pub fn spec(&self) -> &Spec {
        &self.state.spec
    }
    pub fn nomination_ref(&self) -> [u8; 32] {
        self.state.nomination
    }
    pub fn deadline(&self) -> Instant {
        self.state.deadline
    }
    pub fn usage(&self) -> Usage {
        Usage {
            calls: self.state.context.calls.load(Ordering::Acquire),
            wire_bytes: self.state.context.wire_bytes.load(Ordering::Acquire),
            native_work_bytes: self.state.context.native_work_bytes.load(Ordering::Acquire),
        }
    }
    /// Reserve one original bounded request before dispatch. Failed later reservations
    /// retain earlier call charges; no cancellation/drop path refunds this ledger.
    pub fn charge_request(&self, bytes: usize) -> Result<()> {
        self.check()?;
        if bytes == 0 || bytes > crate::MAX_BLOB_UPLOAD_REQUEST_BYTES {
            return Err(Error::Limit);
        }
        let c = &self.state.context;
        c.calls
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                n.checked_add(1).filter(|next| *next <= c.max_calls)
            })
            .map_err(|_| Error::Limit)?;
        c.charge_bytes(bytes as u64)?;
        self.check()
    }
    pub fn charge_response(&self, bytes: usize) -> Result<()> {
        self.check()?;
        if bytes == 0 || bytes > crate::MAX_BLOB_UPLOAD_RESPONSE_BYTES {
            return Err(Error::Limit);
        }
        self.state.context.charge_bytes(bytes as u64)?;
        self.check()
    }
    /// Conservative native-work reservation before Store access. No failed call,
    /// cancellation, drop or smaller encoded reply refunds this instance ledger.
    pub fn charge_native_work(&self, bytes: u64) -> Result<()> {
        self.check()?;
        if bytes == 0 {
            return Err(Error::Limit);
        }
        self.state
            .context
            .native_work_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(bytes)
                    .filter(|next| *next <= MAX_NATIVE_WORK_BYTES)
            })
            .map_err(|_| Error::Limit)?;
        self.check()
    }
}
