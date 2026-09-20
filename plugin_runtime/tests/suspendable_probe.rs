//! S0 feasibility probe only, not a plugin ABI or production scheduler.
//! Wasmi retains the guest stack; its data owns bytes, never HostRuntime borrows.
#![deny(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use wasmi::{
    Caller, Config, Engine, Error, Global, Linker, Memory, Module, Store, StoreLimits,
    StoreLimitsBuilder, TypedResumableCall, TypedResumableCallHostTrap, Val,
};

const GUEST: &str = include_str!("fixtures/suspendable_probe.wat");
const MAX_BYTES: usize = 64;
const YIELD: i32 = 7419;
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Token {
    session: u64,
    sequence: u32,
}
#[derive(Debug)]
struct Request {
    token: Token,
    bytes: Vec<u8>,
    output: usize,
    capacity: usize,
}
struct Data {
    limits: StoreLimits,
    session: u64,
    calls: u32,
    max_calls: u32,
    request: Option<Request>,
}

fn exchange(
    mut caller: Caller<'_, Data>,
    ptr: i32,
    len: i32,
    out: i32,
    capacity: i32,
) -> Result<i32, Error> {
    let invalid = || Error::new("invalid probe request");
    let ptr = usize::try_from(ptr).map_err(|_| invalid())?;
    let len = usize::try_from(len).map_err(|_| invalid())?;
    let output = usize::try_from(out).map_err(|_| invalid())?;
    let capacity = usize::try_from(capacity).map_err(|_| invalid())?;
    if !(1..=MAX_BYTES).contains(&len)
        || !(1..=MAX_BYTES).contains(&capacity)
        || caller.data().request.is_some()
        || caller.data().calls >= caller.data().max_calls
    {
        return Err(invalid());
    }
    let memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(invalid)?;
    let size = memory.data(&caller).len();
    if ptr.checked_add(len).is_none_or(|end| end > size)
        || output.checked_add(capacity).is_none_or(|end| end > size)
    {
        return Err(invalid());
    }
    let mut bytes = vec![0; len];
    memory
        .read(&caller, ptr, &mut bytes)
        .map_err(|_| invalid())?;
    let data = caller.data_mut();
    data.calls += 1;
    data.request = Some(Request {
        token: Token {
            session: data.session,
            sequence: data.calls,
        },
        bytes,
        output,
        capacity,
    });
    // Only this validated import may produce a resumable pending request.
    Err(Error::i32_exit(YIELD))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    Module,
    Trap,
    Fuel,
    WrongToken,
    ResponseBound,
    Terminal,
}
struct Probe {
    store: Store<Data>,
    memory: Memory,
    entries: Option<Global>,
    continuation: Option<TypedResumableCallHostTrap<i32>>,
    live: bool,
    result: Option<i32>,
}
impl Probe {
    fn start(wat: &str, fuel: u64, max_calls: u32) -> Result<Self, Failure> {
        if fuel == 0 || fuel > 100_000 || max_calls > 4 {
            return Err(Failure::Module);
        }
        let bytes = wat::parse_str(wat).map_err(|_| Failure::Module)?;
        for item in wasmparser::Parser::new(0).parse_all(&bytes) {
            if matches!(
                item.map_err(|_| Failure::Module)?,
                wasmparser::Payload::StartSection { .. }
            ) {
                return Err(Failure::Module);
            }
        }
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, &bytes).map_err(|_| Failure::Module)?;
        let mut store = Store::new(
            &engine,
            Data {
                limits: StoreLimitsBuilder::new()
                    .memory_size(65536)
                    .memories(1)
                    .instances(1)
                    .tables(0)
                    .trap_on_grow_failure(true)
                    .build(),
                session: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
                calls: 0,
                max_calls,
                request: None,
            },
        );
        store.limiter(|s| &mut s.limits);
        store.set_fuel(fuel).map_err(|_| Failure::Fuel)?;
        let mut linker = Linker::new(&engine);
        linker
            .func_wrap("morrow_probe_v0", "exchange", exchange)
            .map_err(|_| Failure::Module)?;
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(|_| Failure::Module)?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or(Failure::Module)?;
        let entries = instance.get_global(&store, "entries");
        let run = instance
            .get_typed_func::<(), i32>(&store, "run")
            .map_err(|_| Failure::Module)?;
        let step = run
            .call_resumable(&mut store, ())
            .map_err(|_| Failure::Trap)?;
        let mut probe = Self {
            store,
            memory,
            entries,
            continuation: None,
            live: true,
            result: None,
        };
        probe.accept(step)?;
        Ok(probe)
    }
    fn accept(&mut self, step: TypedResumableCall<i32>) -> Result<(), Failure> {
        match step {
            TypedResumableCall::HostTrap(trap)
                if trap.host_error().i32_exit_status() == Some(YIELD)
                    && self.store.data().request.is_some() =>
            {
                self.continuation = Some(trap);
                Ok(())
            }
            TypedResumableCall::Finished(value) => {
                self.live = false;
                self.result = Some(value);
                Ok(())
            }
            TypedResumableCall::OutOfFuel(_) => {
                self.cancel();
                Err(Failure::Fuel)
            }
            _ => {
                self.cancel();
                Err(Failure::Trap)
            }
        }
    }
    fn request(&self) -> &Request {
        self.store
            .data()
            .request
            .as_ref()
            .expect("probe is suspended")
    }
    fn fuel(&self) -> u64 {
        self.store.get_fuel().unwrap()
    }
    fn entries(&self) -> i32 {
        self.entries.unwrap().get(&self.store).i32().unwrap()
    }
    fn cancel(&mut self) {
        self.live = false;
        self.continuation = None;
        self.store.data_mut().request = None;
    }
    fn resume(&mut self, token: Token, response: &[u8]) -> Result<Option<i32>, Failure> {
        if !self.live {
            return Err(Failure::Terminal);
        }
        let request = self.request();
        if token != request.token {
            return Err(Failure::WrongToken);
        }
        if response.len() > MAX_BYTES || response.len() > request.capacity {
            return Err(Failure::ResponseBound);
        }
        let request = self.store.data_mut().request.take().unwrap();
        let continuation = self.continuation.take().unwrap();
        if self
            .memory
            .write(&mut self.store, request.output, response)
            .is_err()
        {
            self.cancel();
            return Err(Failure::Trap);
        }
        match continuation.resume(&mut self.store, &[Val::I32(response.len() as i32)]) {
            Ok(step) => self.accept(step)?,
            Err(_) => {
                self.cancel();
                return Err(Failure::Trap);
            }
        }
        Ok(self.result)
    }
}

