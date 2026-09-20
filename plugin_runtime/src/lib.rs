#![deny(unsafe_code)]
//! Bounded Wasm backend with an owned continuation and synchronous host driver.
//! No WASI, filesystem or identity imports.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;
use wasmi::{
    Caller, Config, EnforcedLimits, Engine, ExternType, Linker, Module, Store, StoreLimits,
    StoreLimitsBuilder, ValType,
};
mod continuation;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod dependency;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod dynamic_dependencies;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod file_io;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod http_io;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod inline_ui;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod instance_pool;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod io_binding;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod io_execution;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod io_jobs;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod manager;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod package;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod proposal;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod replay;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod service_authority;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod service_content;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod service_history;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod service_io;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod ui_session;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod worker;
// User-approved native boundary; other runtime modules still reject unsafe code.
#[cfg(all(feature = "packages", windows))]
pub mod remote_reader;
#[cfg(not(target_arch = "wasm32"))]
#[allow(unsafe_code)]
pub mod shared_memory;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod shared_objects;
pub const MAX_MESSAGE_BYTES: usize = 65536;
pub const MAX_TASK_BYTES: usize = 128 * 1024;
pub const MAX_MODULE_BYTES: usize = 4 * 1024 * 1024;
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub fuel: u64,
    pub memory_bytes: usize,
    pub host_calls: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            fuel: 20_000_000,
            memory_bytes: 16 * 1024 * 1024,
            host_calls: 16,
        }
    }
}
#[derive(Clone, Default)]
pub struct Cancellation {
    signal: Arc<AtomicBool>,
    deadline: Arc<Mutex<Option<Instant>>>,
    parents: Option<Arc<[Cancellation; 2]>>,
}
impl Cancellation {
    pub fn cancel(&self) {
        self.signal.store(true, Ordering::Release);
    }
    /// Host monotonic deadline; checked at execution/import boundaries, not an OS interrupt.
    pub fn until(deadline: Instant) -> Self {
        let token = Self::default();
        token.limit_deadline(deadline);
        token
    }
    pub(crate) fn limit_deadline(&self, deadline: Instant) {
        let mut current = self.deadline.lock().unwrap_or_else(|e| e.into_inner());
        *current = Some(current.map_or(deadline, |old| old.min(deadline)));
    }
    #[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
    pub(crate) fn linked(first: Self, second: Self) -> Self {
        Self {
            parents: Some(Arc::new([first, second])),
            ..Self::default()
        }
    }
    fn fault(&self) -> Option<Fault> {
        if let Some(parents) = &self.parents
            && let Some(fault) = parents.iter().find_map(Self::fault)
        {
            return Some(fault);
        }

        if self.signal.load(Ordering::Acquire) {
            return Some(Fault::Cancelled);
        }
        if self
            .deadline
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some_and(|v| Instant::now() >= v)
        {
            return Some(Fault::Deadline);
        }
        None
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    TaskProtocol,
    Deadline,
    PackageBinding,
    InactiveConnection,
    InvalidModule,
    UnsupportedAbi,
    Limits,
    Cancelled,
    Trap,
}
#[derive(Debug)]
pub struct Report {
    pub outcome: Result<i32, Fault>,
    pub host_calls: u32,
    pub fuel_remaining: u64,
}
#[derive(Debug)]
pub struct TaskRun {
    pub report: Report,
    pub completion: Option<Vec<u8>>,
}
struct TaskState {
    input: Vec<u8>,
    read: bool,
    completion: Option<Vec<u8>>,
}
/// The closure is trusted and bound to an actual host connection. It must not panic,
/// retain input, re-enter the guest, or return unbounded responses. Calls may commit;
/// a later trap/cancellation never implies rollback. Fuel cannot interrupt this closure.
type Exchange<'a> = &'a mut dyn FnMut(&[u8]) -> Result<Vec<u8>, ()>;
struct State {
    task: Option<TaskState>,
    dependency: bool,
    io: bool,
    pending: Option<continuation::PendingCall>,
    session: Arc<()>,
    limits: StoreLimits,
    cancel: Cancellation,
    calls: u32,
    max_calls: u32,
    stopped: Option<Fault>,
}
pub struct Runner {
    task_abi: bool,
    dependency_abi: bool,
    io_abi: bool,
    engine: Engine,
    module: Module,
    limits: Limits,
}
impl Runner {
    pub fn new(bytes: &[u8], limits: Limits) -> Result<Self, Fault> {
        Self::prepare(bytes, limits, false, false, false)
    }
    pub fn new_task(bytes: &[u8], limits: Limits) -> Result<Self, Fault> {
        Self::prepare(bytes, limits, true, false, false)
    }
    /// Task ABI with one additional fixed dependency import. The callback is host-routed;
    /// it must not re-enter this guest and cannot be supplied to ordinary task runners.
    pub fn new_dependency_task(bytes: &[u8], limits: Limits) -> Result<Self, Fault> {
        Self::prepare(bytes, limits, true, true, false)
    }
    /// Task ABI with the fixed IO import. Combined with dependency imports is rejected.
    pub fn new_io_task(bytes: &[u8], limits: Limits) -> Result<Self, Fault> {
        Self::prepare(bytes, limits, true, false, true)
    }
    fn prepare(
        bytes: &[u8],
        limits: Limits,
        task_abi: bool,
        dependency_abi: bool,
        io_abi: bool,
    ) -> Result<Self, Fault> {
        if io_abi && dependency_abi {
            return Err(Fault::UnsupportedAbi);
        }
        if bytes.len() > MAX_MODULE_BYTES
            || limits.fuel == 0
            || limits.fuel > 100_000_000
            || limits.memory_bytes < 65536
            || limits.memory_bytes > 64 * 1024 * 1024
            || limits.host_calls > 1024
        {
            return Err(Fault::Limits);
        }
        // Static parsing never executes module start or allows imports to run during preparation.
        for payload in wasmparser::Parser::new(0).parse_all(bytes) {
            if matches!(
                payload.map_err(|_| Fault::InvalidModule)?,
                wasmparser::Payload::StartSection { .. }
            ) {
                return Err(Fault::UnsupportedAbi);
            }
        }
        let mut config = Config::default();
        config
            .consume_fuel(true)
            .enforced_limits(EnforcedLimits::strict());
        let engine = Engine::new(&config);
        let module = Module::new(&engine, bytes).map_err(|_| Fault::InvalidModule)?;
        let mut imports = std::collections::BTreeSet::new();
        for import in module.imports() {
            let ExternType::Func(ty) = import.ty() else {
                return Err(Fault::UnsupportedAbi);
            };
            let arity = match (import.module(), import.name()) {
                ("morrow_v1", "exchange") => 4,
                ("morrow_dependency_v1", "call") if dependency_abi => 4,
                ("morrow_io_v1", "call") if io_abi => 4,
                ("morrow_task_v1", "read_input" | "complete") if task_abi => 2,
                _ => return Err(Fault::UnsupportedAbi),
            };
            if !imports.insert((import.module(), import.name()))
                || ty.params() != vec![ValType::I32; arity]
                || ty.results() != [ValType::I32]
            {
                return Err(Fault::UnsupportedAbi);
            }
        }
        let mut memory = false;
        let mut run = false;
        for export in module.exports() {
            match (export.name(), export.ty()) {
                ("memory", ExternType::Memory(_)) => memory = true,
                ("morrow_run", ExternType::Func(ty))
                    if ty.params().is_empty() && ty.results() == [ValType::I32] =>
                {
                    run = true
                }
                _ => {}
            }
        }
        if !memory || !run {
            return Err(Fault::UnsupportedAbi);
        }
        Ok(Self {
            task_abi,
            dependency_abi,
            io_abi,
            engine,
            module,
            limits,
        })
    }
    /// Each call owns a fresh Wasm Store and releases its memory after termination.
    /// Cancellation gates imports and checks before/after execution; a pure loop is
    /// bounded by fuel, not immediately interrupted by the cancellation flag.
    pub fn run(&self, exchange: Exchange<'_>, cancel: Cancellation) -> Report {
        self.execute(exchange, None, None, cancel, None).report
    }
    pub fn run_task<'a>(
        &self,
        input: &'a [u8],
        exchange: Exchange<'a>,
        cancel: Cancellation,
    ) -> TaskRun {
        self.execute(exchange, None, None, cancel, Some(input))
    }
    /// Dependency and core exchange calls share the same bounded import-call counter.
    /// That counter is not a transaction count: a later error cannot imply rollback.
    pub fn run_task_with_dependencies<'a>(
        &self,
        input: &'a [u8],
        exchange: Exchange<'a>,
        dependency: Exchange<'a>,
        cancel: Cancellation,
    ) -> TaskRun {
        self.execute(exchange, Some(dependency), None, cancel, Some(input))
    }
    /// IO and core exchange share the host-call counter. The IO callback must return a
    /// framed response or Err(()) for a fixed transport failure; it must not re-enter.
    /// This raw trusted-host seam grants no authority and validates no IO protocol.
    /// A broker must enforce managed approval, resource scope, cumulative budgets and
    /// revocation before work and result delivery; cancellation cannot undo external effects.
    pub fn run_task_with_io<'a>(
        &self,
        input: &'a [u8],
        exchange: Exchange<'a>,
        io: Exchange<'a>,
        cancel: Cancellation,
    ) -> TaskRun {
        self.execute(exchange, None, Some(io), cancel, Some(input))
    }
    fn execute<'a>(
        &self,
        exchange: Exchange<'a>,
        mut dependency: Option<Exchange<'a>>,
        mut io: Option<Exchange<'a>>,
        cancel: Cancellation,
        input: Option<&'a [u8]>,
    ) -> TaskRun {
        let started = if self.dependency_abi != dependency.is_some() || self.io_abi != io.is_some()
        {
            Err(Fault::UnsupportedAbi)
        } else {
            continuation::Execution::start(self, input, cancel)
        };
        let mut execution = match started {
            Ok(execution) => execution,
            Err(fault) => {
                return TaskRun {
                    report: Report {
                        outcome: Err(fault),
                        host_calls: 0,
                        fuel_remaining: self.limits.fuel,
                    },
                    completion: None,
                };
            }
        };
        // Host callbacks live only in this driver, never in the Wasm Store.
        // The existing entry points remain synchronous until a managed scheduler
        // supplies validated dispatch/delivery phases for the owned execution.
        while let Some(pending) = execution.pending() {
            let token = pending.token.clone();
            let response = match pending.kind {
                continuation::Kind::Core => exchange(&pending.bytes),
                continuation::Kind::Dependency => {
                    dependency.as_mut().expect("checked mode")(&pending.bytes)
                }
                continuation::Kind::Io => io.as_mut().expect("checked mode")(&pending.bytes),
            };
            execution
                .resume(&token, response)
                .expect("same execution and call");
        }
        execution.finish()
    }
}
fn trap(state: &mut State, fault: Fault) -> wasmi::Error {
    state.stopped = Some(fault);
    wasmi::Error::new("guest boundary stopped")
}
fn host_exchange(
    mut caller: Caller<'_, State>,
    input: i32,
    length: i32,
    output: i32,
    capacity: i32,
) -> Result<i32, wasmi::Error> {
    if let Some(fault) = caller.data().cancel.fault() {
        return Err(trap(caller.data_mut(), fault));
    }
    if caller
        .data()
        .task
        .as_ref()
        .is_some_and(|t| t.completion.is_some())
    {
        return Err(trap(caller.data_mut(), Fault::TaskProtocol));
    }
    if caller.data().calls >= caller.data().max_calls {
        return Err(trap(caller.data_mut(), Fault::Limits));
    }
    // Negative i32 pointers cannot name memory under the configured <=64MiB limit.
    if input < 0
        || output < 0
        || length <= 0
        || length as usize > MAX_MESSAGE_BYTES
        || capacity as usize != MAX_MESSAGE_BYTES
    {
        return Err(trap(caller.data_mut(), Fault::Trap));
    }
    let memory = caller
        .get_export("memory")
        .and_then(|v| v.into_memory())
        .ok_or_else(|| wasmi::Error::new("missing memory"))?;
    let input = input as usize;
    let output = output as usize;
    let length = length as usize;
    let input_end = input
        .checked_add(length)
        .ok_or_else(|| wasmi::Error::new("input overflow"))?;
    let output_end = output
        .checked_add(MAX_MESSAGE_BYTES)
        .ok_or_else(|| wasmi::Error::new("output overflow"))?;
    let memory_len = memory.data(&caller).len();
    if input_end > memory_len
        || output_end > memory_len
        || (input < output_end && output < input_end)
    {
        return Err(trap(caller.data_mut(), Fault::Trap));
    }
    // Freeze input and validate full output capacity before a potentially committing call.
    let fixed = memory.data(&caller)[input..input_end].to_vec();
    caller.data_mut().calls += 1;
    Err(continuation::suspend(
        caller.data_mut(),
        continuation::Kind::Core,
        fixed,
        memory,
        output,
        MAX_MESSAGE_BYTES,
    ))
}

