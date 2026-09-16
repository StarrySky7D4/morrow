#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
//! Actual managed IO packages: no synthetic identity or guest-created approval.
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Limits,
    io_binding::IoBinding,
    manager::{ManagedInstance, Manager},
};
use std::collections::BTreeSet;

const ID: &str = "org.example.managed.file-io";
const HANDLER: &str = "file.read-selected";

fn capabilities() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileRead])
}

fn guest() -> Vec<u8> {
    wat::parse_str(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
        (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 4)
        (func (export "morrow_run") (result i32)
          (local $n i32)
          i32.const 0 i32.const 131072 call $read local.set $n
          i32.const 0 local.get $n i32.const 131072 i32.const 131072 call $io local.set $n
          i32.const 131072 local.get $n call $done drop
          i32.const 0))"#,
    )
    .unwrap()
}

struct Fixture {
    _dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    package: Package,
}
impl Fixture {
    fn new(approve: bool, resources: u32, bytes: u64) -> Self {
        Self::with_capabilities(approve, resources, bytes, capabilities())
    }
    fn with_capabilities(
        approve: bool,
        resources: u32,
        bytes: u64,
        declared: BTreeSet<IoCapability>,
    ) -> Self {
        Self::with_module(approve, resources, bytes, declared, guest())
    }
    fn with_module(
        approve: bool,
        resources: u32,
        bytes: u64,
        declared: BTreeSet<IoCapability>,
        wasm: Vec<u8>,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut declaration =
            io::declaration(declared.iter().copied().collect(), vec![HANDLER.into()]);
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_resources = resources;
        budget.max_jobs = 4;
        budget.max_bytes = bytes;
        budget.max_job_bytes = bytes.min(io::MAX_JOB_BYTES);
        budget.max_duration_ms = 100;
        manifest.io_declaration = Some(declaration);
        manifest.required_features.push(io::FEATURE.into());
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        if approve {
            manager
                .approve_io(ID, package.digest(), declared, manager.revision())
                .unwrap();
        }
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        Self {
            _dir: dir,
            manager,
            host,
            package,
        }
    }
    fn connect(&mut self) -> ManagedInstance {
        self.manager.connect(ID, &mut self.host).unwrap()
    }
    fn bind(&self, instance: &ManagedInstance, expires: u64, now: u64) -> IoBinding {
        self.manager
            .bind_io(
                &self.host,
                instance,
                self.package.digest(),
                self.manager.revision(),
                &capabilities(),
                expires,
                now,
            )
            .unwrap()
    }
}

use morrow_core::{
    io::{Request, Response, Status},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    file_io::FileBroker, instance_pool::Pool, io_binding::Error as AdmissionError,
};

fn exchange(
    f: &Fixture,
    broker: &mut FileBroker,
    instance: &ManagedInstance,
    binding: &IoBinding,
    request: &Request,
    now: u64,
) -> Response {
    let raw = broker
        .exchange(&f.manager, &f.host, instance, binding, request, now)
        .unwrap();
    Response::decode(request, &raw).unwrap()
}

