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
        Self::prepare(package, input, limits, true)
    }
    pub(crate) fn new_observation(
        package: &Package,
        input: &Invocation,
        limits: Limits,
    ) -> Result<Self> {
        Self::prepare(package, input, limits, false)
    }
    fn prepare(
        package: &Package,
        input: &Invocation,
        limits: Limits,
        include_package: bool,
    ) -> Result<Self> {
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
                package_archive: if include_package {
                    package.archive().to_vec()
                } else {
                    vec![]
                },
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
    pub(crate) fn observation(
        self,
        report: &TaskReport,
        completion: Vec<u8>,
    ) -> task_evidence::proto::Observation {
        let (fault, exit_code) = match &report.execution.outcome {
            Ok(exit) => (StableFault::Unspecified as i32, Some(*exit)),
            Err(fault) => (stable(fault) as i32, None),
        };
        task_evidence::proto::Observation {
            invocation: self.data.invocation,
            budget: self.data.budget,
            backend: self.data.backend,
            completion,
            fault,
            exit_code,
            observed_host_calls: report.execution.host_calls,
            fuel_remaining: report.execution.fuel_remaining,
        }
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

/// One ordered, complete batch recorded under one live package binding.
pub struct CapturedBatch {
    pub(crate) reports: Vec<TaskReport>,
    pub(crate) evidence: Evidence,
}
impl CapturedBatch {
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }
    pub fn reports(&self) -> &[TaskReport] {
        &self.reports
    }
    pub fn into_parts(self) -> (Vec<TaskReport>, Evidence) {
        (self.reports, self.evidence)
    }
}
pub struct BatchReplayResult {
    pub matches: bool,
    pub reports: Vec<TaskReport>,
}
/// No host or store is accepted. Every page uses the original per-page budget in a fresh
/// runner; total fuel is additionally bounded. A mismatch stops the batch immediately.
pub fn replay_batch(
    evidence: &Evidence,
    policy: Limits,
    total_fuel: u64,
) -> Result<BatchReplayResult> {
    if evidence.data().schema_version != task_evidence::BATCH_VERSION || !policy_valid(policy) {
        return Err(Error::Policy);
    }
    let batch = evidence.data().batch.as_ref().ok_or(Error::Policy)?;
    if total_fuel == 0
        || total_fuel > task_evidence::MAX_TOTAL_FUEL
        || batch.total_fuel > total_fuel
    {
        return Err(Error::Policy);
    }
    // Reject an unsupported later page before executing any earlier page.
    for observation in &batch.observations {
        if observation.backend != task_evidence::BACKEND {
            return Err(Error::UnsupportedBackend);
        }
        let budget = observation.budget.as_ref().ok_or(Error::Policy)?;
        if budget.fuel > policy.fuel
            || budget.memory_bytes > policy.memory_bytes as u64
            || budget.host_calls > policy.host_calls
        {
            return Err(Error::Policy);
        }
    }
    let package = Package::decode(&evidence.data().package_archive)?;
    let mut remaining = batch.total_fuel;
    let mut reports = Vec::with_capacity(batch.observations.len());
    for observation in &batch.observations {
        let budget = observation.budget.as_ref().ok_or(Error::Policy)?;
        if budget.fuel > remaining {
            return Err(Error::Policy);
        }
        let limits = Limits {
            fuel: budget.fuel,
            memory_bytes: usize::try_from(budget.memory_bytes).map_err(|_| Error::Policy)?,
            host_calls: budget.host_calls,
        };
        let input = Invocation::decode(&observation.invocation)?;
        let prepared = PreparedCapture::new_observation(&package, &input, limits)?;
        let (report, completion) = prepared.execute(Cancellation::default());
        let consumed = limits
            .fuel
            .checked_sub(report.execution.fuel_remaining)
            .ok_or(Error::Policy)?;
        remaining = remaining.checked_sub(consumed).ok_or(Error::Policy)?;
        let matches = report.execution.outcome == Ok(0)
            && report.response.is_none()
            && report.failure.is_none()
            && report.output.is_some()
            && completion == observation.completion
            && report.execution.host_calls == observation.observed_host_calls
            && report.execution.fuel_remaining == observation.fuel_remaining;
        reports.push(report);
        if !matches {
            return Ok(BatchReplayResult {
                matches: false,
                reports,
            });
        }
    }
    Ok(BatchReplayResult {
        matches: true,
        reports,
    })
}