fn task_read(
    mut caller: Caller<'_, State>,
    output: i32,
    capacity: i32,
) -> Result<i32, wasmi::Error> {
    if let Some(fault) = caller.data().cancel.fault() {
        return Err(trap(caller.data_mut(), fault));
    }
    if output < 0
        || capacity as usize != MAX_TASK_BYTES
        || caller.data().task.as_ref().is_none_or(|t| t.read)
    {
        return Err(trap(caller.data_mut(), Fault::TaskProtocol));
    }
    let memory = caller
        .get_export("memory")
        .and_then(|v| v.into_memory())
        .ok_or_else(|| wasmi::Error::new("missing memory"))?;
    if (output as usize)
        .checked_add(MAX_TASK_BYTES)
        .is_none_or(|end| end > memory.data(&caller).len())
    {
        return Err(trap(caller.data_mut(), Fault::Trap));
    }
    let input = caller.data().task.as_ref().unwrap().input.to_vec();
    memory
        .write(&mut caller, output as usize, &input)
        .map_err(|_| wasmi::Error::new("task input write"))?;
    caller.data_mut().task.as_mut().unwrap().read = true;
    Ok(input.len() as i32)
}
fn task_complete(
    mut caller: Caller<'_, State>,
    input: i32,
    length: i32,
) -> Result<i32, wasmi::Error> {
    if let Some(fault) = caller.data().cancel.fault() {
        return Err(trap(caller.data_mut(), fault));
    }
    if input < 0
        || length <= 0
        || length as usize > MAX_TASK_BYTES
        || caller
            .data()
            .task
            .as_ref()
            .is_none_or(|t| !t.read || t.completion.is_some())
    {
        return Err(trap(caller.data_mut(), Fault::TaskProtocol));
    }
    let memory = caller
        .get_export("memory")
        .and_then(|v| v.into_memory())
        .ok_or_else(|| wasmi::Error::new("missing memory"))?;
    let start = input as usize;
    let Some(end) = start.checked_add(length as usize) else {
        return Err(trap(caller.data_mut(), Fault::Trap));
    };
    if end > memory.data(&caller).len() {
        return Err(trap(caller.data_mut(), Fault::Trap));
    }
    let bytes = memory.data(&caller)[start..end].to_vec();
    caller.data_mut().task.as_mut().unwrap().completion = Some(bytes);
    Ok(0)
}

