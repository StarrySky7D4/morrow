//! Fixed managed entry for the combined agent session/process task profile.
use super::{Control, MAX_INSTANCES, Manager, Result};
use crate::{Cancellation, Fault, Limits, Runner};
use morrow_core::{
    Error,
    dispatch::{Connection, HostBinding, HostRuntime},
    lifecycle::InstancePhase,
    plugin_package::Package,
};
use std::sync::{Arc, Weak};

/// Original registry-selected package, connection and cancellation under one manager.
/// This binds base-package approval only; session/process grants require separate approval.
pub struct ManagedAgentSessionProcessInstance {
    package: Package,
    _runner: Runner,
    limits: Limits,
    connection: Arc<Connection>,
    host: HostBinding,
    control: Arc<Control>,
    manager: Weak<()>,
}
impl ManagedAgentSessionProcessInstance {
    pub fn package(&self) -> &Package {
        &self.package
    }
    pub fn limits(&self) -> Limits {
        self.limits
    }
    pub fn shared_connection(&self) -> Arc<Connection> {
        Arc::clone(&self.connection)
    }
    pub fn cancellation(&self) -> Cancellation {
        self.control.cancel.clone()
    }
    /// Local managed liveness, not a substitute for validation against the original host.
    pub fn is_active(&self) -> bool {
        self.manager.upgrade().is_some()
            && self.control.active()
            && self.connection.binding() == self.control.binding
            && self.connection.package_digest() == Some(self.package.digest())
    }
    pub fn request_stop(&self) {
        self.control.signal_stop();
    }
    pub fn close(&self, host: &mut HostRuntime) -> morrow_core::Result<()> {
        self.control.stop();
        if host.binding() != self.host || self.connection.binding() != self.control.binding {
            return Err(Error::Invalid("foreign managed agent process host"));
        }
        host.disconnect(&self.connection)
    }
}
impl Drop for ManagedAgentSessionProcessInstance {
    fn drop(&mut self) {
        self.control.stop();
    }
}

fn prepare(package: &Package, host: Limits) -> std::result::Result<(Runner, Limits), Fault> {
    let manifest = package.manifest();
    if manifest.guest_abi_version != 2
        || !manifest.required_features.is_empty()
        || !manifest.dependencies.is_empty()
        || !manifest.transform_handlers.is_empty()
        || !package.capabilities().is_empty()
        || !package.io_capabilities().is_empty()
        || package.io_declaration().is_some()
        || package.channel_declaration().is_some()
        || package.mutation_enabled()
    {
        return Err(Fault::UnsupportedAbi);
    }
    if host.fuel == 0
        || host.fuel > 100_000_000
        || host.memory_bytes < 65536
        || host.memory_bytes > 64 * 1024 * 1024
        || host.host_calls > 1024
    {
        return Err(Fault::Limits);
    }
    let budget = manifest.budget.as_ref().ok_or(Fault::Limits)?;
    let limits = Limits {
        fuel: host.fuel.min(budget.fuel),
        memory_bytes: host.memory_bytes.min(budget.memory_bytes as usize),
        host_calls: host.host_calls.min(budget.host_calls),
    };
    let runner = Runner::new_agent_session_process_task(package.module(), limits)?;
    Ok((runner, limits))
}

impl Manager {
    /// Stop every original control for this exact selected base before a wrapper decision.
    /// The shared table includes ordinary instances, which are conservatively stopped too.
    /// This publishes no registry state and does not advance its revision or revive controls.
    pub fn revoke_agent_session_process(
        &mut self,
        id: &str,
        digest: [u8; 32],
        expected_registry_revision: u64,
    ) -> Result<()> {
        self.checked_selection(id, digest, expected_registry_revision)?;
        self.revoke(id);
        Ok(())
    }

    /// Resolve and validate the fixed profile before connecting under the original approval.
    pub fn connect_agent_session_process(
        &mut self,
        id: &str,
        host: &mut HostRuntime,
        expected_registry_revision: u64,
    ) -> Result<ManagedAgentSessionProcessInstance> {
        self.check_revision(expected_registry_revision)?;
        #[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
        {
            self.reap_channels();
            if self.channel_cleanup.len() >= MAX_INSTANCES {
                return Err(Error::Limit.into());
            }
        }
        if self.prune() >= MAX_INSTANCES {
            return Err(Error::Limit.into());
        }
        let (package, selection) = self.registry.resolve_enabled(id)?;
        if selection.digest != package.digest()
            || !selection.approved.is_empty()
            || !selection.approved_io.is_empty()
        {
            return Err(Error::Invalid("mixed managed agent process approval").into());
        }
        let (runner, limits) = prepare(&package, self.limits)?;
        let connection = Arc::new(host.connect_package_approved(&package, &selection.approved)?);
        let revocation = match host.revocation(&connection) {
            Ok(revocation) => revocation,
            Err(error) => {
                let _ = host.disconnect(&connection);
                return Err(error.into());
            }
        };
        let control = Arc::new(Control {
            binding: connection.binding(),
            io: None,
            #[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
            channel: None,
            revocation,
            cancel: Cancellation::default(),
        });
        self.instances
            .entry(id.into())
            .or_default()
            .push(Arc::downgrade(&control));
        Ok(ManagedAgentSessionProcessInstance {
            package,
            _runner: runner,
            limits,
            connection,
            host: host.binding(),
            control,
            manager: self.identity(),
        })
    }

    /// Revalidate the exact original managed control and current registry selection.
    pub fn validate_agent_session_process(
        &self,
        instance: &ManagedAgentSessionProcessInstance,
        host: &HostRuntime,
        expected_registry_revision: u64,
    ) -> Result<()> {
        self.check_revision(expected_registry_revision)?;
        let id = &instance.package.manifest().package_id;
        if !Weak::ptr_eq(&instance.manager, &self.identity())
            || !instance.is_active()
            || host.binding() != instance.host
            || host.connection_phase(&instance.connection) != Ok(InstancePhase::Ready)
            || !self.instances.get(id).is_some_and(|controls| {
                controls
                    .iter()
                    .any(|control| Weak::ptr_eq(control, &Arc::downgrade(&instance.control)))
            })
        {
            return Err(Error::Invalid("inactive managed agent process instance").into());
        }
        let (package, selection) = self.registry.resolve_enabled(id)?;
        if package.digest() != instance.package.digest()
            || selection.digest != instance.package.digest()
            || !selection.approved.is_empty()
            || !selection.approved_io.is_empty()
        {
            return Err(Error::Invalid("stale managed agent process selection").into());
        }
        Ok(())
    }
}
