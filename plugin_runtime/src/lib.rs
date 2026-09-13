#![deny(unsafe_code)]
//! Replaceable synchronous Wasm backend probe. No WASI, filesystem or identity imports.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;
use wasmi::{
    Caller, Config, EnforcedLimits, Engine, ExternType, Linker, Module, Store, StoreLimits,
    StoreLimitsBuilder, ValType,
};
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod dependency;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod dynamic_dependencies;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod inline_ui;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod manager;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod instance_pool;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod replay;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod package;
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod proposal;
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
struct TaskState<'a> {
    input: &'a [u8],
    read: bool,
    completion: Option<Vec<u8>>,
}
/// The closure is trusted and bound to an actual host connection. It must not panic,
/// retain input, re-enter the guest, or return unbounded responses. Calls may commit;
/// a later trap/cancellation never implies rollback. Fuel cannot interrupt this closure.
type Exchange<'a> = &'a mut dyn FnMut(&[u8]) -> Result<Vec<u8>, ()>;
struct State<'a> {
    task: Option<TaskState<'a>>,
    exchange: Exchange<'a>,
    dependency: Option<Exchange<'a>>,
    limits: StoreLimits,
    cancel: Cancellation,
    calls: u32,
    max_calls: u32,
    stopped: Option<Fault>,
}
pub struct Runner {
    task_abi: bool,
    dependency_abi: bool,
    engine: Engine,
    module: Module,
    limits: Limits,
}
impl Runner {
    pub fn new(bytes: &[u8], limits: Limits) -> Result<Self, Fault> {
        Self::prepare(bytes, limits, false, false)
    }
    pub fn new_task(bytes: &[u8], limits: Limits) -> Result<Self, Fault> {
        Self::prepare(bytes, limits, true, false)
    }
    /// Task ABI with one additional fixed dependency import. The callback is host-routed;
    /// it must not re-enter this guest and cannot be supplied to ordinary task runners.
    pub fn new_dependency_task(bytes: &[u8], limits: Limits) -> Result<Self, Fault> {
        Self::prepare(bytes, limits, true, true)
    }
    fn prepare(
        bytes: &[u8],
        limits: Limits,
        task_abi: bool,
        dependency_abi: bool,
    ) -> Result<Self, Fault> {
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
            engine,
            module,
            limits,
        })
    }
    /// Each call owns a fresh Wasm Store and releases its memory after termination.
    /// Cancellation gates imports and checks before/after execution; a pure loop is
    /// bounded by fuel, not immediately interrupted by the cancellation flag.
    pub fn run(&self, exchange: Exchange<'_>, cancel: Cancellation) -> Report {
        self.execute(exchange, None, cancel, None).report
    }
    pub fn run_task<'a>(
        &self,
        input: &'a [u8],
        exchange: Exchange<'a>,
        cancel: Cancellation,
    ) -> TaskRun {
        self.execute(exchange, None, cancel, Some(input))
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
        self.execute(exchange, Some(dependency), cancel, Some(input))
    }
    fn execute<'a>(
        &self,
        exchange: Exchange<'a>,
        dependency: Option<Exchange<'a>>,
        cancel: Cancellation,
        input: Option<&'a [u8]>,
    ) -> TaskRun {
        let fault =
            if self.task_abi != input.is_some() || self.dependency_abi != dependency.is_some() {
                Some(Fault::UnsupportedAbi)
            } else if input.is_some_and(|b| b.is_empty() || b.len() > MAX_TASK_BYTES) {
                Some(Fault::Limits)
            } else {
                cancel.fault()
            };
        if let Some(fault) = fault {
            return TaskRun {
                report: Report {
                    outcome: Err(fault),
                    host_calls: 0,
                    fuel_remaining: self.limits.fuel,
                },
                completion: None,
            };
        }
        let state = State {
            task: input.map(|input| TaskState {
                input,
                read: false,
                completion: None,
            }),
            exchange,
            dependency,
            limits: StoreLimitsBuilder::new()
                .memory_size(self.limits.memory_bytes)
                .memories(1)
                .tables(1)
                .table_elements(4096)
                .instances(1)
                .trap_on_grow_failure(true)
                .build(),
            cancel,
            calls: 0,
            max_calls: self.limits.host_calls,
            stopped: None,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(self.limits.fuel).expect("fuel enabled");
        let mut linker = Linker::new(&self.engine);
        linker
            .func_wrap("morrow_v1", "exchange", host_exchange)
            .expect("one fixed import");
        if self.task_abi {
            linker
                .func_wrap("morrow_task_v1", "read_input", task_read)
                .expect("task input import");
            linker
                .func_wrap("morrow_task_v1", "complete", task_complete)
                .expect("task result import");
        }
        if self.dependency_abi {
            linker
                .func_wrap("morrow_dependency_v1", "call", dependency_call)
                .expect("dependency call import");
        }
        let outcome = (|| {
            let instance = linker
                .instantiate_and_start(&mut store, &self.module)
                .map_err(|_| Fault::Limits)?;
            let func = instance
                .get_typed_func::<(), i32>(&store, "morrow_run")
                .map_err(|_| Fault::UnsupportedAbi)?;
            func.call(&mut store, ()).map_err(|e| {
                if e.as_trap_code() == Some(wasmi::TrapCode::OutOfFuel) {
                    Fault::Limits
                } else {
                    Fault::Trap
                }
            })
        })();
        let outcome = if let Some(f) = &store.data().stopped {
            Err(f.clone())
        } else if let Some(fault) = store.data().cancel.fault() {
            Err(fault)
        } else {
            outcome
        };
        let outcome = if self.task_abi
            && outcome.is_ok()
            && (outcome != Ok(0)
                || store
                    .data()
                    .task
                    .as_ref()
                    .is_none_or(|t| t.completion.is_none()))
        {
            Err(Fault::TaskProtocol)
        } else {
            outcome
        };
        let completion = if outcome.is_ok() {
            store
                .data_mut()
                .task
                .as_mut()
                .and_then(|t| t.completion.take())
        } else {
            None
        };
        TaskRun {
            report: Report {
                outcome,
                host_calls: store.data().calls,
                fuel_remaining: store.get_fuel().unwrap_or(0),
            },
            completion,
        }
    }
}
fn trap(state: &mut State<'_>, fault: Fault) -> wasmi::Error {
    state.stopped = Some(fault);
    wasmi::Error::new("guest boundary stopped")
}
fn host_exchange(
    mut caller: Caller<'_, State<'_>>,
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
    let response = (caller.data_mut().exchange)(&fixed);
    if let Some(fault) = caller.data().cancel.fault() {
        return Err(trap(caller.data_mut(), fault));
    }
    let response = match response {
        Ok(v) => v,
        Err(()) => return Ok(-1),
    };
    if response.is_empty() || response.len() > MAX_MESSAGE_BYTES {
        return Err(trap(caller.data_mut(), Fault::Trap));
    }
    // Reacquire memory after the host call; never retain a borrowed memory view.
    memory
        .write(&mut caller, output, &response)
        .map_err(|_| wasmi::Error::new("response write failed"))?;
    Ok(response.len() as i32)
}

fn task_read(
    mut caller: Caller<'_, State<'_>>,
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
    mut caller: Caller<'_, State<'_>>,
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
    mut caller: Caller<'_, State<'_>>,
    input: i32,
    length: i32,
    output: i32,
    capacity: i32,
) -> Result<i32, wasmi::Error> {
    if let Some(fault) = caller.data().cancel.fault() {
        return Err(trap(caller.data_mut(), fault));
    }
    if caller.data().dependency.is_none()
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
    let response = (caller
        .data_mut()
        .dependency
        .as_mut()
        .expect("checked callback"))(&fixed);
    if let Some(fault) = caller.data().cancel.fault() {
        return Err(trap(caller.data_mut(), fault));
    }
    let response = match response {
        Ok(bytes) if !bytes.is_empty() && bytes.len() <= MAX_TASK_BYTES => bytes,
        _ => return Err(trap(caller.data_mut(), Fault::TaskProtocol)),
    };
    // No memory view is held across the callback. Reacquire it for the bounded response write.
    memory
        .write(&mut caller, output, &response)
        .map_err(|_| wasmi::Error::new("dependency response write failed"))?;
    Ok(response.len() as i32)
}
