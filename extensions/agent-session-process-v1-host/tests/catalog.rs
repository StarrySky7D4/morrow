//! Durable wrapper approval under the original Manager and one ordinary Core runtime.
//! Real Rust Wasm is hash-pinned; logical cases do not qualify OS process isolation.
use morrow_agent_process_control_v1::{Capabilities as PC, host::Host as ProcessHost};
use morrow_agent_session_exec_v1_r2::{
    Action, Error, Outcome, Reply, Request, Result,
    authority::{Capabilities as SC, SessionExecHost},
    hash,
};
use morrow_agent_session_process_v1_host::{
    AgentProcessPackage, Declaration,
    catalog::{
        Approval, Catalog, CatalogManagedPackage, NativeCatalogStorage, Revisions,
        WrapperCatalogStorage,
    },
};
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{
        Package,
        catalog::Catalog as BaseCatalog,
        registry::{Registry, RegistryStorage, sqlite::SqliteRegistryStorage},
    },
    store::{EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{Limits, manager::Manager};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

const ID: &str = "org.example.durable-agent";
const EXPIRES: u64 = 100_000;
fn sc() -> SC {
    SC {
        session_read: true,
        session_write: true,
        propose: true,
        ..Default::default()
    }
}
fn pc() -> PC {
    PC {
        read: true,
        events: true,
        write: true,
        terminate: true,
        ..Default::default()
    }
}
fn limits() -> Limits {
    Limits {
        fuel: 100_000_000,
        memory_bytes: 16 * 1024 * 1024,
        host_calls: 16,
    }
}
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
fn wrapper(base: &Package, domain: &str) -> AgentProcessPackage {
    AgentProcessPackage::build(
        Package::decode(base.archive()).unwrap(),
        Declaration {
            session: sc(),
            process: pc(),
            sessions: vec!["session".into(), "session-child".into()],
            execution_domain: domain.into(),
        },
    )
    .unwrap()
}
fn approval() -> Approval {
    Approval {
        session: sc(),
        process: pc(),
        sessions: vec!["session".into(), "session-child".into()],
        execution_domain: "domain".into(),
    }
}
fn expected(catalog: &Catalog, manager: &Manager) -> Revisions {
    Revisions {
        catalog: catalog.revision(),
        manager: manager.revision(),
    }
}
struct Fixture {
    manager: Manager,
    runtime: HostRuntime,
    host: SessionExecHost,
    base: Package,
    package: AgentProcessPackage,
    processes: ProcessHost,
    temp: tempfile::TempDir,
}
impl Fixture {
    fn new(wasm: &[u8]) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base =
            Package::build(Package::manifest_for_task(ID, "1.0.0", wasm, vec![]), wasm).unwrap();
        let registry = Registry::open(
            &temp.path().join("original-registry"),
            BaseCatalog::open(&temp.path().join("original-packages")).unwrap(),
        )
        .unwrap();
        let mut manager = Manager::new(registry, limits());
        manager.install_package(base.archive()).unwrap();
        manager.select(&base, manager.revision()).unwrap();
        manager
            .set_enabled(ID, base.digest(), true, manager.revision())
            .unwrap();
        let mut runtime = HostRuntime::new(
            Store::open(&temp.path().join("ordinary.sqlite"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        let host = SessionExecHost::new(&mut runtime).unwrap();
        let package = wrapper(&base, "domain");
        Self {
            manager,
            runtime,
            host,
            base,
            package,
            processes: ProcessHost::default(),
            temp,
        }
    }
    fn root(&self) -> PathBuf {
        self.temp.path().join("wrapper-catalog")
    }
    fn install_select(&mut self, catalog: &mut Catalog) {
        let revision = expected(catalog, &self.manager);
        let review = catalog
            .install(
                self.package.archive(),
                self.package.review_sha256(),
                &mut self.manager,
                revision,
            )
            .unwrap();
        assert_eq!(review.wrapper_sha256, self.package.review_sha256());
        assert_eq!(review.base_sha256, self.base.digest());
        let revision = expected(catalog, &self.manager);
        catalog
            .select(self.package.review_sha256(), &mut self.manager, revision)
            .unwrap();
    }
    fn activate(&mut self, catalog: &mut Catalog) {
        self.install_select(catalog);
        let revision = expected(catalog, &self.manager);
        catalog
            .approve(
                ID,
                self.package.review_sha256(),
                approval(),
                &mut self.manager,
                revision,
            )
            .unwrap();
        let revision = expected(catalog, &self.manager);
        catalog
            .set_enabled(
                ID,
                self.package.review_sha256(),
                true,
                &mut self.manager,
                revision,
            )
            .unwrap();
    }
    fn connect(&mut self, catalog: &mut Catalog) -> Result<CatalogManagedPackage> {
        let revision = expected(catalog, &self.manager);
        catalog.connect(
            ID,
            self.package.review_sha256(),
            &mut self.manager,
            &mut self.runtime,
            &self.host,
            revision,
            EXPIRES,
            1,
        )
    }
    fn list(&mut self, bridge: &CatalogManagedPackage) -> Result<morrow_plugin_runtime::TaskRun> {
        let request =
            Request::new_for_generation("catalog-list", bridge.generation(), Action::List).unwrap();
        bridge.run(
            &self.manager,
            &mut self.runtime,
            &self.host,
            &mut self.processes,
            request.raw(),
            || 6,
        )
    }
    fn close(&mut self, bridge: &CatalogManagedPackage) {
        bridge
            .close(&mut self.runtime, &self.host, &mut self.processes)
            .unwrap();
    }
}

#[test]
fn inspect_install_and_duplicate_install_do_not_approve_or_change_base_selection() {
    let mut f = Fixture::new(&module());
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    let manager_revision = f.manager.revision();
    let base_snapshot = f.manager.persisted_snapshot().unwrap();
    let before = catalog.revision();
    let review = Catalog::inspect(f.package.archive()).unwrap();
    assert_eq!(review.id, ID);
    assert_eq!(review.wrapper_sha256, f.package.review_sha256());
    assert_eq!(
        review.session_schema,
        morrow_agent_session_exec_v1_r2::schema_digest()
    );
    assert_eq!(
        review.process_schema,
        morrow_agent_process_control_v1::schema_digest()
    );
    assert_eq!(catalog.revision(), before);
    let revision = expected(&catalog, &f.manager);
    catalog
        .install(
            f.package.archive(),
            f.package.review_sha256(),
            &mut f.manager,
            revision,
        )
        .unwrap();
    let installed_revision = catalog.revision();
    let revision = expected(&catalog, &f.manager);
    catalog
        .install(
            f.package.archive(),
            f.package.review_sha256(),
            &mut f.manager,
            revision,
        )
        .unwrap();
    assert_eq!(catalog.revision(), installed_revision);
    assert_eq!(f.manager.revision(), manager_revision);
    assert_eq!(f.manager.persisted_snapshot().unwrap(), base_snapshot);
    assert!(f.connect(&mut catalog).is_err());
}

#[test]
fn base_enable_never_substitutes_for_complete_wrapper_approval_and_enable() {
    let mut f = Fixture::new(&module());
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.install_select(&mut catalog);
    assert!(f.connect(&mut catalog).is_err());
    let revision = expected(&catalog, &f.manager);
    catalog
        .approve(
            ID,
            f.package.review_sha256(),
            approval(),
            &mut f.manager,
            revision,
        )
        .unwrap();
    assert!(f.connect(&mut catalog).is_err());
    let revision = expected(&catalog, &f.manager);
    catalog
        .set_enabled(
            ID,
            f.package.review_sha256(),
            true,
            &mut f.manager,
            revision,
        )
        .unwrap();
    let bridge = f.connect(&mut catalog).unwrap();
    let first = bridge.shared_connection();
    assert!(Arc::ptr_eq(&first, &bridge.shared_connection()));
    assert_eq!(first.package_digest(), Some(f.base.digest()));
    assert_eq!(bridge.limits().host_calls, limits().host_calls);
    let run = f.list(&bridge).unwrap();
    assert_eq!(run.report.outcome, Ok(0));
    let request =
        Request::new_for_generation("catalog-list", bridge.generation(), Action::List).unwrap();
    assert!(matches!(
        Reply::decode_for(&request, &run.completion.unwrap())
            .unwrap()
            .outcome,
        Outcome::Sessions(_)
    ));
    f.close(&bridge);
}

#[test]
fn invalid_digest_scope_or_stale_confirmation_cannot_stop_a_live_connection() {
    let mut f = Fixture::new(&module());
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.activate(&mut catalog);
    let bridge = f.connect(&mut catalog).unwrap();
    let connection = bridge.shared_connection();
    let revocation = f.runtime.revocation(&connection).unwrap();
    let original = expected(&catalog, &f.manager);
    for invalid in 0..5 {
        let mut value = approval();
        match invalid {
            0 => value.session.execute = true,
            1 => value.process.resize_pty = true,
            2 => value.sessions = vec!["session".into(), "session".into()],
            3 => value.sessions = vec!["foreign-session".into()],
            _ => value.execution_domain = "foreign-domain".into(),
        }
        let revision = expected(&catalog, &f.manager);
        assert!(
            catalog
                .approve(
                    ID,
                    f.package.review_sha256(),
                    value,
                    &mut f.manager,
                    revision
                )
                .is_err()
        );
        assert!(!revocation.is_revoked());
        assert!(f.list(&bridge).unwrap().report.outcome.is_ok());
    }
    for revision in [
        Revisions {
            catalog: original.catalog - 1,
            manager: original.manager,
        },
        Revisions {
            catalog: original.catalog,
            manager: original.manager - 1,
        },
    ] {
        assert!(
            catalog
                .set_enabled(
                    ID,
                    f.package.review_sha256(),
                    false,
                    &mut f.manager,
                    revision
                )
                .is_err()
        );
        assert!(!revocation.is_revoked());
    }
    let revision = expected(&catalog, &f.manager);
    assert!(
        catalog
            .approve(ID, [0; 32], approval(), &mut f.manager, revision)
            .is_err()
    );
    assert!(!revocation.is_revoked());
    assert_eq!(catalog.revision(), original.catalog);
    assert_eq!(f.manager.revision(), original.manager);
    f.close(&bridge);
}

#[test]
fn complete_wrapper_approval_reopens_but_live_connections_do_not_survive_catalog_drop() {
    let mut f = Fixture::new(&module());
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.activate(&mut catalog);
    let revision = catalog.revision();
    let bridge = f.connect(&mut catalog).unwrap();
    let original = bridge.shared_connection();
    let revocation = f.runtime.revocation(&original).unwrap();
    assert!(f.list(&bridge).unwrap().report.outcome.is_ok());
    drop(catalog);
    assert!(revocation.is_revoked());
    assert!(f.list(&bridge).is_err());
    let mut reopened = Catalog::open(&f.root(), false).unwrap();
    assert_eq!(reopened.revision(), revision);
    let page = reopened.page(None, 16, revision).unwrap();
    assert_eq!(page.entries.len(), 1);
    assert!(page.entries[0].selected && page.entries[0].enabled);
    assert_eq!(
        page.entries[0].review.wrapper_sha256,
        f.package.review_sha256()
    );
    assert!(
        page.entries[0]
            .approval
            .as_ref()
            .unwrap()
            .session
            .session_read
    );
    let revision = expected(&reopened, &f.manager);
    // The original issuer has already observed clock 6; a fresh admission cannot rewind it.
    let fresh = reopened
        .connect(
            ID,
            f.package.review_sha256(),
            &mut f.manager,
            &mut f.runtime,
            &f.host,
            revision,
            EXPIRES,
            6,
        )
        .unwrap();
    assert!(!Arc::ptr_eq(&original, &fresh.shared_connection()));
    assert!(f.list(&fresh).unwrap().report.outcome.is_ok());
    assert!(f.list(&bridge).is_err());
    f.close(&bridge);
    f.close(&fresh);
}

#[test]
fn wrapper_approval_disable_and_removal_revoke_without_publishing_base_decisions() {
    for mutation in 0..3 {
        let mut f = Fixture::new(&module());
        let mut catalog = Catalog::open(&f.root(), true).unwrap();
        f.activate(&mut catalog);
        let bridge = f.connect(&mut catalog).unwrap();
        let revocation = f.runtime.revocation(&bridge.shared_connection()).unwrap();
        let base_revision = f.manager.revision();
        let base_snapshot = f.manager.persisted_snapshot().unwrap();
        let revision = expected(&catalog, &f.manager);
        match mutation {
            0 => {
                let mut narrower = approval();
                narrower.process.write = false;
                catalog
                    .approve(
                        ID,
                        f.package.review_sha256(),
                        narrower,
                        &mut f.manager,
                        revision,
                    )
                    .unwrap();
            }
            1 => catalog
                .set_enabled(
                    ID,
                    f.package.review_sha256(),
                    false,
                    &mut f.manager,
                    revision,
                )
                .unwrap(),
            _ => catalog
                .remove(ID, f.package.review_sha256(), &mut f.manager, revision)
                .unwrap(),
        }
        assert!(revocation.is_revoked());
        assert!(f.list(&bridge).is_err());
        assert_eq!(f.manager.revision(), base_revision);
        assert_eq!(f.manager.persisted_snapshot().unwrap(), base_snapshot);
        assert!(f.manager.selection(ID).unwrap().enabled);
        f.close(&bridge);
    }
}

#[test]
fn selecting_another_wrapper_of_the_same_base_revokes_and_requires_fresh_review() {
    let mut f = Fixture::new(&module());
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.activate(&mut catalog);
    let bridge = f.connect(&mut catalog).unwrap();
    let revocation = f.runtime.revocation(&bridge.shared_connection()).unwrap();
    let manager_revision = f.manager.revision();
    let next = wrapper(&f.base, "another-domain");
    let revision = expected(&catalog, &f.manager);
    catalog
        .install(
            next.archive(),
            next.review_sha256(),
            &mut f.manager,
            revision,
        )
        .unwrap();
    let revision = expected(&catalog, &f.manager);
    catalog
        .select(next.review_sha256(), &mut f.manager, revision)
        .unwrap();
    assert!(revocation.is_revoked());
    assert_eq!(f.manager.revision(), manager_revision);
    let revision = expected(&catalog, &f.manager);
    assert!(
        catalog
            .connect(
                ID,
                next.review_sha256(),
                &mut f.manager,
                &mut f.runtime,
                &f.host,
                revision,
                EXPIRES,
                1
            )
            .is_err()
    );
    assert!(f.connect(&mut catalog).is_err());
    f.close(&bridge);
}

struct FaultStorage {
    original: NativeCatalogStorage,
    mode: Arc<AtomicU8>,
}
impl WrapperCatalogStorage for FaultStorage {
    fn read(&self) -> Result<Option<Vec<u8>>> {
        self.original.read()
    }
    fn load(&self, digest: [u8; 32]) -> Result<AgentProcessPackage> {
        self.original.load(digest)
    }
    fn install(&self, package: &AgentProcessPackage) -> Result<()> {
        self.original.install(package)
    }
    fn publish(&mut self, bytes: &[u8]) -> Result<()> {
        match self.mode.load(Ordering::SeqCst) {
            1 => Err(Error::Storage),
            2 => {
                self.original.publish(bytes)?;
                Err(Error::CommitUnknown)
            }
            _ => self.original.publish(bytes),
        }
    }
}
fn fault_catalog(f: &Fixture, mode: Arc<AtomicU8>) -> Catalog {
    Catalog::from_storage(Box::new(FaultStorage {
        original: NativeCatalogStorage::open(&f.root(), true).unwrap(),
        mode,
    }))
    .unwrap()
}

#[test]
fn known_save_failure_retains_disk_decision_but_never_revives_stopped_connections() {
    let mut f = Fixture::new(&module());
    let mode = Arc::new(AtomicU8::new(0));
    let mut catalog = fault_catalog(&f, mode.clone());
    f.activate(&mut catalog);
    let bridge = f.connect(&mut catalog).unwrap();
    let connection = bridge.shared_connection();
    let revocation = f.runtime.revocation(&connection).unwrap();
    let revision = expected(&catalog, &f.manager);
    mode.store(1, Ordering::SeqCst);
    assert!(matches!(
        catalog.set_enabled(
            ID,
            f.package.review_sha256(),
            false,
            &mut f.manager,
            revision
        ),
        Err(Error::Storage)
    ));
    assert!(revocation.is_revoked());
    assert!(f.list(&bridge).is_err());
    assert_eq!(catalog.revision(), revision.catalog);
    f.close(&bridge);
    drop(catalog);
    let mut reopened = Catalog::open(&f.root(), false).unwrap();
    let page = reopened.page(None, 16, reopened.revision()).unwrap();
    assert!(page.entries[0].enabled);
    let fresh = f.connect(&mut reopened).unwrap();
    assert!(!Arc::ptr_eq(&connection, &fresh.shared_connection()));
    assert!(f.list(&fresh).unwrap().report.outcome.is_ok());
    f.close(&fresh);
}

#[test]
fn committed_unknown_blocks_noops_and_execution_until_reopen_reconciles_disk() {
    let mut f = Fixture::new(&module());
    let mode = Arc::new(AtomicU8::new(0));
    let mut catalog = fault_catalog(&f, mode.clone());
    f.activate(&mut catalog);
    let bridge = f.connect(&mut catalog).unwrap();
    let connection = bridge.shared_connection();
    let revocation = f.runtime.revocation(&connection).unwrap();
    let revision = expected(&catalog, &f.manager);
    mode.store(2, Ordering::SeqCst);
    assert!(matches!(
        catalog.set_enabled(
            ID,
            f.package.review_sha256(),
            false,
            &mut f.manager,
            revision
        ),
        Err(Error::CommitUnknown)
    ));
    assert!(revocation.is_revoked());
    assert!(f.list(&bridge).is_err());
    assert!(matches!(
        catalog.page(None, 16, revision.catalog),
        Err(Error::CommitUnknown)
    ));
    assert!(matches!(
        catalog.set_enabled(
            ID,
            f.package.review_sha256(),
            true,
            &mut f.manager,
            revision
        ),
        Err(Error::CommitUnknown)
    ));
    assert!(matches!(f.connect(&mut catalog), Err(Error::CommitUnknown)));
    f.close(&bridge);
    drop(catalog);
    let mut reopened = Catalog::open(&f.root(), false).unwrap();
    assert_eq!(reopened.revision(), revision.catalog + 1);
    let page = reopened.page(None, 16, reopened.revision()).unwrap();
    assert!(!page.entries[0].enabled);
    assert!(f.connect(&mut reopened).is_err());
    let revision = expected(&reopened, &f.manager);
    reopened
        .set_enabled(
            ID,
            f.package.review_sha256(),
            true,
            &mut f.manager,
            revision,
        )
        .unwrap();
    let fresh = f.connect(&mut reopened).unwrap();
    assert!(!Arc::ptr_eq(&connection, &fresh.shared_connection()));
    assert!(f.list(&fresh).unwrap().report.outcome.is_ok());
    f.close(&fresh);
}

#[test]
fn malformed_or_wrong_sha_wrapper_is_not_installed_or_approved() {
    let mut f = Fixture::new(&module());
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.activate(&mut catalog);
    let bridge = f.connect(&mut catalog).unwrap();
    let revocation = f.runtime.revocation(&bridge.shared_connection()).unwrap();
    let before = expected(&catalog, &f.manager);
    let mut corrupt = f.package.archive().to_vec();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 0x80;
    assert!(Catalog::inspect(&corrupt).is_err());
    let revision = expected(&catalog, &f.manager);
    assert!(
        catalog
            .install(
                &corrupt,
                f.package.review_sha256(),
                &mut f.manager,
                revision
            )
            .is_err()
    );
    let revision = expected(&catalog, &f.manager);
    assert!(
        catalog
            .install(f.package.archive(), [0; 32], &mut f.manager, revision)
            .is_err()
    );
    assert_eq!(catalog.revision(), before.catalog);
    assert_eq!(f.manager.revision(), before.manager);
    assert!(!revocation.is_revoked());
    assert!(f.list(&bridge).unwrap().report.outcome.is_ok());
    f.close(&bridge);
}

fn large_module() -> Vec<u8> {
    let mut bytes = module();
    let mut payload = vec![0]; // Empty custom-section name; remaining bytes are arbitrary data.
    let mut state = 0x9e37_79b9u32;
    for _ in 0..700_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        payload.push(state as u8);
    }
    bytes.push(0); // Custom section ID.
    let mut remaining = payload.len();
    loop {
        let part = (remaining & 0x7f) as u8;
        remaining >>= 7;
        bytes.push(part | if remaining == 0 { 0 } else { 0x80 });
        if remaining == 0 {
            break;
        }
    }
    bytes.extend(payload);
    bytes
}

#[test]
fn legal_wrapper_larger_than_snapshot_budget_reopens_without_losing_archive_bytes() {
    let mut f = Fixture::new(&large_module());
    assert!(f.package.archive().len() > 512 * 1024);
    let digest = f.package.review_sha256();
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.activate(&mut catalog);
    let revision = catalog.revision();
    drop(catalog);
    let original = NativeCatalogStorage::open(&f.root(), false).unwrap();
    let loaded = original.load(digest).unwrap();
    assert_eq!(loaded.archive(), f.package.archive());
    assert_eq!(loaded.review_sha256(), digest);
    let snapshot = original.read().unwrap().unwrap();
    assert!(
        snapshot.len() <= 512 * 1024,
        "original metadata limit remains unchanged"
    );
    drop(original);
    let mut reopened = Catalog::open(&f.root(), false).unwrap();
    assert_eq!(reopened.revision(), revision);
    let bridge = f.connect(&mut reopened).unwrap();
    assert!(f.list(&bridge).unwrap().report.outcome.is_ok());
    f.close(&bridge);
}

#[test]
fn durable_approval_executes_sealed_rust_session_wasm_on_the_same_original_runtime() {
    let path = PathBuf::from(
        std::env::var_os("MORROW_CODEX_COMBINED_GUEST")
            .expect("explicit sealed Rust session guest required"),
    );
    assert!(path.is_absolute());
    let bytes = std::fs::read(path).unwrap();
    let actual = hash(&bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    assert_eq!(
        actual,
        "b1f0f44ad2bbe2abc89eee0ee971683419bf3407f370194f5557df57fa45799e"
    );
    let mut f = Fixture::new(&bytes);
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.activate(&mut catalog);
    let revision = catalog.revision();
    drop(catalog);
    let mut reopened = Catalog::open(&f.root(), false).unwrap();
    assert_eq!(reopened.revision(), revision);
    let bridge = f.connect(&mut reopened).unwrap();
    let original = bridge.shared_connection();
    let mut config = bridge.generation().to_le_bytes().to_vec();
    config.extend_from_slice(&[7; 16]);
    config.extend_from_slice(b"session");
    let task = Invocation::new_transform(
        "durable-managed-session",
        Transform {
            handler: "codex.session.continue".into(),
            input_type: "codex.session.config.v1".into(),
            output_type: "codex.session.receipt.v1".into(),
            input: config,
        },
    )
    .unwrap();
    let run = bridge
        .run(
            &f.manager,
            &mut f.runtime,
            &f.host,
            &mut f.processes,
            task.bytes(),
            || 6,
        )
        .unwrap();
    assert_eq!(run.report.outcome, Ok(0));
    assert_eq!(run.report.host_calls, 7);
    let output = task.verify_output(&run.completion.unwrap()).unwrap();
    let mut expected = 1u64.to_le_bytes().to_vec();
    expected.extend_from_slice(&hash(b"sealed-state\0\xff"));
    expected.extend_from_slice(&1u64.to_le_bytes());
    assert_eq!(output.bytes, expected);
    assert!(Arc::ptr_eq(&original, &bridge.shared_connection()));
    assert_eq!(original.package_digest(), Some(f.base.digest()));
    f.close(&bridge);
}

#[test]
fn catalog_pagination_uses_its_own_revision_and_bounds_without_mutating_decisions() {
    let mut f = Fixture::new(&module());
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.activate(&mut catalog);
    let other = wrapper(&f.base, "other-domain");
    let revision = expected(&catalog, &f.manager);
    catalog
        .install(
            other.archive(),
            other.review_sha256(),
            &mut f.manager,
            revision,
        )
        .unwrap();
    let revision = catalog.revision();
    assert!(catalog.page(None, 0, revision).is_err());
    assert!(catalog.page(None, 17, revision).is_err());
    assert!(catalog.page(None, 1, revision - 1).is_err());
    assert!(catalog.page(Some("invalid-cursor"), 1, revision).is_err());
    let first = catalog.page(None, 1, revision).unwrap();
    assert_eq!(first.entries.len(), 1);
    let second = catalog.page(first.next.as_deref(), 1, revision).unwrap();
    assert_eq!(second.entries.len(), 1);
    assert_ne!(
        first.entries[0].review.wrapper_sha256,
        second.entries[0].review.wrapper_sha256
    );
    assert!(second.next.is_none());
    assert_eq!(catalog.revision(), revision);
}

#[test]
fn replacement_or_corruption_of_an_installed_wrapper_cannot_reopen_approval() {
    let mut f = Fixture::new(&module());
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.activate(&mut catalog);
    drop(catalog);
    let name = hash(f.package.archive())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let artifact = f.root().join("wrappers").join(format!("{name}.magent"));
    let original = std::fs::read(&artifact).unwrap();
    assert_eq!(original, f.package.archive());
    let mut corrupt = original.clone();
    corrupt[0] ^= 1;
    std::fs::write(&artifact, &corrupt).unwrap();
    assert!(Catalog::open(&f.root(), false).is_err());
    std::fs::write(&artifact, &original).unwrap();
    let mut reopened = Catalog::open(&f.root(), false).unwrap();
    let bridge = f.connect(&mut reopened).unwrap();
    assert!(f.list(&bridge).unwrap().report.outcome.is_ok());
    f.close(&bridge);
}

#[test]
fn catalog_drop_at_last_r2_precommit_clock_cannot_create_a_session() {
    let mut f = Fixture::new(&module());
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.activate(&mut catalog);
    let bridge = f.connect(&mut catalog).unwrap();
    let connection = bridge.shared_connection();
    let revocation = f.runtime.revocation(&connection).unwrap();
    let mut original_catalog = Some(catalog);
    let request = Request::new_for_generation(
        "catalog-cancel-before-commit",
        bridge.generation(),
        Action::Create {
            session_id: "session".into(),
            parent: None,
            parent_tail: 0,
        },
    )
    .unwrap();
    let domain = hash(b"morrow/agent-session-exec-v1/session/state/2");
    let before = f
        .runtime
        .store_local()
        .load_agent_ledger_local(&domain, "session")
        .unwrap()
        .map(|r| (r.revision(), r.payload().to_vec()));
    let mut samples = 0;
    let run = bridge
        .run(
            &f.manager,
            &mut f.runtime,
            &f.host,
            &mut f.processes,
            request.raw(),
            || {
                samples += 1;
                if samples == 2 {
                    drop(original_catalog.take());
                }
                6
            },
        )
        .unwrap();
    assert_eq!(samples, 2);
    assert!(run.report.outcome.is_err());
    assert!(run.completion.is_none());
    assert!(revocation.is_revoked());
    let after = f
        .runtime
        .store_local()
        .load_agent_ledger_local(&domain, "session")
        .unwrap()
        .map(|r| (r.revision(), r.payload().to_vec()));
    assert_eq!(
        after, before,
        "catalog loss must prevent the original durable write"
    );
    f.close(&bridge);
}

#[test]
fn persisted_metadata_does_not_authorize_an_old_connection_under_another_manager() {
    let mut f = Fixture::new(&module());
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.activate(&mut catalog);
    let bridge = f.connect(&mut catalog).unwrap();
    let other = Fixture::new(&module());
    assert_eq!(other.base.digest(), f.base.digest());
    assert_eq!(other.manager.revision(), f.manager.revision());
    let request =
        Request::new_for_generation("foreign-manager", bridge.generation(), Action::List).unwrap();
    assert!(
        bridge
            .run(
                &other.manager,
                &mut f.runtime,
                &f.host,
                &mut f.processes,
                request.raw(),
                || 6
            )
            .is_err()
    );
    assert!(f.list(&bridge).unwrap().report.outcome.is_ok());
    f.manager
        .set_enabled(ID, f.base.digest(), false, f.manager.revision())
        .unwrap();
    assert!(f.list(&bridge).is_err());
    assert!(f.connect(&mut catalog).is_err());
    f.close(&bridge);
}

struct UnknownManagerInstallStorage {
    original: SqliteRegistryStorage,
    fail_install: Arc<AtomicU8>,
}
impl RegistryStorage for UnknownManagerInstallStorage {
    fn read(&self) -> morrow_core::Result<Option<Vec<u8>>> {
        self.original.read()
    }
    fn publish(&mut self, bytes: &[u8]) -> morrow_core::Result<()> {
        self.original.publish(bytes)
    }
    fn load_package(&self, digest: [u8; 32]) -> morrow_core::Result<Package> {
        self.original.load_package(digest)
    }
    fn install_package(&self, package: &Package) -> morrow_core::Result<()> {
        self.original.install_package(package)?;
        if self.fail_install.load(Ordering::SeqCst) != 0 {
            Err(morrow_core::Error::CommitUnknown)
        } else {
            Ok(())
        }
    }
}

fn manager_install_fault_fixture() -> (Fixture, Arc<AtomicU8>) {
    let mut f = Fixture::new(&module());
    let fail_install = Arc::new(AtomicU8::new(0));
    let registry = Registry::from_storage(Box::new(UnknownManagerInstallStorage {
        original: SqliteRegistryStorage::open(
            &f.temp.path().join("original-manager-registry.sqlite"),
            true,
        )
        .unwrap(),
        fail_install: fail_install.clone(),
    }))
    .unwrap();
    // Replace the unused fixture Manager before making any connection. Keep the
    // same original Core Store/runtime and wrap the real registry adapter only.
    f.manager = Manager::new(registry, limits());
    f.manager.install_package(f.base.archive()).unwrap();
    f.manager.select(&f.base, f.manager.revision()).unwrap();
    f.manager
        .set_enabled(ID, f.base.digest(), true, f.manager.revision())
        .unwrap();
    (f, fail_install)
}

#[test]
fn manager_install_commit_unknown_immediately_revokes_original_core_and_r2_authority() {
    let (mut f, fail_install) = manager_install_fault_fixture();
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.activate(&mut catalog);
    let bridge = f.connect(&mut catalog).unwrap();
    let connection = bridge.shared_connection();
    let revocation = f.runtime.revocation(&connection).unwrap();
    let admission = f
        .host
        .admit(
            &f.runtime,
            &connection,
            sc(),
            sc(),
            vec!["session".into()],
            "domain".into(),
            EXPIRES,
            1,
        )
        .unwrap();
    let request = Request::new_for_generation(
        "original-r2-after-manager-unknown",
        bridge.generation(),
        Action::List,
    )
    .unwrap();
    let before = f
        .host
        .dispatch(&mut f.runtime, &connection, &admission, request.raw(), || 6)
        .unwrap();
    assert!(matches!(
        Reply::decode_for(&request, &before).unwrap().outcome,
        Outcome::Sessions(_)
    ));
    let next = wrapper(&f.base, "another-domain");
    let revision = expected(&catalog, &f.manager);
    fail_install.store(1, Ordering::SeqCst);
    assert!(matches!(
        catalog.install(
            next.archive(),
            next.review_sha256(),
            &mut f.manager,
            revision,
        ),
        Err(Error::CommitUnknown)
    ));
    assert!(revocation.is_revoked());
    assert_ne!(
        f.runtime.connection_phase(&connection),
        Ok(morrow_core::lifecycle::InstancePhase::Ready)
    );
    // Bypass CatalogManagedPackage's own ready check: the original R2 admission
    // and original Arc<Connection> themselves must already have lost authority.
    let after = f
        .host
        .dispatch(&mut f.runtime, &connection, &admission, request.raw(), || 6)
        .unwrap();
    assert!(matches!(
        Reply::decode_for(&request, &after).unwrap().outcome,
        Outcome::Rejected(Error::Denied)
    ));
    assert!(matches!(
        f.manager.installed_package(f.base.digest()),
        Err(morrow_plugin_runtime::manager::ManagerError::Core(
            morrow_core::Error::CommitUnknown
        ))
    ));
    assert!(matches!(
        catalog.page(None, 16, revision.catalog),
        Err(Error::CommitUnknown)
    ));
    assert!(matches!(f.connect(&mut catalog), Err(Error::CommitUnknown)));
    f.close(&bridge);
}

#[test]
fn externally_poisoned_manager_load_revokes_old_core_and_preserves_unknown() {
    let (mut f, fail_install) = manager_install_fault_fixture();
    let mut catalog = Catalog::open(&f.root(), true).unwrap();
    f.activate(&mut catalog);
    let bridge = f.connect(&mut catalog).unwrap();
    let connection = bridge.shared_connection();
    let revocation = f.runtime.revocation(&connection).unwrap();
    let revision = expected(&catalog, &f.manager);
    fail_install.store(1, Ordering::SeqCst);
    assert!(matches!(
        f.manager.install_package(f.base.archive()),
        Err(morrow_plugin_runtime::manager::ManagerError::Core(
            morrow_core::Error::CommitUnknown
        ))
    ));
    // The original Registry's uncertainty alone does not revoke original Core
    // authority. Catalog's current-base check must perform that stop itself.
    assert!(!revocation.is_revoked());
    assert_eq!(
        f.runtime.connection_phase(&connection),
        Ok(morrow_core::lifecycle::InstancePhase::Ready)
    );
    assert!(matches!(f.connect(&mut catalog), Err(Error::CommitUnknown)));
    assert!(revocation.is_revoked());
    assert_ne!(
        f.runtime.connection_phase(&connection),
        Ok(morrow_core::lifecycle::InstancePhase::Ready)
    );
    assert!(matches!(
        catalog.set_enabled(
            ID,
            f.package.review_sha256(),
            true,
            &mut f.manager,
            revision,
        ),
        Err(Error::CommitUnknown)
    ));
    assert!(matches!(
        catalog.page(None, 16, revision.catalog),
        Err(Error::CommitUnknown)
    ));
    f.close(&bridge);
}

#[test]
fn persisted_revision_zero_is_rejected_while_absent_snapshot_is_fresh() {
    let f = Fixture::new(&module());
    let storage = NativeCatalogStorage::open(&f.root(), true).unwrap();
    assert!(storage.read().unwrap().is_none());
    let fresh = Catalog::from_storage(Box::new(storage)).unwrap();
    assert_eq!(fresh.revision(), 0);
    drop(fresh);
    // Canonical protobuf Snapshot{version:1, revision:0, packages:[], selections:[]}.
    // Revision zero is omitted by protobuf; its envelope and hash are valid.
    let raw = [0x08, 0x01];
    let compressed = lz4_flex::block::compress(&raw);
    let mut bytes = b"MROWAC01".to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&hash(&raw));
    bytes.extend_from_slice(&compressed);
    let mut storage = NativeCatalogStorage::open(&f.root(), false).unwrap();
    storage.publish(&bytes).unwrap();
    assert_eq!(storage.read().unwrap(), Some(bytes));
    drop(storage);
    assert!(matches!(
        Catalog::open(&f.root(), false),
        Err(Error::Contract)
    ));
}
