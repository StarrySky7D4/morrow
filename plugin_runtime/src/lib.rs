#![forbid(unsafe_code)]
//! Replaceable synchronous Wasm backend probe. No WASI, filesystem or identity imports.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use wasmi::{
    Caller, Config, EnforcedLimits, Engine, ExternType, Linker, Module, Store, StoreLimits,
    StoreLimitsBuilder, ValType,
};
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
pub mod package;
pub const MAX_MESSAGE_BYTES: usize = 65536;
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
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    PackageBinding,
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
/// The closure is trusted and bound to an actual host connection. It must not panic,
/// retain input, re-enter the guest, or return unbounded responses. Calls may commit;
/// a later trap/cancellation never implies rollback. Fuel cannot interrupt this closure.
type Exchange<'a> = &'a mut dyn FnMut(&[u8]) -> Result<Vec<u8>, ()>;
struct State<'a> {
    exchange: Exchange<'a>,
    limits: StoreLimits,
    cancel: Cancellation,
    calls: u32,
    max_calls: u32,
    stopped: Option<Fault>,
}
pub struct Runner {
    engine: Engine,
    module: Module,
    limits: Limits,
}
impl Runner {
    pub fn new(bytes: &[u8], limits: Limits) -> Result<Self, Fault> {
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
        let mut imports = 0;
        for import in module.imports() {
            imports += 1;
            let ExternType::Func(ty) = import.ty() else {
                return Err(Fault::UnsupportedAbi);
            };
            if imports > 1
                || import.module() != "morrow_v1"
                || import.name() != "exchange"
                || ty.params() != [ValType::I32; 4]
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
            engine,
            module,
            limits,
        })
    }
    /// Each call owns a fresh Wasm Store and releases its memory after termination.
    /// Cancellation gates imports and checks before/after execution; a pure loop is
    /// bounded by fuel, not immediately interrupted by the cancellation flag.
    pub fn run(&self, exchange: Exchange<'_>, cancel: Cancellation) -> Report {
        if cancel.cancelled() {
            return Report {
                outcome: Err(Fault::Cancelled),
                host_calls: 0,
                fuel_remaining: self.limits.fuel,
            };
        }
        let state = State {
            exchange,
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
        } else if store.data().cancel.cancelled() {
            Err(Fault::Cancelled)
        } else {
            outcome
        };
        Report {
            outcome,
            host_calls: store.data().calls,
            fuel_remaining: store.get_fuel().unwrap_or(0),
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
    if caller.data().cancel.cancelled() {
        return Err(trap(caller.data_mut(), Fault::Cancelled));
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
    if caller.data().cancel.cancelled() {
        return Err(trap(caller.data_mut(), Fault::Cancelled));
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