// Fixed byte exchange only. Authorization and dependency routing belong to the trusted callback.
fn dependency_call(
    mut caller: Caller<'_, State>,
    input: i32,
    length: i32,
    output: i32,
    capacity: i32,
) -> Result<i32, wasmi::Error> {
    if let Some(fault) = caller.data().cancel.fault() {
        return Err(trap(caller.data_mut(), fault));
    }
    if !caller.data().dependency
        || caller
            .data()
            .task
            .as_ref()
            .is_none_or(|t| !t.read || t.completion.is_some())
    {
        return Err(trap(caller.data_mut(), Fault::TaskProtocol));
    }
    if input < 0
        || output < 0
        || length <= 0
        || length as usize > MAX_TASK_BYTES
        || capacity as usize != MAX_TASK_BYTES
    {
        return Err(trap(caller.data_mut(), Fault::Trap));
    }
    let memory = caller
        .get_export("memory")
        .and_then(|v| v.into_memory())
        .ok_or_else(|| wasmi::Error::new("missing memory"))?;
    let input = input as usize;
    let output = output as usize;
    let Some(input_end) = input.checked_add(length as usize) else {
        return Err(trap(caller.data_mut(), Fault::Trap));
    };
    let Some(output_end) = output.checked_add(MAX_TASK_BYTES) else {
        return Err(trap(caller.data_mut(), Fault::Trap));
    };
    let memory_len = memory.data(&caller).len();
    if input_end > memory_len
        || output_end > memory_len
        || (input < output_end && output < input_end)
    {
        return Err(trap(caller.data_mut(), Fault::Trap));
    }
    if caller.data().calls >= caller.data().max_calls {
        return Err(trap(caller.data_mut(), Fault::Limits));
    }
    let fixed = memory.data(&caller)[input..input_end].to_vec();
    caller.data_mut().calls += 1;
    Err(continuation::suspend(
        caller.data_mut(),
        continuation::Kind::Dependency,
        fixed,
        memory,
        output,
        MAX_TASK_BYTES,
    ))
}

