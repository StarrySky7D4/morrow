//! Host-authorized leases over frozen native mappings; descriptors carry no authority.
//! Retired mappings remain charged and readable by existing borrowers until released.
use crate::{
    Cancellation,
    package::{PreparedPackage, TaskReport},
    shared_memory::FrozenRegion,
};
use morrow_core::{
    dispatch::{Connection, ConnectionBinding, HostBinding, HostRuntime},
    lifecycle::InstancePhase,
    shared_object::Descriptor,
    task::{Invocation, Transform},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};
static NEXT_ARENA: AtomicU64 = AtomicU64::new(1);
const CHARGE_UNIT: usize = 64 * 1024;
#[derive(Debug)]
pub enum Error {
    Core(morrow_core::Error),
    Mapping(std::io::Error),
    Denied,
    Limit,
    Retired,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "shared object: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<morrow_core::Error> for Error {
    fn from(e: morrow_core::Error) -> Self {
        Self::Core(e)
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Mapping(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub objects: usize,
    pub charged_bytes: usize,
    pub leases: usize,
    pub mappings: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            objects: 64,
            charged_bytes: 32 * 1024 * 1024,
            leases: 256,
            mappings: 128,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub objects: usize,
    pub charged_bytes: usize,
    pub leases: usize,
    pub mappings: usize,
}
/// Live host-issued token, not serializable or reconstructible from a descriptor.
#[derive(Clone)]
pub struct Lease {
    arena: u64,
    serial: u64,
}
struct Allocation {
    descriptor: Descriptor,
    region: FrozenRegion,
    charge: usize,
}
struct Published {
    allocation: Arc<Allocation>,
    producer: ConnectionBinding,
    scope: String,
}
struct Permission {
    object: u64,
    consumer: ConnectionBinding,
    expires: u64,
}
/// Existing mappings cannot be made unreadable by retiring an ID or revoking future access.
/// No Clone or writable accessor; Drop releases the actual allocation reference.
pub struct Mapping {
    allocation: Arc<Allocation>,
    lease: Lease,
    active: Arc<AtomicUsize>,
}
impl Mapping {
    #[cfg(windows)]
    pub(crate) fn duplicate_readonly(&self, child: &std::process::Child) -> std::io::Result<u64> {
        self.allocation.region.duplicate_readonly(child)
    }
    pub fn bytes(&self) -> &[u8] {
        self.allocation.region.bytes()
    }
    pub fn descriptor(&self) -> &Descriptor {
        &self.allocation.descriptor
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}
pub struct TransformRequest<'a> {
    pub task_id: &'a str,
    pub handler: &'a str,
    pub input_type: &'a str,
    pub output_type: &'a str,
}
pub struct SharedObjects {
    arena: u64,
    host: HostBinding,
    next: u64,
    last_tick: u64,
    limits: Limits,
    published: BTreeMap<u64, Published>,
    retired: Vec<Weak<Allocation>>,
    permissions: BTreeMap<u64, Permission>,
    mappings: Arc<AtomicUsize>,
}
impl SharedObjects {
    pub fn new(host: &HostRuntime, limits: Limits) -> Result<Self> {
        if limits.objects == 0
            || limits.objects > 1024
            || limits.charged_bytes < CHARGE_UNIT
            || limits.charged_bytes > 256 * 1024 * 1024
            || limits.leases == 0
            || limits.leases > 4096
            || limits.mappings == 0
            || limits.mappings > 4096
        {
            return Err(Error::Limit);
        }
        let arena = NEXT_ARENA
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| Error::Limit)?;
        Ok(Self {
            arena,
            host: host.binding(),
            next: 1,
            last_tick: 0,
            limits,
            published: BTreeMap::new(),
            retired: vec![],
            permissions: BTreeMap::new(),
            mappings: Arc::new(AtomicUsize::new(0)),
        })
    }
    fn tick(&mut self, now: u64) -> Result<()> {
        if now < self.last_tick {
            return Err(Error::Denied);
        }
        self.last_tick = now;
        Ok(())
    }
    fn serial(&mut self) -> Result<u64> {
        let value = self.next;
        self.next = self.next.checked_add(1).ok_or(Error::Limit)?;
        Ok(value)
    }
    fn connection(&self, host: &HostRuntime, connection: &Connection) -> Result<()> {
        if host.binding() != self.host
            || host.connection_phase(connection) != Ok(InstancePhase::Ready)
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn object(&self, descriptor: &Descriptor) -> Result<&Published> {
        descriptor.validate()?;
        if descriptor.arena != self.arena {
            return Err(Error::Denied);
        }
        let value = self
            .published
            .get(&descriptor.object)
            .ok_or(Error::Retired)?;
        if &value.allocation.descriptor != descriptor {
            return Err(Error::Denied);
        }
        Ok(value)
    }
    pub fn usage(&mut self) -> Usage {
        self.retired.retain(|a| a.strong_count() > 0);
        let mut objects = self.published.len();
        let mut charged_bytes = self.published.values().map(|a| a.allocation.charge).sum();
        for old in &self.retired {
            if let Some(a) = old.upgrade() {
                objects += 1;
                charged_bytes += a.charge;
            }
        }
        Usage {
            objects,
            charged_bytes,
            leases: self.permissions.len(),
            mappings: self.mappings.load(Ordering::Acquire),
        }
    }
    /// Reclaims only resources whose real borrowers have released them. Expiry alone never unmaps pages.
    pub fn collect(&mut self, now: u64) -> Result<Usage> {
        self.tick(now)?;
        self.permissions.retain(|_, p| p.expires > now);
        Ok(self.usage())
    }
    /// Trusted publication decision: scope comes from host policy, never from an untrusted caller ID.
    pub fn publish(
        &mut self,
        host: &HostRuntime,
        producer: &Connection,
        scope: &str,
        bytes: &[u8],
        now: u64,
    ) -> Result<Descriptor> {
        self.connection(host, producer)?;
        if scope.is_empty() || scope.len() > 256 || scope.chars().any(char::is_control) {
            return Err(Error::Denied);
        }
        if bytes.is_empty() || bytes.len() as u64 > morrow_core::shared_object::MAX_OBJECT_BYTES {
            return Err(Error::Limit);
        }
        let charge = bytes.len().div_ceil(CHARGE_UNIT) * CHARGE_UNIT;
        let usage = self.usage();
        if usage.objects >= self.limits.objects
            || charge
                > self
                    .limits
                    .charged_bytes
                    .saturating_sub(usage.charged_bytes)
        {
            return Err(Error::Limit);
        }
        self.tick(now)?;
        let object = self.serial()?;
        let region = FrozenRegion::copy_from(bytes)?;
        // Hash the fixed mapping, not a former producer view.
        let descriptor = Descriptor::for_bytes(self.arena, object, 1, region.bytes())?;
        self.connection(host, producer)?;
        self.published.insert(
            object,
            Published {
                allocation: Arc::new(Allocation {
                    descriptor: descriptor.clone(),
                    region,
                    charge,
                }),
                producer: producer.binding(),
                scope: scope.into(),
            },
        );
        Ok(descriptor)
    }
    /// Explicit host grant; no automatic authority transfer from producer to consumer.
    #[allow(clippy::too_many_arguments)]
    pub fn grant(
        &mut self,
        host: &HostRuntime,
        consumer: &Connection,
        descriptor: &Descriptor,
        scope: &str,
        expires: u64,
        now: u64,
    ) -> Result<Lease> {
        self.connection(host, consumer)?;
        let value = self.object(descriptor)?;
        if value.scope != scope
            || host.binding_phase(value.producer) != Ok(InstancePhase::Ready)
            || expires <= now
        {
            return Err(Error::Denied);
        }
        self.collect(now)?;
        if self.permissions.len() >= self.limits.leases {
            return Err(Error::Limit);
        }
        let serial = self.serial()?;
        self.permissions.insert(
            serial,
            Permission {
                object: descriptor.object,
                consumer: consumer.binding(),
                expires,
            },
        );
        Ok(Lease {
            arena: self.arena,
            serial,
        })
    }
    fn admitted(
        &self,
        host: &HostRuntime,
        consumer: &Connection,
        lease: &Lease,
        now: u64,
    ) -> Result<&Published> {
        self.connection(host, consumer)?;
        if lease.arena != self.arena {
            return Err(Error::Denied);
        }
        let permission = self.permissions.get(&lease.serial).ok_or(Error::Denied)?;
        if permission.consumer != consumer.binding() || now >= permission.expires {
            return Err(Error::Denied);
        }
        self.published.get(&permission.object).ok_or(Error::Retired)
    }
    pub fn map(
        &mut self,
        host: &HostRuntime,
        consumer: &Connection,
        lease: &Lease,
        now: u64,
    ) -> Result<Mapping> {
        self.admitted(host, consumer, lease, now)?;
        self.tick(now)?;
        let value = self.admitted(host, consumer, lease, now)?;
        self.mappings
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                (v < self.limits.mappings).then_some(v + 1)
            })
            .map_err(|_| Error::Limit)?;
        Ok(Mapping {
            allocation: Arc::clone(&value.allocation),
            lease: lease.clone(),
            active: Arc::clone(&self.mappings),
        })
    }
    pub fn revoke(&mut self, lease: &Lease) -> Result<()> {
        if lease.arena != self.arena || self.permissions.remove(&lease.serial).is_none() {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub fn retire(&mut self, descriptor: &Descriptor) -> Result<()> {
        self.object(descriptor)?;
        let value = self
            .published
            .remove(&descriptor.object)
            .ok_or(Error::Retired)?;
        self.permissions
            .retain(|_, p| p.object != descriptor.object);
        // No allocator pool: the old allocation is kept by real Mapping owners, never retagged.
        self.retired.push(Arc::downgrade(&value.allocation));
        drop(value);
        self.usage();
        Ok(())
    }
    /// Admission preflight for a trusted dependency route; invalid input must not move broker time.
    pub(crate) fn check_mapping_scope(
        &self,
        host: &HostRuntime,
        consumer: &Connection,
        mapping: &Mapping,
        scope: &str,
        now: u64,
    ) -> Result<()> {
        let value = self.admitted(host, consumer, &mapping.lease, now)?;
        if now < self.last_tick
            || value.scope != scope
            || !Arc::ptr_eq(&value.allocation, &mapping.allocation)
            || host.binding_phase(value.producer) != Ok(InstancePhase::Ready)
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub(crate) fn validate_mapping(
        &mut self,
        host: &HostRuntime,
        consumer: &Connection,
        mapping: &Mapping,
        now: u64,
    ) -> Result<()> {
        let value = self.admitted(host, consumer, &mapping.lease, now)?;
        if !Arc::ptr_eq(&value.allocation, &mapping.allocation) {
            return Err(Error::Denied);
        }
        self.tick(now)
    }
    /// Only a host-selected trusted reader may receive this raw mapping. No guest chooses the command.
    #[cfg(windows)]
    pub fn start_reader(
        &mut self,
        host: &HostRuntime,
        consumer: &Connection,
        lease: &Lease,
        command: &mut std::process::Command,
        mut clock: impl FnMut() -> u64,
    ) -> Result<crate::remote_reader::RemoteReader> {
        let mapping = self.map(host, consumer, lease, clock())?;
        crate::remote_reader::RemoteReader::spawn(command, mapping, |mapping| {
            self.validate_mapping(host, consumer, mapping, clock())
        })
    }
    /// Pure guest transform over fixed bytes, followed by final authorization before result delivery.
    /// The Wasm copy is intentional: native mappings are not guest linear-memory addresses.
    #[allow(clippy::too_many_arguments)]
    pub fn run_transform(
        &mut self,
        host: &mut HostRuntime,
        consumer: &Connection,
        package: &PreparedPackage,
        mapping: &Mapping,
        request: TransformRequest<'_>,
        mut clock: impl FnMut() -> u64,
        cancel: Cancellation,
    ) -> Result<TaskReport> {
        self.validate_mapping(host, consumer, mapping, clock())?;
        if mapping.bytes().len() > morrow_core::task::MAX_VALUE_BYTES {
            return Err(Error::Limit);
        }
        let task = Invocation::new_transform(
            request.task_id,
            Transform {
                handler: request.handler.into(),
                input_type: request.input_type.into(),
                output_type: request.output_type.into(),
                input: mapping.bytes().to_vec(),
            },
        )?;
        let report = package.run_task(host, consumer, &task, &mut clock, cancel);
        self.validate_mapping(host, consumer, mapping, clock())?;
        if report.execution.host_calls != 0 || report.response.is_some() {
            return Err(Error::Denied);
        }
        Ok(report)
    }
}
