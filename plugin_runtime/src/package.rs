//! Optional trusted adapter. The module and connection must refer to the same archive.
use crate::{Cancellation, Fault, Limits, Report, Runner};
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    plugin_package::Package,
};
pub struct PreparedPackage {
    package: Package,
    runner: Runner,
    limits: Limits,
}
impl PreparedPackage {
    pub fn new(package: Package, host_limits: Limits) -> Result<Self, Fault> {
        // Validate host policy before intersecting; an invalid policy must not silently pass.
        if host_limits.fuel == 0
            || host_limits.fuel > 100_000_000
            || host_limits.memory_bytes < 65536
            || host_limits.memory_bytes > 64 * 1024 * 1024
            || host_limits.host_calls > 1024
        {
            return Err(Fault::Limits);
        }
        let budget = package
            .manifest()
            .budget
            .as_ref()
            .expect("validated package budget");
        let limits = Limits {
            fuel: host_limits.fuel.min(budget.fuel),
            memory_bytes: host_limits.memory_bytes.min(budget.memory_bytes as usize),
            host_calls: host_limits.host_calls.min(budget.host_calls),
        };
        let runner = Runner::new(package.module(), limits)?;
        Ok(Self {
            package,
            runner,
            limits,
        })
    }
    pub fn package(&self) -> &Package {
        &self.package
    }
    pub fn limits(&self) -> Limits {
        self.limits
    }
    /// No grants are created here. This is called only after host approval, not by the guest.
    pub fn connect(&self, host: &mut HostRuntime) -> morrow_core::Result<Connection> {
        host.connect_package(&self.package)
    }
    /// Clock belongs to the trusted host. No guest-selected identity or callback is accepted.
    pub fn run(
        &self,
        host: &mut HostRuntime,
        connection: &Connection,
        mut clock: impl FnMut() -> u64,
        cancel: Cancellation,
    ) -> Report {
        if connection.package_digest() != Some(self.package.digest()) {
            return Report {
                outcome: Err(Fault::PackageBinding),
                host_calls: 0,
                fuel_remaining: self.limits.fuel,
            };
        }
        self.runner.run(
            &mut |input| host.dispatch(connection, input, &mut clock).map_err(|_| ()),
            cancel,
        )
    }
}