#[test]
fn same_stack_and_memory_resume_twice_without_restarting_entry() {
    let mut p = Probe::start(GUEST, 10_000, 2).unwrap();
    let fuel = p.fuel();
    assert!(fuel < 10_000);
    let first = p.request().token;
    assert_eq!(p.request().bytes, b"first");
    assert_eq!(p.resume(first, &7i32.to_le_bytes()), Ok(None));
    // Wasmi charges basic blocks in batches; resuming inside a prepaid block
    // need not deduct again. Fuel must never increase or reset on resumption.
    assert!(p.fuel() <= fuel);
    let fuel = p.fuel();
    let second = p.request().token;
    assert_ne!(first, second);
    assert_eq!(p.request().bytes, b"second");
    assert_eq!(p.resume(second, &13i32.to_le_bytes()), Ok(Some(61)));
    assert!(p.fuel() <= fuel);
    assert_eq!(p.entries(), 1);
    assert_eq!(p.store.data().calls, 2);
    assert_eq!(p.resume(second, &[0; 4]), Err(Failure::Terminal));
}

#[test]
fn foreign_stale_and_oversize_replies_do_not_consume_or_refuel_pending_call() {
    let mut p = Probe::start(GUEST, 10_000, 2).unwrap();
    let other = Probe::start(GUEST, 10_000, 2).unwrap();
    let first = p.request().token;
    let fuel = p.fuel();
    assert_eq!(
        p.resume(other.request().token, &[0; 4]),
        Err(Failure::WrongToken)
    );
    assert_eq!(p.resume(first, &[0; 33]), Err(Failure::ResponseBound));
    assert_eq!(p.fuel(), fuel);
    assert_eq!(p.request().token, first);
    p.resume(first, &7i32.to_le_bytes()).unwrap();
    let second = p.request().token;
    assert_eq!(p.resume(first, &[0; 4]), Err(Failure::WrongToken));
    assert_eq!(p.resume(second, &13i32.to_le_bytes()), Ok(Some(61)));
}

#[test]
fn cancellation_discards_continuation_and_rejects_late_completion() {
    let mut p = Probe::start(GUEST, 10_000, 2).unwrap();
    let token = p.request().token;
    p.cancel();
    assert_eq!(p.resume(token, &[0; 4]), Err(Failure::Terminal));
    assert!(p.store.data().request.is_none());
    assert!(p.continuation.is_none());
    assert_eq!(p.entries(), 1);
    assert_eq!(p.store.data().calls, 1);
}

