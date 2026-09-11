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
        let runner = if package.manifest().guest_abi_version == 2 {
            Runner::new_task(package.module(), limits)?
        } else {
            Runner::new(package.module(), limits)?
        };
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
        if host.connection_phase(connection) != Ok(morrow_core::lifecycle::InstancePhase::Ready) {
            return Report {
                outcome: Err(Fault::InactiveConnection),
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

#[derive(Debug)]
pub struct TaskReport {
    pub execution: Report,
    pub response: Option<morrow_core::response::Response>,
    pub output: Option<morrow_core::task::TransformOutput>,
}
impl PreparedPackage {
    pub fn run_task(
        &self,
        host: &mut HostRuntime,
        connection: &Connection,
        input: &morrow_core::task::Invocation,
        mut clock: impl FnMut() -> u64,
        cancel: Cancellation,
    ) -> TaskReport {
        let fault = if connection.package_digest() != Some(self.package.digest()) {
            Some(Fault::PackageBinding)
        } else if host.connection_phase(connection)
            != Ok(morrow_core::lifecycle::InstancePhase::Ready)
        {
            Some(Fault::InactiveConnection)
        } else if self.package.manifest().guest_abi_version != 2 {
            Some(Fault::UnsupportedAbi)
        } else {
            None
        };
        if let Some(fault) = fault {
            return TaskReport {
                execution: Report {
                    outcome: Err(fault),
                    host_calls: 0,
                    fuel_remaining: self.limits.fuel,
                },
                response: None,
                output: None,
            };
        }
        let mut actual = None;
        let mut protocol_fault = false;
        let mut called = false;
        let run = self.runner.run_task(
            input.bytes(),
            &mut |command| {
                if input.transform().is_some()
                    || called
                    || protocol_fault
                    || command != input.command_bytes()
                {
                    protocol_fault = true;
                    return Err(());
                }
                called = true;
                let response = host
                    .dispatch(connection, command, &mut clock)
                    .map_err(|_| ())?;
                actual = Some(response.clone());
                Ok(response)
            },
            cancel,
        );
        let mut execution = run.report;
        let mut output = None;
        let response = if execution.outcome.is_ok() && input.transform().is_some() {
            match run.completion {
                Some(completion) if !protocol_fault => match input.verify_output(&completion) {
                    Ok(value) => output = Some(value),
                    Err(_) => execution.outcome = Err(Fault::TaskProtocol),
                },
                _ => execution.outcome = Err(Fault::TaskProtocol),
            };
            None
        } else if execution.outcome.is_ok() {
            match (run.completion, actual) {
                (Some(completion), Some(actual)) if !protocol_fault => {
                    match input.verify_completion(&completion, &actual) {
                        Ok(v) => Some(v),
                        Err(_) => {
                            execution.outcome = Err(Fault::TaskProtocol);
                            None
                        }
                    }
                }
                _ => {
                    execution.outcome = Err(Fault::TaskProtocol);
                    None
                }
            }
        } else {
            None
        };
        TaskReport {
            execution,
            response,
            output,
        }
    }
}