#[test]
fn actual_guest_reads_multiple_chunks_and_finish_releases_only_held_capacity() {
    let mut f = Fixture::new(true, 2, 2_000_000);
    let instance = f.connect();
    let binding = f.bind(&instance, 90, 1);
    let mut broker = FileBroker::new([9; 32]);
    let original = (0..90_007).map(|i| (i % 251) as u8).collect::<Vec<_>>();
    let mut selected = original.clone();
    let token = broker
        .grant_file(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            selected.clone(),
            2,
        )
        .unwrap();
    selected.fill(0);
    assert_eq!(broker.usage(), (1, original.len() as u64));
    let mut all = Vec::new();
    for (call, offset) in [(1, 0), (2, 65_536)] {
        let request = Request::encode_read(call, &token, offset, 0).unwrap();
        let report = broker.run(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            HANDLER,
            &request,
            || 3,
        );
        assert!(
            report.execution.outcome.is_ok(),
            "{:?}",
            report.execution.outcome
        );
        let response = report.response.unwrap();
        assert_eq!(response.status, Status::Completed);
        assert_eq!(response.offset, offset);
        assert_eq!(response.eof, offset != 0);
        all.extend_from_slice(&response.payload);
    }
    assert_eq!(all, original);
    let spent = binding.usage().bytes;
    assert!(spent > original.len() as u64 * 2);
    let finish = Request::encode_finish(3, &token).unwrap();
    assert_eq!(
        exchange(&f, &mut broker, &instance, &binding, &finish, 4).status,
        Status::Completed
    );
    assert_eq!(broker.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    assert!(binding.usage().bytes > spent);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn independent_io_handler_is_required_and_pure_task_entry_stays_closed() {
    let mut f = Fixture::new(true, 2, 1_000_000);
    let instance = f.connect();
    let binding = f.bind(&instance, 90, 1);
    let mut broker = FileBroker::new([4; 32]);
    let token = broker
        .grant_file(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            b"secret".to_vec(),
            2,
        )
        .unwrap();
    let request = Request::encode_read(1, &token, 0, 6).unwrap();
    let before = binding.usage();
    let wrong = broker.run(
        &f.manager,
        &f.host,
        &instance,
        &binding,
        "not.declared",
        &request,
        || 3,
    );
    assert!(wrong.execution.outcome.is_err());
    assert!(wrong.response.is_none());
    assert_eq!(binding.usage(), before);
    let invocation = Invocation::new_transform(
        "ordinary-task",
        Transform {
            handler: HANDLER.into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: vec![1],
        },
    )
    .unwrap();
    assert!(
        instance
            .run_task(&mut f.host, &invocation, || 3)
            .execution
            .outcome
            .is_err()
    );
    assert!(instance.run(&mut f.host, || 3).outcome.is_err());
    let valid = broker.run(
        &f.manager,
        &f.host,
        &instance,
        &binding,
        HANDLER,
        &request,
        || 4,
    );
    assert!(valid.execution.outcome.is_ok());
    assert_eq!(valid.response.unwrap().payload, b"secret");
}

#[test]
fn declaration_without_approval_cannot_bind_and_other_instance_cannot_read() {
    let mut denied = Fixture::new(false, 2, 1_000_000);
    let instance = denied.connect();
    assert!(
        denied
            .manager
            .bind_io(
                &denied.host,
                &instance,
                denied.package.digest(),
                denied.manager.revision(),
                &capabilities(),
                90,
                1
            )
            .is_err()
    );
    let mut f = Fixture::new(true, 2, 1_000_000);
    let first = f.connect();
    let second = f.connect();
    let binding = f.bind(&first, 90, 1);
    let other = f.bind(&second, 90, 1);
    let mut broker = FileBroker::new([5; 32]);
    let token = broker
        .grant_file(
            &f.manager,
            &f.host,
            &first,
            &binding,
            b"private".to_vec(),
            2,
        )
        .unwrap();
    let request = Request::encode_read(1, &token, 0, 7).unwrap();
    let rejected = exchange(&f, &mut broker, &second, &other, &request, 3);
    assert_ne!(rejected.status, Status::Completed);
    assert!(rejected.payload.is_empty());
    let owner = exchange(&f, &mut broker, &first, &binding, &request, 4);
    assert_eq!(owner.payload, b"private");
    assert_eq!(other.usage().bytes, 0);
}

#[test]
fn approval_change_revokes_saved_binding_and_reaps_fixed_bytes() {
    let mut f = Fixture::new(true, 2, 1_000_000);
    let instance = f.connect();
    let binding = f.bind(&instance, 90, 1);
    let mut broker = FileBroker::new([6; 32]);
    let token = broker
        .grant_file(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            b"secret".to_vec(),
            2,
        )
        .unwrap();
    f.manager
        .approve_io(
            ID,
            f.package.digest(),
            BTreeSet::new(),
            f.manager.revision(),
        )
        .unwrap();
    let request = Request::encode_read(1, &token, 0, 6).unwrap();
    let rejected = exchange(&f, &mut broker, &instance, &binding, &request, 3);
    assert_ne!(rejected.status, Status::Completed);
    assert!(rejected.payload.is_empty());
    broker.reap(3);
    assert_eq!(broker.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
}

#[test]
fn expired_resources_reclaim_slots_but_do_not_refund_cumulative_bytes() {
    let mut f = Fixture::new(true, 2, 1_000_000);
    let instance = f.connect();
    let old = f.bind(&instance, 10, 1);
    let mut broker = FileBroker::new([7; 32]);
    for _ in 0..2 {
        broker
            .grant_file(&f.manager, &f.host, &instance, &old, vec![1; 100], 2)
            .unwrap();
    }
    assert_eq!(old.usage().resources, 2);
    broker.reap(10);
    assert_eq!(broker.usage(), (0, 0));
    let fresh = f.bind(&instance, 90, 11);
    assert_eq!(fresh.usage().resources, 0);
    assert_eq!(fresh.usage().bytes, 200);
    broker
        .grant_file(&f.manager, &f.host, &instance, &fresh, vec![2; 100], 12)
        .unwrap();
    assert_eq!(fresh.usage().bytes, 300);
}

#[test]
fn repeated_bindings_and_brokers_share_resource_limit_and_read_byte_charges() {
    let mut f = Fixture::new(true, 1, 4_000);
    let instance = f.connect();
    let a = f.bind(&instance, 90, 1);
    let b = f.bind(&instance, 90, 1);
    let mut first = FileBroker::new([8; 32]);
    let mut second = FileBroker::new([9; 32]);
    let token = first
        .grant_file(&f.manager, &f.host, &instance, &a, vec![42; 1_000], 2)
        .unwrap();
    assert!(matches!(
        second.grant_file(&f.manager, &f.host, &instance, &b, vec![0; 1], 3),
        Err(AdmissionError::Limit)
    ));
    assert_eq!(b.usage().bytes, 1_000);
    let request = Request::encode_read(1, &token, 0, 1_000).unwrap();
    let before = a.usage().bytes;
    let response = exchange(&f, &mut first, &instance, &a, &request, 4);
    assert_eq!(response.status, Status::Completed);
    let once = a.usage().bytes;
    assert!(once > before + 1_000);
    let repeated = exchange(&f, &mut first, &instance, &b, &request, 5);
    assert_eq!(repeated.status, Status::Completed);
    assert_eq!(b.usage().bytes - once, once - before);
    let over = exchange(&f, &mut first, &instance, &b, &request, 6);
    assert_eq!(over.status, Status::Quota);
    assert!(over.payload.is_empty());
    assert_eq!(a.usage().bytes, b.usage().bytes);
}

#[test]
fn pool_close_invalidates_old_binding_and_reference() {
    let mut f = Fixture::new(true, 2, 1_000_000);
    let mut pool = Pool::new(&f.host, Default::default()).unwrap();
    let revision = f.manager.revision();
    let session = pool
        .start(&mut f.manager, &mut f.host, ID, &[], revision)
        .unwrap();
    let binding = pool
        .bind_root_io(
            &f.manager,
            &f.host,
            &session,
            f.package.digest(),
            revision,
            &capabilities(),
            90,
            1,
        )
        .unwrap();
    let mut broker = FileBroker::new([10; 32]);
    let token = broker
        .grant_file(
            &f.manager,
            &f.host,
            pool.root(&session).unwrap(),
            &binding,
            b"pool".to_vec(),
            2,
        )
        .unwrap();
    let request = Request::encode_read(1, &token, 0, 4).unwrap();
    let result = broker.run(
        &f.manager,
        &f.host,
        pool.root(&session).unwrap(),
        &binding,
        HANDLER,
        &request,
        || 3,
    );
    assert_eq!(result.response.unwrap().payload, b"pool");
    pool.close(&mut f.host, &session).unwrap();
    broker.reap(4);
    assert_eq!(broker.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
    let replacement = f.connect();
    let new_binding = f.bind(&replacement, 90, 5);
    assert!(binding.check(&f.manager, &f.host, &replacement, 5).is_err());
    let rejected = exchange(&f, &mut broker, &replacement, &new_binding, &request, 6);
    assert_ne!(rejected.status, Status::Completed);
    assert!(rejected.payload.is_empty());
}

#[test]
fn cancel_releases_reference_without_refunding_bytes_or_revoking_other_file() {
    let mut f = Fixture::new(true, 2, 1_000_000);
    let instance = f.connect();
    let binding = f.bind(&instance, 90, 1);
    let mut broker = FileBroker::new([11; 32]);
    let first = broker
        .grant_file(&f.manager, &f.host, &instance, &binding, vec![1; 20], 2)
        .unwrap();
    let second = broker
        .grant_file(&f.manager, &f.host, &instance, &binding, vec![2; 30], 2)
        .unwrap();
    let before = binding.usage().bytes;
    let cancel = Request::encode_cancel(1, &first).unwrap();
    let closed = exchange(&f, &mut broker, &instance, &binding, &cancel, 3);
    assert!(matches!(
        closed.status,
        Status::Completed | Status::Cancelled
    ));
    assert_eq!(broker.usage(), (1, 30));
    assert_eq!(binding.usage().resources, 1);
    assert!(binding.usage().bytes > before);
    let request = Request::encode_read(2, &first, 0, 20).unwrap();
    let rejected = exchange(&f, &mut broker, &instance, &binding, &request, 4);
    assert_ne!(rejected.status, Status::Completed);
    assert!(rejected.payload.is_empty());
    let other = Request::encode_read(3, &second, 0, 30).unwrap();
    assert_eq!(
        exchange(&f, &mut broker, &instance, &binding, &other, 5).payload,
        vec![2; 30]
    );
}

#[test]
fn actual_guest_late_result_is_rejected_after_expiry_or_instance_stop() {
    for stop in [false, true] {
        let mut f = Fixture::new(true, 2, 1_000_000);
        let instance = f.connect();
        let binding = f.bind(&instance, 10, 1);
        let mut broker = FileBroker::new([12; 32]);
        let token = broker
            .grant_file(
                &f.manager,
                &f.host,
                &instance,
                &binding,
                b"late".to_vec(),
                2,
            )
            .unwrap();
        let request = Request::encode_read(1, &token, 0, 4).unwrap();
        let mut clock_calls = 0;
        let result = broker.run(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            HANDLER,
            &request,
            || {
                clock_calls += 1;
                if clock_calls == 3 {
                    if stop {
                        instance.stop();
                        4
                    } else {
                        10
                    }
                } else {
                    3
                }
            },
        );
        assert_eq!(clock_calls, 3);
        assert!(
            binding.usage().bytes > 4,
            "broker callback must have read and charged its real response"
        );
        assert!(result.execution.outcome.is_err());
        assert!(result.response.is_none());
        assert_eq!(broker.usage(), (0, 0));
        assert_eq!(binding.usage().resources, 0);
    }
}

#[test]
fn longer_binding_cannot_extend_original_file_lease_at_final_delivery() {
    let mut f = Fixture::new(true, 2, 1_000_000);
    let instance = f.connect();
    let short = f.bind(&instance, 10, 1);
    let long = f.bind(&instance, 90, 1);
    let mut broker = FileBroker::new([13; 32]);
    let token = broker
        .grant_file(&f.manager, &f.host, &instance, &short, b"short".to_vec(), 2)
        .unwrap();
    let request = Request::encode_read(1, &token, 0, 5).unwrap();
    let mut times = [3, 3, 10].into_iter();
    let report = broker.run(
        &f.manager,
        &f.host,
        &instance,
        &long,
        HANDLER,
        &request,
        || times.next().unwrap(),
    );
    assert!(times.next().is_none());
    assert!(
        long.usage().bytes > 5,
        "real callback must have produced and charged the response"
    );
    assert!(report.execution.outcome.is_err());
    assert!(report.response.is_none());
    assert_eq!(broker.usage(), (0, 0));
    long.check(&f.manager, &f.host, &instance, 10).unwrap();
}

#[test]
fn foreign_instance_with_future_clock_cannot_reap_other_instances_files() {
    let mut f = Fixture::new(true, 2, 1_000_000);
    let owner = f.connect();
    let foreign = f.connect();
    let binding = f.bind(&owner, 10, 1);
    let mut broker = FileBroker::new([14; 32]);
    let token = broker
        .grant_file(&f.manager, &f.host, &owner, &binding, b"owner".to_vec(), 2)
        .unwrap();
    let request = Request::encode_read(1, &token, 0, 5).unwrap();
    let rejected = exchange(&f, &mut broker, &foreign, &binding, &request, 999);
    assert_ne!(rejected.status, Status::Completed);
    assert!(rejected.payload.is_empty());
    assert_eq!(broker.usage(), (1, 5));
    let run = broker.run(
        &f.manager,
        &f.host,
        &foreign,
        &binding,
        HANDLER,
        &request,
        || 999,
    );
    assert!(run.execution.outcome.is_err());
    assert!(run.response.is_none());
    assert_eq!(broker.usage(), (1, 5));
    let owner_result = exchange(&f, &mut broker, &owner, &binding, &request, 3);
    assert_eq!(owner_result.status, Status::Completed);
    assert_eq!(owner_result.payload, b"owner");
}

#[test]
fn foreign_token_with_valid_binding_cannot_advance_clock_or_reap_owner() {
    let mut f = Fixture::new(true, 2, 1_000_000);
    let owner = f.connect();
    let other = f.connect();
    let owner_binding = f.bind(&owner, 10, 1);
    let other_binding = f.bind(&other, 90, 1);
    let mut broker = FileBroker::new([15; 32]);
    let owner_token = broker
        .grant_file(
            &f.manager,
            &f.host,
            &owner,
            &owner_binding,
            b"owner".to_vec(),
            2,
        )
        .unwrap();
    let other_token = broker
        .grant_file(
            &f.manager,
            &f.host,
            &other,
            &other_binding,
            b"other".to_vec(),
            2,
        )
        .unwrap();
    let owner_request = Request::encode_read(1, &owner_token, 0, 5).unwrap();
    let before = other_binding.usage();
    let rejected = exchange(&f, &mut broker, &other, &other_binding, &owner_request, 50);
    assert_ne!(rejected.status, Status::Completed);
    assert!(rejected.payload.is_empty());
    assert_eq!(other_binding.usage(), before);
    assert_eq!(broker.usage(), (2, 10));
    let run = broker.run(
        &f.manager,
        &f.host,
        &other,
        &other_binding,
        HANDLER,
        &owner_request,
        || 50,
    );
    assert!(
        run.response
            .as_ref()
            .is_none_or(|response| response.status != Status::Completed)
    );
    assert_eq!(other_binding.usage(), before);
    assert_eq!(broker.usage(), (2, 10));
    let own = exchange(&f, &mut broker, &owner, &owner_binding, &owner_request, 3);
    assert_eq!(own.payload, b"owner");
    let other_request = Request::encode_read(2, &other_token, 0, 5).unwrap();
    let other_result = exchange(&f, &mut broker, &other, &other_binding, &other_request, 3);
    assert_eq!(other_result.status, Status::Completed);
    assert_eq!(other_result.payload, b"other");
}

#[test]
fn successful_guest_delivery_advances_shared_clock_to_final_observation() {
    let mut f = Fixture::new(true, 2, 1_000_000);
    let instance = f.connect();
    let binding = f.bind(&instance, 90, 1);
    let mut broker = FileBroker::new([16; 32]);
    let token = broker
        .grant_file(&f.manager, &f.host, &instance, &binding, b"ok".to_vec(), 2)
        .unwrap();
    let request = Request::encode_read(1, &token, 0, 2).unwrap();
    let mut times = [3, 4, 8].into_iter();
    let report = broker.run(
        &f.manager,
        &f.host,
        &instance,
        &binding,
        HANDLER,
        &request,
        || times.next().unwrap(),
    );
    assert!(times.next().is_none());
    assert!(report.execution.outcome.is_ok());
    assert_eq!(report.response.unwrap().payload, b"ok");
    assert_eq!(
        binding.check(&f.manager, &f.host, &instance, 7),
        Err(AdmissionError::Clock)
    );
    binding.check(&f.manager, &f.host, &instance, 8).unwrap();
}

#[test]
fn file_list_only_binding_cannot_grant_read_or_mutate_clock_and_other_resources() {
    let mut f = Fixture::with_capabilities(
        true,
        2,
        1_000_000,
        BTreeSet::from([IoCapability::FileRead, IoCapability::FileList]),
    );
    let owner = f.connect();
    let listing = f.connect();
    let owner_binding = f.bind(&owner, 10, 1);
    let listing_binding = f
        .manager
        .bind_io(
            &f.host,
            &listing,
            f.package.digest(),
            f.manager.revision(),
            &BTreeSet::from([IoCapability::FileList]),
            90,
            1,
        )
        .unwrap();
    let mut broker = FileBroker::new([17; 32]);
    let token = broker
        .grant_file(
            &f.manager,
            &f.host,
            &owner,
            &owner_binding,
            b"owner".to_vec(),
            2,
        )
        .unwrap();
    let before = listing_binding.usage();
    let denied = broker.grant_file(
        &f.manager,
        &f.host,
        &listing,
        &listing_binding,
        b"forbidden".to_vec(),
        50,
    );
    assert_eq!(denied, Err(AdmissionError::Denied));
    assert_eq!(listing_binding.usage(), before);
    assert_eq!(broker.usage(), (1, 5));
    listing_binding
        .check(&f.manager, &f.host, &listing, 3)
        .unwrap();
    let request = Request::encode_read(1, &token, 0, 5).unwrap();
    let response = exchange(&f, &mut broker, &owner, &owner_binding, &request, 3);
    assert_eq!(response.status, Status::Completed);
    assert_eq!(response.payload, b"owner");
}

#[test]
fn managed_run_rejects_duplicate_io_forged_completion_and_core_exchange() {
    let variants = [
        (
            "duplicate-io",
            "",
            "i32.const 0 local.get $input i32.const 131072 i32.const 131072 call $io drop",
        ),
        (
            "forged-completion",
            "",
            "i32.const 131072 i32.const 131072 i32.load8_u i32.const 1 i32.xor i32.store8",
        ),
        (
            "core-exchange",
            r#"(import "morrow_v1" "exchange" (func $core (param i32 i32 i32 i32) (result i32)))"#,
            "i32.const 0 i32.const 1 i32.const 196608 i32.const 65536 call $core drop",
        ),
    ];
    for (label, extra_import, forbidden) in variants {
        let wasm = wat::parse_str(format!(r#"(module
            (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
            (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
            (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
            {extra_import}
            (memory (export "memory") 4)
            (func (export "morrow_run") (result i32)
                (local $input i32) (local $output i32)
                i32.const 0 i32.const 131072 call $read local.set $input
                i32.const 0 local.get $input i32.const 131072 i32.const 131072 call $io local.set $output
                {forbidden}
                i32.const 131072 local.get $output call $done drop
                i32.const 0))"#)).unwrap();
        let mut f = Fixture::with_module(true, 2, 1_000_000, capabilities(), wasm);
        let instance = f.connect();
        let binding = f.bind(&instance, 90, 1);
        let mut broker = FileBroker::new([18; 32]);
        let token = broker
            .grant_file(
                &f.manager,
                &f.host,
                &instance,
                &binding,
                b"original".to_vec(),
                2,
            )
            .unwrap();
        let request = Request::encode_read(1, &token, 0, 8).unwrap();
        let report = broker.run(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            HANDLER,
            &request,
            || 3,
        );
        assert!(
            binding.usage().bytes > 8,
            "{label}: actual broker response must have been produced first"
        );
        assert_eq!(
            report.execution.host_calls,
            if label == "forged-completion" { 1 } else { 2 },
            "{label}: callbacks must actually execute"
        );
        assert!(report.execution.outcome.is_err(), "{label}");
        assert!(report.response.is_none(), "{label}");
    }
}
