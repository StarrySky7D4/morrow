//! Self-contained pure-transform history. Integrity is not provenance or commit authority.
use crate::{Cancellation, Fault, Limits, Runner, package::TaskReport};
use morrow_core::{
    plugin_package::Package,
    task::{Invocation, TransformResult},
    task_evidence::{
        self, Evidence,
        proto::{ExecutionBudget, StableFault, TaskEvidence},
    },
};
#[derive(Debug)]
pub enum Error {
    Core(morrow_core::Error),
    Runtime(Fault),
    UnsupportedBackend,
    UnsupportedOutcome,
    Policy,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "transform replay: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<morrow_core::Error> for Error {
    fn from(e: morrow_core::Error) -> Self {
        Self::Core(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
/// Created by actual execution, but the container alone carries no signature or authority.
pub struct CapturedTransform {
    report: TaskReport,
    evidence: Evidence,
}
impl CapturedTransform {
    pub fn report(&self) -> &TaskReport {
        &self.report
    }
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }
    pub fn into_parts(self) -> (TaskReport, Evidence) {
        (self.report, self.evidence)
    }
}
pub struct ReplayResult {
    pub matches: bool,
    pub report: TaskReport,
}
pub(crate) struct PreparedCapture {
    data: TaskEvidence,
    input: Invocation,
    runner: Runner,
    max_output: usize,
}
fn stable(fault: &Fault) -> StableFault {
    match fault {
        Fault::TaskProtocol => StableFault::TaskProtocol,
        Fault::Deadline => StableFault::Deadline,
        Fault::PackageBinding => StableFault::PackageBinding,
        Fault::InactiveConnection => StableFault::InactiveConnection,
        Fault::InvalidModule => StableFault::InvalidModule,
        Fault::UnsupportedAbi => StableFault::UnsupportedAbi,
        Fault::Limits => StableFault::Limits,
        Fault::Cancelled => StableFault::Cancelled,
        Fault::Trap => StableFault::Trap,
    }
}
fn policy_valid(l: Limits) -> bool {
    l.fuel > 0
        && l.fuel <= 100_000_000
        && (65536..=64 * 1024 * 1024).contains(&l.memory_bytes)
        && l.host_calls <= 1024
}
impl PreparedCapture {
    pub(crate) fn new(package: &Package, input: &Invocation, limits: Limits) -> Result<Self> {
        let budget = ExecutionBudget {
            fuel: limits.fuel,
            memory_bytes: limits.memory_bytes as u64,
            host_calls: limits.host_calls,
        };
        task_evidence::validate_plan(package, input, &budget)?;
        let max_output = package
            .transform_handler(input.transform().ok_or(Error::Policy)?)?
            .max_output_bytes as usize;
        let runner = Runner::new_task(package.module(), limits).map_err(Error::Runtime)?;
        Ok(Self {
            data: TaskEvidence {
                schema_version: task_evidence::VERSION,
                package_archive: package.archive().to_vec(),
                invocation: input.bytes().to_vec(),
                budget: Some(budget),
                backend: task_evidence::BACKEND.into(),
                ..Default::default()
            },
            input: input.clone(),
            runner,
            max_output,
        })
    }
    pub(crate) fn execute(&self, cancel: Cancellation) -> (TaskReport, Vec<u8>) {
        let mut forbidden = false;
        let run = self.runner.run_task(
            self.input.bytes(),
            &mut |_| {
                forbidden = true;
                Err(())
            },
            cancel,
        );
        let mut execution = run.report;
        if forbidden && execution.outcome.is_ok() {
            execution.outcome = Err(Fault::TaskProtocol);
        }
        // These are the actual bytes retained by Runner, never reconstructed from typed output.
        let completion = run.completion.unwrap_or_default();
        let mut output = None;
        let mut failure = None;
        if execution.outcome.is_ok() {
            match self.input.verify_transform_result(&completion) {
                Ok(TransformResult::Output(value)) if value.bytes.len() <= self.max_output => {
                    output = Some(value)
                }
                Ok(TransformResult::Failure(value)) => failure = Some(value),
                _ => execution.outcome = Err(Fault::TaskProtocol),
            }
        }
        (
            TaskReport {
                execution,
                response: None,
                output,
                failure,
            },
            completion,
        )
    }
    pub(crate) fn finish(
        mut self,
        report: TaskReport,
        completion: Vec<u8>,
    ) -> Result<CapturedTransform> {
        self.data.completion = completion;
        self.data.observed_host_calls = report.execution.host_calls;
        self.data.fuel_remaining = report.execution.fuel_remaining;
        match &report.execution.outcome {
            Ok(exit) => {
                self.data.fault = StableFault::Unspecified as i32;
                self.data.exit_code = Some(*exit);
            }
            Err(fault) => {
                self.data.fault = stable(fault) as i32;
                self.data.exit_code = None;
            }
        }
        let evidence = task_evidence::encode(self.data)?;
        Ok(CapturedTransform { report, evidence })
    }
}
/// Executes only embedded bytes in a fresh no-WASI runner. No HostRuntime, filesystem,
/// database, identity, credential or restored grant is accepted by this interface.
pub fn replay(evidence: &Evidence, policy: Limits) -> Result<ReplayResult> {
    let data = evidence.data();
    if data.backend != task_evidence::BACKEND {
        return Err(Error::UnsupportedBackend);
    }
    match StableFault::try_from(data.fault).map_err(|_| Error::UnsupportedOutcome)? {
        StableFault::Unspecified
        | StableFault::Trap
        | StableFault::TaskProtocol
        | StableFault::Limits => {}
        _ => return Err(Error::UnsupportedOutcome),
    }
    let budget = data.budget.as_ref().ok_or(Error::Policy)?;
    if !policy_valid(policy)
        || budget.fuel > policy.fuel
        || budget.memory_bytes > policy.memory_bytes as u64
        || budget.host_calls > policy.host_calls
    {
        return Err(Error::Policy);
    }
    let limits = Limits {
        fuel: budget.fuel,
        memory_bytes: usize::try_from(budget.memory_bytes).map_err(|_| Error::Policy)?,
        host_calls: budget.host_calls,
    };
    let package = Package::decode(&data.package_archive)?;
    let input = Invocation::decode(&data.invocation)?;
    let prepared = PreparedCapture::new(&package, &input, limits)?;
    let (report, completion) = prepared.execute(Cancellation::default());
    let (fault, exit) = match &report.execution.outcome {
        Ok(exit) => (StableFault::Unspecified, Some(*exit)),
        Err(f) => (stable(f), None),
    };
    let matches = fault as i32 == data.fault
        && exit == data.exit_code
        && completion == data.completion
        && report.execution.host_calls == data.observed_host_calls
        && report.execution.fuel_remaining == data.fuel_remaining;
    Ok(ReplayResult { matches, report })
}
