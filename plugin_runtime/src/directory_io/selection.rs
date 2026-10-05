//! Original FileList-bound descendant selection relative to a trusted opened anchor.
//! This proves no ancestors above that object or picker-time pathname identity.
use super::{
    DirectoryClock, DirectoryRelativePath, Error, ROOT_QUERY_BYTES, cancellation_veto,
    native_windows,
};
use crate::{
    io_binding::{IoBinding, IoJobLease, IoResourceLease},
    manager::{ManagedInstance, Manager},
};
use morrow_core::{dispatch::HostRuntime, plugin_package::io::IoCapability};
use std::fs::File;

struct Ancestor {
    // Drop the actual object before releasing its resource reservation.
    file: File,
    identity: [u8; 24],
    lease: IoResourceLease,
}
#[derive(Default)]
pub(super) struct Ancestors {
    nodes: Vec<Ancestor>,
}
impl Ancestors {
    pub(super) fn query_bytes(&self) -> u64 {
        self.nodes.len() as u64 * ROOT_QUERY_BYTES
    }
    pub(super) fn memory_allowance(path: &DirectoryRelativePath) -> u64 {
        // Both buffers coexist during opening. This bounds owned bookkeeping,
        // not the OS handle footprint, allocator overhead or process RSS.
        path.retained_input_bytes() as u64
            + path.component_count() as u64
                * (size_of::<Ancestor>() + size_of::<IoResourceLease>()) as u64
    }
    /// Each same-handle query is outside original-clock guards. Caller charges
    /// it before OS access; no pathname is reopened or child authority inferred.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn verify<C: DirectoryClock>(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        clock: &mut C,
        cancelled: &mut impl FnMut() -> bool,
        mut reserve: impl FnMut(&mut C) -> Result<(), Error>,
    ) -> Result<(), Error> {
        for node in &self.nodes {
            clock.with(|now| {
                node.lease
                    .check(manager, host, instance, now)
                    .map_err(Error::from)
            })?;
            reserve(clock)?;
            cancellation_veto(cancelled)?;
            if native_windows::identity(&node.file)? != node.identity {
                return Err(Error::SourceChanged);
            }
            cancellation_veto(cancelled)?;
            clock.with(|now| {
                node.lease
                    .check(manager, host, instance, now)
                    .map_err(Error::from)
            })?;
        }
        Ok(())
    }
}

/// Reserves every retained ancestor before the first native query/open. The
/// broker already reserved the leaf, so anchor plus N children cost N+1 slots
/// in the same original (at most eight) resource ledger. Partial failures drop
/// actual handles before their leases, while admitted byte fees stay spent.
#[allow(clippy::too_many_arguments)]
pub(super) fn open_under(
    manager: &Manager,
    host: &HostRuntime,
    instance: &ManagedInstance,
    binding: &IoBinding,
    anchor: File,
    path: DirectoryRelativePath,
    leaf_lease: &IoResourceLease,
    job: &IoJobLease,
    clock: &mut impl DirectoryClock,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(File, Ancestors), Error> {
    clock.with(|now| {
        leaf_lease.check(manager, host, instance, now)?;
        job.charge(
            &[IoCapability::FileList],
            Ancestors::memory_allowance(&path),
            now,
        )?;
        Ok(())
    })?;
    // Declare reservations before handles so failed native work closes every
    // owned object before unconsumed resource reservations are released.
    let mut reservations = Vec::new();
    reservations
        .try_reserve_exact(path.component_count())
        .map_err(|_| Error::Allocation)?;
    for _ in path.as_components() {
        let lease = clock.with(|now| {
            binding
                .admit_resource(manager, host, instance, &[IoCapability::FileList], now)
                .map_err(Error::from)
        })?;
        reservations.push(lease);
    }
    let mut ancestors = Ancestors::default();
    ancestors
        .nodes
        .try_reserve_exact(path.component_count())
        .map_err(|_| Error::Allocation)?;
    let mut current = anchor;
    clock.with(|now| {
        leaf_lease.check(manager, host, instance, now)?;
        job.charge(&[IoCapability::FileList], ROOT_QUERY_BYTES, now)?;
        Ok(())
    })?;
    cancellation_veto(cancelled)?;
    let mut identity = native_windows::identity(&current)?;
    cancellation_veto(cancelled)?;
    clock.with(|now| {
        leaf_lease
            .check(manager, host, instance, now)
            .map_err(Error::from)
    })?;
    for (index, component) in path.as_components().iter().enumerate() {
        ancestors.verify(manager, host, instance, clock, cancelled, |clock| {
            clock.with(|now| {
                job.charge(&[IoCapability::FileList], ROOT_QUERY_BYTES, now)
                    .map_err(Error::from)
            })
        })?;
        clock.with(|now| {
            leaf_lease.check(manager, host, instance, now)?;
            // Owned UTF16 argument copy in the narrow native boundary, parent
            // and returned-child object queries, and fixed native open request.
            job.charge(
                &[IoCapability::FileList],
                component.len() as u64 * 2 + ROOT_QUERY_BYTES * 2 + 128,
                now,
            )?;
            Ok(())
        })?;
        cancellation_veto(cancelled)?;
        if native_windows::identity(&current)? != identity {
            return Err(Error::SourceChanged);
        }
        cancellation_veto(cancelled)?;
        clock.with(|now| {
            leaf_lease
                .check(manager, host, instance, now)
                .map_err(Error::from)
        })?;
        let child = crate::file_target::open_relative_directory(
            &current,
            component,
            index + 1 == path.component_count(),
        )
        .map_err(|error| Error::Io(error.kind()))?;
        cancellation_veto(cancelled)?;
        clock.with(|now| {
            leaf_lease
                .check(manager, host, instance, now)
                .map_err(Error::from)
        })?;
        let child_identity = native_windows::identity(&child)?;
        cancellation_veto(cancelled)?;
        clock.with(|now| {
            leaf_lease
                .check(manager, host, instance, now)
                .map_err(Error::from)
        })?;
        // pop moves an already-reserved lease without allocating another buffer.
        ancestors.nodes.push(Ancestor {
            file: current,
            identity,
            lease: reservations.pop().ok_or(Error::Limit)?,
        });
        current = child;
        identity = child_identity;
    }
    ancestors.verify(manager, host, instance, clock, cancelled, |clock| {
        clock.with(|now| {
            job.charge(&[IoCapability::FileList], ROOT_QUERY_BYTES, now)
                .map_err(Error::from)
        })
    })?;
    Ok((current, ancestors))
}
