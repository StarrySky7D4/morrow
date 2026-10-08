//! Ordinary temporary Core/registry SQLite only; no protected account or OS process.
use morrow_agent_catalog_admin_v1::{Action, Approval, Body, Request, Revisions, Status};
use morrow_agent_catalog_owner_v1::{CatalogOwner, MAX_MUTATING_REQUESTS};
use morrow_agent_process_control_v1::{Capabilities as PC, host::Host as ProcessHost};
use morrow_agent_session_exec_v1_r2::{
    Action as R2Action, Error, Outcome as R2Outcome, Reply as R2Reply, Request as R2Request,
    Result as R2Result,
    authority::{Capabilities as SC, SessionExecHost},
};
use morrow_agent_session_process_v1_host::{
    AgentProcessPackage, Declaration, MAX_ARCHIVE_BYTES,
    catalog::{Catalog, CatalogManagedPackage, NativeCatalogStorage, WrapperCatalogStorage},
};
use morrow_core::{
    dispatch::HostRuntime,
    lifecycle::InstancePhase,
    plugin_package::{
        Package,
        registry::{Registry, RegistryStorage, sqlite::SqliteRegistryStorage},
    },
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{Limits, manager::Manager};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    },
};

const ID: &str = "org.example.catalog-owner";
fn module() -> Vec<u8> {
    wat::parse_str(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
        (import "morrow_agent_session_process_v1" "call"
            (func $call (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 6)
        (func (export "morrow_run") (result i32) (local $n i32)
            (local.set $n (call $read (i32.const 0) (i32.const 131072)))
            (local.set $n (call $call (i32.const 0) (local.get $n)
                (i32.const 131072) (i32.const 131072)))
            (drop (call $done (i32.const 131072) (local.get $n)))
            i32.const 0))"#,
    )
    .unwrap()
}
fn package(id: &str, version: &str, domain: &str) -> AgentProcessPackage {
    let wasm = module();
    AgentProcessPackage::build(
        Package::build(
            Package::manifest_for_task(id, version, &wasm, vec![]),
            &wasm,
        )
        .unwrap(),
        Declaration {
            session: SC {
                session_read: true,
                session_write: true,
                propose: true,
                ..Default::default()
            },
            process: PC {
                read: true,
                events: true,
                write: true,
                ..Default::default()
            },
            sessions: vec!["session".into()],
            execution_domain: domain.into(),
        },
    )
    .unwrap()
}
fn approval() -> Approval {
    Approval {
        session_bits: 7,
        process_bits: 7,
        sessions: vec!["session".into()],
        domain: "domain".into(),
    }
}
#[derive(Default)]
struct Fault {
    mode: AtomicU8,
    publishes: AtomicUsize,
    installs: AtomicUsize,
}
struct CatalogStorage {
    original: NativeCatalogStorage,
    fault: Arc<Fault>,
}
impl WrapperCatalogStorage for CatalogStorage {
    fn read(&self) -> R2Result<Option<Vec<u8>>> {
        self.original.read()
    }
    fn load(&self, sha: [u8; 32]) -> R2Result<AgentProcessPackage> {
        self.original.load(sha)
    }
    fn install(&self, package: &AgentProcessPackage) -> R2Result<()> {
        self.fault.installs.fetch_add(1, Ordering::SeqCst);
        self.original.install(package)
    }
    fn publish(&mut self, bytes: &[u8]) -> R2Result<()> {
        self.fault.publishes.fetch_add(1, Ordering::SeqCst);
        match self.fault.mode.load(Ordering::SeqCst) {
            1 => Err(Error::Storage),
            2 => {
                self.original.publish(bytes)?;
                Err(Error::CommitUnknown)
            }
            _ => self.original.publish(bytes),
        }
    }
}
struct ManagerStorage {
    original: SqliteRegistryStorage,
    fault: Arc<Fault>,
}
impl RegistryStorage for ManagerStorage {
    fn read(&self) -> morrow_core::Result<Option<Vec<u8>>> {
        self.original.read()
    }
    fn load_package(&self, sha: [u8; 32]) -> morrow_core::Result<Package> {
        self.original.load_package(sha)
    }
    fn install_package(&self, package: &Package) -> morrow_core::Result<()> {
        self.fault.installs.fetch_add(1, Ordering::SeqCst);
        self.original.install_package(package)
    }
    fn publish(&mut self, bytes: &[u8]) -> morrow_core::Result<()> {
        self.fault.publishes.fetch_add(1, Ordering::SeqCst);
        match self.fault.mode.load(Ordering::SeqCst) {
            1 => Err(morrow_core::Error::Storage),
            2 => {
                self.original.publish(bytes)?;
                Err(morrow_core::Error::CommitUnknown)
            }
            _ => self.original.publish(bytes),
        }
    }
}
struct Fixture {
    owner: Option<CatalogOwner>,
    manager: Manager,
    runtime: HostRuntime,
    host: SessionExecHost,
    processes: ProcessHost,
    package: AgentProcessPackage,
    path: PathBuf,
    temp: tempfile::TempDir,
    catalog_fault: Arc<Fault>,
    manager_fault: Arc<Fault>,
    nonce: u64,
}
impl Fixture {
    fn new(reserved: Vec<String>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let manager_fault = Arc::new(Fault::default());
        let registry = Registry::from_storage(Box::new(ManagerStorage {
            original: SqliteRegistryStorage::open(&temp.path().join("manager.sqlite"), true)
                .unwrap(),
            fault: manager_fault.clone(),
        }))
        .unwrap();
        let manager = Manager::new(
            registry,
            Limits {
                fuel: 100_000_000,
                memory_bytes: 16 * 1024 * 1024,
                host_calls: 16,
            },
        );
        let catalog_fault = Arc::new(Fault::default());
        let catalog = Catalog::from_storage(Box::new(CatalogStorage {
            original: NativeCatalogStorage::open(&temp.path().join("catalog"), true).unwrap(),
            fault: catalog_fault.clone(),
        }))
        .unwrap();
        let owner = CatalogOwner::with_reserved(catalog, reserved).unwrap();
        let mut runtime = HostRuntime::new(
            Store::open(&temp.path().join("ordinary.sqlite"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        let host = SessionExecHost::new(&mut runtime).unwrap();
        let package = package(ID, "1.0.0", "domain");
        let path = temp.path().join("selected.magent");
        std::fs::write(&path, package.archive()).unwrap();
        Self {
            owner: Some(owner),
            manager,
            runtime,
            host,
            processes: ProcessHost::default(),
            package,
            path,
            temp,
            catalog_fault,
            manager_fault,
            nonce: 0,
        }
    }
    fn revisions(&self) -> Revisions {
        self.owner.as_ref().unwrap().revisions(&self.manager)
    }
    fn request(&mut self, action: Action) -> Request {
        self.nonce += 1;
        let mut id = [0; 16];
        id[..8].copy_from_slice(&self.nonce.to_le_bytes());
        Request { id, action }
    }
    fn call(&mut self, action: Action) -> morrow_agent_catalog_admin_v1::Outcome {
        let request = self.request(action);
        let invalid = request.validate().is_err();
        let before = self.revisions();
        let writes = [
            self.catalog_fault.installs.load(Ordering::SeqCst),
            self.catalog_fault.publishes.load(Ordering::SeqCst),
            self.manager_fault.installs.load(Ordering::SeqCst),
            self.manager_fault.publishes.load(Ordering::SeqCst),
        ];
        let outcome = self
            .owner
            .as_mut()
            .unwrap()
            .execute(&mut self.manager, &request);
        if invalid {
            // Invalid typed-owner tests cannot construct a legal wire request.
            // Preserve their exact status assertions and prove no writes/revision change.
            assert_ne!(outcome.status, Status::Ok);
            assert_eq!(outcome.body, Body::None);
            assert_eq!(self.revisions(), before);
            assert_eq!(
                writes,
                [
                    self.catalog_fault.installs.load(Ordering::SeqCst),
                    self.catalog_fault.publishes.load(Ordering::SeqCst),
                    self.manager_fault.installs.load(Ordering::SeqCst),
                    self.manager_fault.publishes.load(Ordering::SeqCst),
                ]
            );
        } else {
            // Actual wire reply validation covers valid field/cursor/capability mapping.
            let reply =
                morrow_agent_catalog_admin_v1::Reply::new(&request, outcome.clone()).unwrap();
            reply.encode_for(&request).unwrap();
        }
        outcome
    }
    fn install(&mut self) {
        let revisions = self.revisions();
        assert_eq!(
            self.call(Action::Install {
                path: self.path.to_str().unwrap().into(),
                full_sha256: self.package.review_sha256(),
                revisions,
            })
            .status,
            Status::Ok
        );
    }
    fn base_select(&mut self) {
        let revisions = self.revisions();
        assert_eq!(
            self.call(Action::BaseSelect {
                full_sha256: self.package.review_sha256(),
                revisions,
            })
            .status,
            Status::Ok
        );
    }
    fn base_enable(&mut self, enabled: bool) -> Status {
        let revisions = self.revisions();
        self.call(Action::BaseEnable {
            id: ID.into(),
            full_sha256: self.package.review_sha256(),
            enabled,
            revisions,
        })
        .status
    }
    fn wrapper_select(&mut self) {
        let revisions = self.revisions();
        assert_eq!(
            self.call(Action::WrapperSelect {
                full_sha256: self.package.review_sha256(),
                revisions,
            })
            .status,
            Status::Ok
        );
    }
    fn approve(&mut self, approval: Approval) -> Status {
        let revisions = self.revisions();
        self.call(Action::Approve {
            id: ID.into(),
            full_sha256: self.package.review_sha256(),
            approval,
            revisions,
        })
        .status
    }
    fn wrapper_enable(&mut self, enabled: bool) -> Status {
        let revisions = self.revisions();
        self.call(Action::WrapperEnable {
            id: ID.into(),
            full_sha256: self.package.review_sha256(),
            enabled,
            revisions,
        })
        .status
    }
    fn activate(&mut self) {
        self.install();
        self.base_select();
        assert_eq!(self.base_enable(true), Status::Ok);
        self.wrapper_select();
        assert_eq!(self.approve(approval()), Status::Ok);
        assert_eq!(self.wrapper_enable(true), Status::Ok);
    }
    fn connect(&mut self) -> R2Result<CatalogManagedPackage> {
        let revisions = self.revisions();
        self.owner.as_mut().unwrap().connect(
            &mut self.manager,
            &mut self.runtime,
            &self.host,
            ID,
            self.package.review_sha256(),
            revisions,
            100_000,
            1,
        )
    }
    fn close(&mut self, bridge: &CatalogManagedPackage) {
        bridge
            .close(&mut self.runtime, &self.host, &mut self.processes)
            .unwrap();
    }
    fn list(&mut self, bridge: &CatalogManagedPackage) {
        let request =
            R2Request::new_for_generation("owner-list", bridge.generation(), R2Action::List)
                .unwrap();
        let run = bridge
            .run(
                &self.manager,
                &mut self.runtime,
                &self.host,
                &mut self.processes,
                request.raw(),
                || 1,
            )
            .unwrap();
        assert_eq!(run.report.outcome, Ok(0));
        assert!(matches!(
            R2Reply::decode_for(&request, &run.completion.unwrap())
                .unwrap()
                .outcome,
            R2Outcome::Sessions(_)
        ));
    }
}

#[test]
fn inspection_is_read_only_and_reports_both_real_schema_and_archive_identities() {
    let mut f = Fixture::new(vec![]);
    let before = f.revisions();
    let inspected = f.call(Action::Inspect {
        path: f.path.to_str().unwrap().into(),
    });
    assert_eq!(inspected.status, Status::Ok);
    let Body::Review(review) = inspected.body else {
        panic!("review expected")
    };
    assert_eq!(review.id, ID);
    assert_eq!(review.full_sha256, f.package.review_sha256());
    assert_eq!(review.base_sha256, f.package.base_sha256());
    assert_eq!(
        review.session_schema,
        morrow_agent_session_exec_v1_r2::schema_digest()
    );
    assert_eq!(
        review.process_schema,
        morrow_agent_process_control_v1::schema_digest()
    );
    assert_eq!((review.session_bits, review.process_bits), (7, 7));
    assert_eq!(f.revisions(), before);
    assert_eq!(f.catalog_fault.installs.load(Ordering::SeqCst), 0);
    assert_eq!(f.manager_fault.installs.load(Ordering::SeqCst), 0);
    assert!(f.manager.selection(ID).is_none());
}

#[test]
fn full_explicit_steps_enable_only_the_reviewed_wrapper_and_borrow_original_core() {
    let mut f = Fixture::new(vec![]);
    let original_host = f.runtime.binding();
    f.install();
    assert!(f.manager.selection(ID).is_none());
    assert!(f.connect().is_err());
    f.base_select();
    assert!(!f.manager.selection(ID).unwrap().enabled);
    assert_eq!(f.base_enable(true), Status::Ok);
    assert!(f.connect().is_err());
    f.wrapper_select();
    assert_eq!(f.wrapper_enable(true), Status::Denied);
    assert_eq!(f.approve(approval()), Status::Ok);
    assert!(f.connect().is_err());
    assert_eq!(f.wrapper_enable(true), Status::Ok);
    let bridge = f.connect().unwrap();
    let connection = bridge.shared_connection();
    assert_eq!(f.runtime.binding(), original_host);
    assert!(Arc::ptr_eq(&connection, &bridge.shared_connection()));
    assert_eq!(connection.package_digest(), Some(f.package.base_sha256()));
    assert_eq!(
        f.runtime.connection_phase(&connection),
        Ok(InstancePhase::Ready)
    );
    assert!(f.manager.selection(ID).unwrap().approved.is_empty());
    assert!(f.manager.selection(ID).unwrap().approved_io.is_empty());
    f.list(&bridge);
    f.close(&bridge);
}

#[test]
fn base_enable_does_not_approve_session_process_or_old_factory_access() {
    let mut f = Fixture::new(vec![]);
    f.install();
    f.base_select();
    assert_eq!(f.base_enable(true), Status::Ok);
    assert!(f.manager.connect(ID, &mut f.runtime).is_err());
    assert!(f.connect().is_err());
    let revisions = f.revisions();
    let page = f.call(Action::Page {
        after: None,
        limit: 16,
        revisions,
    });
    let Body::Page { entries, .. } = page.body else {
        panic!("page expected")
    };
    assert_eq!(entries.len(), 1);
    assert!(entries[0].base_selected && entries[0].base_enabled);
    assert!(!entries[0].selected && !entries[0].enabled);
    assert!(entries[0].approval.is_none());
}

#[test]
fn stale_dual_revisions_prevent_base_or_wrapper_changes_without_stopping_live_core() {
    let mut f = Fixture::new(vec![]);
    f.activate();
    let bridge = f.connect().unwrap();
    let revoked = f.runtime.revocation(&bridge.shared_connection()).unwrap();
    let now = f.revisions();
    for revisions in [
        Revisions {
            catalog: now.catalog - 1,
            manager: now.manager,
        },
        Revisions {
            catalog: now.catalog,
            manager: now.manager - 1,
        },
    ] {
        assert_eq!(
            f.call(Action::BaseEnable {
                id: ID.into(),
                full_sha256: f.package.review_sha256(),
                enabled: false,
                revisions
            })
            .status,
            Status::Conflict
        );
        assert_eq!(
            f.call(Action::WrapperEnable {
                id: ID.into(),
                full_sha256: f.package.review_sha256(),
                enabled: false,
                revisions
            })
            .status,
            Status::Conflict
        );
        assert!(!revoked.is_revoked());
    }
    assert_eq!(f.revisions(), now);
    f.list(&bridge);
    f.close(&bridge);
}

#[test]
fn installation_rereads_selected_bytes_and_rejects_replaced_or_wrong_archive_hash() {
    let mut f = Fixture::new(vec![]);
    let inspection = f.call(Action::Inspect {
        path: f.path.to_str().unwrap().into(),
    });
    assert_eq!(inspection.status, Status::Ok);
    let original_sha = f.package.review_sha256();
    let next = package(ID, "1.0.0", "other-domain");
    std::fs::write(&f.path, next.archive()).unwrap();
    let before = f.revisions();
    assert_eq!(
        f.call(Action::Install {
            path: f.path.to_str().unwrap().into(),
            full_sha256: original_sha,
            revisions: before
        })
        .status,
        Status::Denied
    );
    assert_eq!(f.revisions(), before);
    assert_eq!(f.catalog_fault.installs.load(Ordering::SeqCst), 0);
    assert_eq!(f.manager_fault.installs.load(Ordering::SeqCst), 0);
}

#[test]
fn reserved_application_ids_cannot_be_inspected_installed_selected_or_enabled() {
    let mut f = Fixture::new(vec![ID.into()]);
    let before = f.revisions();
    assert_eq!(
        f.call(Action::Inspect {
            path: f.path.to_str().unwrap().into()
        })
        .status,
        Status::Denied
    );
    assert_eq!(
        f.call(Action::Install {
            path: f.path.to_str().unwrap().into(),
            full_sha256: f.package.review_sha256(),
            revisions: before
        })
        .status,
        Status::Denied
    );
    assert_eq!(f.catalog_fault.installs.load(Ordering::SeqCst), 0);
    assert_eq!(f.manager_fault.installs.load(Ordering::SeqCst), 0);
    // Also deny an already persisted reserved wrapper after trusted configuration changes.
    let mut unrestricted = Fixture::new(vec![]);
    unrestricted.activate();
    drop(unrestricted.owner.take());
    unrestricted.owner = Some(
        CatalogOwner::open(
            &unrestricted.temp.path().join("catalog"),
            false,
            vec![ID.into()],
        )
        .unwrap(),
    );
    let revisions = unrestricted.revisions();
    for action in [
        Action::BaseSelect {
            full_sha256: unrestricted.package.review_sha256(),
            revisions,
        },
        Action::BaseEnable {
            id: ID.into(),
            full_sha256: unrestricted.package.review_sha256(),
            enabled: false,
            revisions,
        },
        Action::WrapperSelect {
            full_sha256: unrestricted.package.review_sha256(),
            revisions,
        },
        Action::Approve {
            id: ID.into(),
            full_sha256: unrestricted.package.review_sha256(),
            approval: approval(),
            revisions,
        },
        Action::WrapperEnable {
            id: ID.into(),
            full_sha256: unrestricted.package.review_sha256(),
            enabled: false,
            revisions,
        },
        Action::Remove {
            id: ID.into(),
            full_sha256: unrestricted.package.review_sha256(),
            revisions,
        },
    ] {
        assert_eq!(unrestricted.call(action).status, Status::Denied);
    }
    assert!(matches!(unrestricted.connect(), Err(Error::Denied)));
    assert_eq!(unrestricted.revisions(), revisions);
}

#[test]
fn approval_subset_scope_domain_and_id_digest_pairs_fail_without_revocation() {
    let mut f = Fixture::new(vec![]);
    f.activate();
    let bridge = f.connect().unwrap();
    let revoked = f.runtime.revocation(&bridge.shared_connection()).unwrap();
    for variant in 0..4 {
        let mut value = approval();
        match variant {
            0 => value.session_bits |= 8,
            1 => value.process_bits |= 64,
            2 => value.sessions = vec!["other-session".into()],
            _ => value.domain = "other-domain".into(),
        }
        assert_eq!(f.approve(value), Status::Denied);
        assert!(!revoked.is_revoked());
    }
    let revisions = f.revisions();
    assert_eq!(
        f.call(Action::BaseEnable {
            id: "other-id".into(),
            full_sha256: f.package.review_sha256(),
            enabled: false,
            revisions
        })
        .status,
        Status::Denied
    );
    assert_eq!(
        f.call(Action::WrapperEnable {
            id: ID.into(),
            full_sha256: [8; 32],
            enabled: true,
            revisions
        })
        .status,
        Status::NotFound
    );
    assert!(!revoked.is_revoked());
    f.close(&bridge);
}

#[test]
fn duplicate_or_changed_mutation_id_never_replays_an_old_confirmation() {
    let mut f = Fixture::new(vec![]);
    let revisions = f.revisions();
    let request = f.request(Action::Install {
        path: f.path.to_str().unwrap().into(),
        full_sha256: f.package.review_sha256(),
        revisions,
    });
    assert_eq!(
        f.owner
            .as_mut()
            .unwrap()
            .execute(&mut f.manager, &request)
            .status,
        Status::Ok
    );
    let writes = f.catalog_fault.publishes.load(Ordering::SeqCst);
    assert_eq!(
        f.owner
            .as_mut()
            .unwrap()
            .execute(&mut f.manager, &request)
            .status,
        Status::Conflict
    );
    let changed = Request {
        id: request.id,
        action: Action::BaseSelect {
            full_sha256: f.package.review_sha256(),
            revisions: f.revisions(),
        },
    };
    assert_eq!(
        f.owner
            .as_mut()
            .unwrap()
            .execute(&mut f.manager, &changed)
            .status,
        Status::Conflict
    );
    assert_eq!(f.catalog_fault.publishes.load(Ordering::SeqCst), writes);
    assert!(f.manager.selection(ID).is_none());
}

#[test]
fn mutation_id_budget_is_bounded_and_never_evicts_or_refunds_failed_confirmations() {
    let mut f = Fixture::new(vec![]);
    let revisions = f.revisions();
    for _ in 0..MAX_MUTATING_REQUESTS {
        assert_eq!(
            f.call(Action::BaseSelect {
                full_sha256: [7; 32],
                revisions
            })
            .status,
            Status::NotFound
        );
    }
    assert_eq!(
        f.call(Action::Install {
            path: f.path.to_str().unwrap().into(),
            full_sha256: f.package.review_sha256(),
            revisions
        })
        .status,
        Status::Limit
    );
    assert_eq!(f.catalog_fault.installs.load(Ordering::SeqCst), 0);
    assert_eq!(f.call(Action::State).status, Status::Ok);
}

#[test]
fn durable_approval_reopens_but_never_restores_old_live_grants() {
    let mut f = Fixture::new(vec![]);
    f.activate();
    let bridge = f.connect().unwrap();
    let original = bridge.shared_connection();
    let revoked = f.runtime.revocation(&original).unwrap();
    let before = f.revisions();
    drop(f.owner.take());
    assert!(revoked.is_revoked());
    f.owner = Some(CatalogOwner::open(&f.temp.path().join("catalog"), false, vec![]).unwrap());
    assert_eq!(f.revisions(), before);
    let fresh = f.connect().unwrap();
    assert!(!Arc::ptr_eq(&original, &fresh.shared_connection()));
    assert_eq!(
        f.runtime.connection_phase(&original),
        Ok(InstancePhase::Revoked)
    );
    f.list(&fresh);
    f.close(&bridge);
    f.close(&fresh);
}

#[test]
fn catalog_commit_unknown_stops_original_core_and_never_retries_until_explicit_reopen() {
    let mut f = Fixture::new(vec![]);
    f.activate();
    let bridge = f.connect().unwrap();
    let revoked = f.runtime.revocation(&bridge.shared_connection()).unwrap();
    f.catalog_fault.mode.store(2, Ordering::SeqCst);
    assert_eq!(f.wrapper_enable(false), Status::Unknown);
    assert!(revoked.is_revoked());
    let writes = f.catalog_fault.publishes.load(Ordering::SeqCst);
    assert_eq!(f.wrapper_enable(true), Status::Unknown);
    assert_eq!(f.call(Action::State).status, Status::Unknown);
    assert!(matches!(f.connect(), Err(Error::CommitUnknown)));
    assert_eq!(f.catalog_fault.publishes.load(Ordering::SeqCst), writes);
    f.close(&bridge);
    drop(f.owner.take());
    f.owner = Some(CatalogOwner::open(&f.temp.path().join("catalog"), false, vec![]).unwrap());
    assert!(matches!(f.connect(), Err(Error::Denied)));
    assert_eq!(f.wrapper_enable(true), Status::Ok);
    let fresh = f.connect().unwrap();
    f.close(&fresh);
}

#[test]
fn original_manager_commit_unknown_poison_revokes_catalog_even_for_base_step() {
    let mut f = Fixture::new(vec![]);
    f.activate();
    let bridge = f.connect().unwrap();
    let revoked = f.runtime.revocation(&bridge.shared_connection()).unwrap();
    f.manager_fault.mode.store(2, Ordering::SeqCst);
    assert_eq!(f.base_enable(false), Status::Unknown);
    assert!(revoked.is_revoked());
    let writes = f.manager_fault.publishes.load(Ordering::SeqCst);
    assert_eq!(f.base_enable(true), Status::Unknown);
    assert_eq!(f.call(Action::State).status, Status::Unknown);
    assert!(matches!(f.connect(), Err(Error::CommitUnknown)));
    assert_eq!(f.manager_fault.publishes.load(Ordering::SeqCst), writes);
    f.close(&bridge);
}

#[test]
fn known_catalog_save_failure_remains_stopped_and_does_not_retry_or_reopen_itself() {
    let mut f = Fixture::new(vec![]);
    f.activate();
    let bridge = f.connect().unwrap();
    let revoked = f.runtime.revocation(&bridge.shared_connection()).unwrap();
    f.catalog_fault.mode.store(1, Ordering::SeqCst);
    assert_eq!(f.wrapper_enable(false), Status::Storage);
    assert!(revoked.is_revoked());
    let writes = f.catalog_fault.publishes.load(Ordering::SeqCst);
    assert_eq!(f.wrapper_enable(true), Status::Unknown);
    assert_eq!(f.catalog_fault.publishes.load(Ordering::SeqCst), writes);
    f.close(&bridge);
}

#[test]
fn owner_move_preserves_original_runtime_manager_and_connection_identity() {
    let mut f = Fixture::new(vec![]);
    f.activate();
    let original_host = f.runtime.binding();
    let bridge = f.connect().unwrap();
    let original_connection = bridge.shared_connection();
    let owner = Box::new(f.owner.take().unwrap());
    f.owner = Some(*owner);
    assert_eq!(f.runtime.binding(), original_host);
    f.list(&bridge);
    assert!(Arc::ptr_eq(
        &original_connection,
        &bridge.shared_connection()
    ));
    let fresh = f.connect().unwrap();
    assert!(!Arc::ptr_eq(
        &original_connection,
        &fresh.shared_connection()
    ));
    assert_eq!(f.runtime.binding(), original_host);
    f.close(&bridge);
    f.close(&fresh);
}

#[test]
fn regular_file_bounds_reject_directory_missing_relative_network_and_oversized_input() {
    let mut f = Fixture::new(vec![]);
    for (path, expected) in [
        (f.temp.path().to_str().unwrap().to_string(), Status::Denied),
        (
            f.temp
                .path()
                .join("missing.magent")
                .to_str()
                .unwrap()
                .to_string(),
            Status::NotFound,
        ),
        ("selected.magent".into(), Status::Invalid),
        (
            "https://example.invalid/package.magent".into(),
            Status::Invalid,
        ),
        (
            f.temp
                .path()
                .join("sub/../selected.magent")
                .to_str()
                .unwrap()
                .to_string(),
            Status::Invalid,
        ),
    ] {
        assert_eq!(f.call(Action::Inspect { path }).status, expected);
    }
    let huge = f.temp.path().join("huge.magent");
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(MAX_ARCHIVE_BYTES as u64 + 1)
        .unwrap();
    assert_eq!(
        f.call(Action::Inspect {
            path: huge.to_str().unwrap().into()
        })
        .status,
        Status::Limit
    );
    assert_eq!(
        f.revisions(),
        Revisions {
            catalog: 0,
            manager: 0
        }
    );
    assert_eq!(f.catalog_fault.installs.load(Ordering::SeqCst), 0);
}

#[cfg(unix)]
#[test]
fn symlink_files_and_ancestors_are_rejected_without_following_the_target() {
    let mut f = Fixture::new(vec![]);
    let file_link = f.temp.path().join("linked.magent");
    std::os::unix::fs::symlink(&f.path, &file_link).unwrap();
    assert_eq!(
        f.call(Action::Inspect {
            path: file_link.to_str().unwrap().into()
        })
        .status,
        Status::Denied
    );
    let dir_link = f.temp.path().join("linked-directory");
    std::os::unix::fs::symlink(f.temp.path(), &dir_link).unwrap();
    assert_eq!(
        f.call(Action::Inspect {
            path: dir_link.join("selected.magent").to_str().unwrap().into()
        })
        .status,
        Status::Denied
    );
}

#[cfg(windows)]
#[test]
fn windows_namespace_and_alternate_stream_paths_are_rejected() {
    let mut f = Fixture::new(vec![]);
    for path in [
        r"\\server\share\selected.magent".to_string(),
        r"\\?\C:\selected.magent".into(),
        r"\\.\pipe\selected.magent".into(),
    ] {
        assert_eq!(f.call(Action::Inspect { path }).status, Status::Denied);
    }
    assert_eq!(
        f.call(Action::Inspect {
            path: format!("{}:stream", f.path.display())
        })
        .status,
        Status::Invalid
    );
    assert_eq!(
        f.call(Action::Inspect {
            path: "C:selected.magent".into()
        })
        .status,
        Status::Invalid
    );
}

#[test]
fn pages_use_exact_dual_revision_cursor_and_base_flags_for_all_installed_wrappers() {
    let mut f = Fixture::new(vec![]);
    f.activate();
    let other = package(ID, "1.0.0", "other-domain");
    let path = f.temp.path().join("other.magent");
    std::fs::write(&path, other.archive()).unwrap();
    let revisions = f.revisions();
    assert_eq!(
        f.call(Action::Install {
            path: path.to_str().unwrap().into(),
            full_sha256: other.review_sha256(),
            revisions
        })
        .status,
        Status::Ok
    );
    let revisions = f.revisions();
    let Body::Page {
        entries: first,
        next,
    } = f
        .call(Action::Page {
            after: None,
            limit: 1,
            revisions,
        })
        .body
    else {
        panic!("page")
    };
    assert_eq!(first.len(), 1);
    let Body::Page {
        entries: second,
        next: last,
    } = f
        .call(Action::Page {
            after: next,
            limit: 1,
            revisions,
        })
        .body
    else {
        panic!("page")
    };
    assert_eq!(second.len(), 1);
    assert!(last.is_none());
    assert!(first[0].review.full_sha256 < second[0].review.full_sha256);
    assert!(
        first[0].base_selected
            && first[0].base_enabled
            && second[0].base_selected
            && second[0].base_enabled
    );
    assert_ne!(first[0].selected, second[0].selected);
    assert_eq!(
        f.call(Action::Page {
            after: None,
            limit: 0,
            revisions
        })
        .status,
        Status::Limit
    );
    assert_eq!(
        f.call(Action::Page {
            after: None,
            limit: 17,
            revisions
        })
        .status,
        Status::Limit
    );
    assert_eq!(
        f.call(Action::Page {
            after: None,
            limit: 1,
            revisions: Revisions {
                catalog: revisions.catalog,
                manager: revisions.manager - 1
            }
        })
        .status,
        Status::Conflict
    );
}

#[test]
fn wrapper_removal_revokes_original_grants_but_keeps_explicit_base_decisions() {
    let mut f = Fixture::new(vec![]);
    f.activate();
    let bridge = f.connect().unwrap();
    let revoked = f.runtime.revocation(&bridge.shared_connection()).unwrap();
    let revisions = f.revisions();
    assert_eq!(
        f.call(Action::Remove {
            id: ID.into(),
            full_sha256: f.package.review_sha256(),
            revisions
        })
        .status,
        Status::Ok
    );
    assert!(revoked.is_revoked());
    assert_eq!(f.manager.revision(), revisions.manager);
    assert!(f.manager.selection(ID).unwrap().enabled);
    assert!(f.connect().is_err());
    f.close(&bridge);
}