#[test]
fn fuel_and_import_budgets_are_cumulative_across_suspensions() {
    let mut p = Probe::start(GUEST, 10_000, 1).unwrap();
    let token = p.request().token;
    assert_eq!(p.resume(token, &[0; 4]), Err(Failure::Trap));
    assert_eq!(p.store.data().calls, 1);
    assert!(!p.live);
    let burning = GUEST.replace(
        "local.get $seed local.get $first i32.add",
        "(loop $burn br $burn) local.get $seed local.get $first i32.add",
    );
    let mut p = Probe::start(&burning, 1000, 2).unwrap();
    p.resume(p.request().token, &[0; 4]).unwrap();
    let token = p.request().token;
    assert_eq!(p.resume(token, &[0; 4]), Err(Failure::Fuel));
    assert_eq!(p.resume(token, &[0; 4]), Err(Failure::Terminal));
}

#[test]
fn malformed_memory_ranges_and_plain_traps_are_not_yields() {
    for args in [
        "-1 5 128 32",
        "65535 5 128 32",
        "0 65 128 32",
        "0 5 65535 32",
        "0 5 128 -1",
    ] {
        let args = args
            .split_whitespace()
            .map(|n| format!("i32.const {n}"))
            .collect::<Vec<_>>()
            .join(" ");
        let wat = format!(
            r#"(module (import "morrow_probe_v0" "exchange" (func $io (param i32 i32 i32 i32) (result i32))) (memory (export "memory") 1) (func (export "run") (result i32) {args} call $io))"#
        );
        assert!(matches!(Probe::start(&wat, 1000, 2), Err(Failure::Trap)));
    }
    assert!(matches!(
        Probe::start(
            r#"(module (memory (export "memory") 1) (func (export "run") (result i32) unreachable))"#,
            1000,
            2
        ),
        Err(Failure::Trap)
    ));
    assert!(matches!(
        Probe::start(
            r#"(module (memory (export "memory") 1) (func $init) (start $init) (func (export "run") (result i32) i32.const 0))"#,
            1000,
            2
        ),
        Err(Failure::Module)
    ));
}

#[test]
fn real_owner_commits_during_delayed_transport_without_a_second_store() {
    use morrow_core::{content::CardRecord, dispatch::HostRuntime, store::Store as ContentStore};
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::mpsc,
        thread,
        time::Duration,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("owner.db");
    let mut owner =
        HostRuntime::new(ContentStore::open(&path, Default::default()).unwrap()).unwrap();
    let identity = owner.binding();
    let mut p = Probe::start(GUEST, 10_000, 2).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    // Establish both endpoints before spawning so failure cannot strand an
    // accept thread. All subsequent waits have explicit finite timeouts.
    let client_socket = TcpStream::connect_timeout(&address, Duration::from_secs(5)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let (server_socket, _) = listener.accept().unwrap();
    server_socket.set_nonblocking(false).unwrap();
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut socket = server_socket;
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 5];
        socket.read_exact(&mut request).unwrap();
        assert_eq!(&request, b"first");
        entered_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        socket.write_all(&7i32.to_le_bytes()).unwrap();
    });
    // The actual waiting thread receives owned bytes only, no runtime/Store.
    let request = p.request().bytes.clone();
    let client = thread::spawn(move || {
        let mut socket = client_socket;
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket.write_all(&request).unwrap();
        let mut reply = [0; 4];
        socket.read_exact(&mut reply).unwrap();
        reply
    });
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let card = CardRecord::new(
        "while-paused",
        "probe.content",
        1,
        "saved while waiting",
        vec![1, 2, 3],
    )
    .unwrap();
    owner
        .store_local_mut()
        .create_local("create-while-paused", &card)
        .unwrap();
    assert_eq!(
        owner
            .store_local()
            .card("while-paused")
            .unwrap()
            .unwrap()
            .body(),
        [1, 2, 3]
    );
    assert_eq!(owner.binding(), identity);
    assert!(!client.is_finished());
    release.send(()).unwrap();
    let reply = client.join().unwrap();
    server.join().unwrap();
    p.resume(p.request().token, &reply).unwrap();
    assert_eq!(
        p.resume(p.request().token, &13i32.to_le_bytes()),
        Ok(Some(61))
    );
    assert_eq!(p.entries(), 1);
    assert_eq!(owner.binding(), identity);
    drop((p, owner));
    let reopened = ContentStore::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        reopened.card("while-paused").unwrap().unwrap().body(),
        [1, 2, 3]
    );
}