// Framed IO only. Authorization belongs to the trusted broker callback.
fn io_call(
    mut caller: Caller<'_, State>,
    input: i32,
    length: i32,
    output: i32,
    capacity: i32,
) -> Result<i32, wasmi::Error> {
    if let Some(fault) = caller.data().cancel.fault() {
        return Err(trap(caller.data_mut(), fault));
    }
    if !caller.data().io
        || caller
            .data()
            .task
            .as_ref()
            .is_none_or(|t| !t.read || t.completion.is_some())
    {
        return Err(trap(caller.data_mut(), Fault::TaskProtocol));
    }
    if input < 0
        || output < 0
        || length <= 0
        || length as usize > MAX_TASK_BYTES
        || capacity as usize != MAX_TASK_BYTES
    {
        return Err(trap(caller.data_mut(), Fault::Trap));
    }
    let memory = caller
        .get_export("memory")
        .and_then(|v| v.into_memory())
        .ok_or_else(|| wasmi::Error::new("missing memory"))?;
    let input = input as usize;
    let output = output as usize;
    let Some(input_end) = input.checked_add(length as usize) else {
        return Err(trap(caller.data_mut(), Fault::Trap));
    };
    let Some(output_end) = output.checked_add(MAX_TASK_BYTES) else {
        return Err(trap(caller.data_mut(), Fault::Trap));
    };
    let memory_len = memory.data(&caller).len();
    if input_end > memory_len
        || output_end > memory_len
        || (input < output_end && output < input_end)
    {
        return Err(trap(caller.data_mut(), Fault::Trap));
    }
    if caller.data().calls >= caller.data().max_calls {
        return Err(trap(caller.data_mut(), Fault::Limits));
    }
    let fixed = memory.data(&caller)[input..input_end].to_vec();
    caller.data_mut().calls += 1;
    Err(continuation::suspend(
        caller.data_mut(),
        continuation::Kind::Io,
        fixed,
        memory,
        output,
        MAX_TASK_BYTES,
    ))
}
