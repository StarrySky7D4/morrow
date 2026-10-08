//! Explicit-profile approval on original ordinary SQLite. No OS/ProtectedSession qualification.
use morrow_agent_session_exec_v1_r2::{Action, Request, authority::{Capabilities, SessionExecHost}, hash};
use morrow_agent_session_process_v1_host::{AgentProcessPackage, Declaration,
    catalog::{Approval, Catalog, CatalogManagedSessionExecPackage, Revisions}};
use morrow_core::{dispatch::HostRuntime, lifecycle::InstancePhase,
    plugin_package::{Package, catalog::Catalog as BaseCatalog, registry::Registry},
    store::{EventBudget, Store}};
use morrow_plugin_runtime::{Limits, manager::Manager};

const ID: &str = "org.example.explicit-r2-profile";
fn capabilities() -> Capabilities {
    Capabilities { session_read: true, session_write: true, propose: true, ..Default::default() }
}
fn wasm(import: &str) -> Vec<u8> {
    wat::parse_str(format!(r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (import "{import}" "call" (func $call (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 6)
      (func (export "morrow_run") (result i32) (local $n i32)
        (local.set $n (call $read (i32.const 0) (i32.const 131072)))
        (local.set $n (call $call (i32.const 0) (local.get $n) (i32.const 131072) (i32.const 131072)))
        (drop (call $done (i32.const 131072) (local.get $n))) i32.const 0))"#)).unwrap()
}
struct Fixture {
    temp: tempfile::TempDir, manager: Manager, runtime: HostRuntime,
    host: SessionExecHost, package: AgentProcessPackage,
}
impl Fixture {
    fn new(import: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let wasm = wasm(import);
        let base = Package::build(Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]), &wasm).unwrap();
        let registry = Registry::open(&temp.path().join("registry"),
            BaseCatalog::open(&temp.path().join("base")).unwrap()).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.install_package(base.archive()).unwrap();
        manager.select(&base, manager.revision()).unwrap();
        manager.set_enabled(ID, base.digest(), true, manager.revision()).unwrap();
        let mut runtime = HostRuntime::new(Store::open(&temp.path().join("ordinary.sqlite"),
            EventBudget::default()).unwrap()).unwrap();
        let host = SessionExecHost::new(&mut runtime).unwrap();
        let package = AgentProcessPackage::build(base, Declaration { session: capabilities(),
            process: Default::default(), sessions: vec!["session".into()],
            execution_domain: "domain".into() }).unwrap();
        Self { temp, manager, runtime, host, package }
    }
    fn revisions(&self, catalog: &Catalog) -> Revisions {
        Revisions { catalog: catalog.revision(), manager: self.manager.revision() }
    }
    fn activate(&mut self, catalog: &mut Catalog) {
        let expected = self.revisions(catalog);
        catalog.install_session_exec(self.package.archive(), self.package.review_sha256(),
            &mut self.manager, expected).unwrap();
        let expected = self.revisions(catalog);
        catalog.select(self.package.review_sha256(), &mut self.manager, expected).unwrap();
        let expected = self.revisions(catalog);
        catalog.approve(ID, self.package.review_sha256(), Approval { session: capabilities(),
            process: Default::default(), sessions: vec!["session".into()], execution_domain: "domain".into() },
            &mut self.manager, expected).unwrap();
        let expected = self.revisions(catalog);
        catalog.set_enabled(ID, self.package.review_sha256(), true, &mut self.manager, expected).unwrap();
    }
    fn connect(&mut self, catalog: &mut Catalog) -> CatalogManagedSessionExecPackage {
        let expected = self.revisions(catalog);
        catalog.connect_session_exec(ID, self.package.review_sha256(), &mut self.manager,
            &mut self.runtime, &self.host, expected, 60_000, 1).unwrap()
    }
}

#[test]
fn explicit_r2_inspection_never_falls_back_to_process_profile() {
    let r2 = Fixture::new("morrow_agent_session_exec_v1");
    assert!(Catalog::inspect(r2.package.archive()).is_err());
    assert_eq!(Catalog::inspect_session_exec(r2.package.archive()).unwrap().wrapper_sha256,
        r2.package.review_sha256());
    let process = Fixture::new("morrow_agent_session_process_v1");
    assert!(Catalog::inspect(process.package.archive()).is_ok());
    assert!(Catalog::inspect_session_exec(process.package.archive()).is_err());
}

#[test]
fn r2_install_is_not_approval_and_reopen_preserves_complete_review() {
    let mut f = Fixture::new("morrow_agent_session_exec_v1");
    let root = f.temp.path().join("wrappers");
    let mut catalog = Catalog::open(&root, true).unwrap();
    let expected = f.revisions(&catalog);
    catalog.install_session_exec(f.package.archive(), f.package.review_sha256(), &mut f.manager, expected).unwrap();
    let expected = f.revisions(&catalog);
    assert!(catalog.connect_session_exec(ID, f.package.review_sha256(), &mut f.manager,
        &mut f.runtime, &f.host, expected, 60_000, 1).is_err());
    f.activate(&mut catalog);
    drop(catalog);
    let mut reopened = Catalog::open(&root, false).unwrap();
    let bridge = f.connect(&mut reopened);
    assert_eq!(bridge.package().archive(), f.package.archive());
    let expected = f.revisions(&reopened);
    assert!(reopened.connect(ID, f.package.review_sha256(), &mut f.manager,
        &mut f.runtime, &f.host, expected,
        60_000, 1).is_err());
    bridge.close(&mut f.runtime, &f.host).unwrap();
}

#[test]
fn catalog_drop_and_manager_disable_revoke_original_r2_connections() {
    let mut f = Fixture::new("morrow_agent_session_exec_v1");
    let mut catalog = Catalog::open(&f.temp.path().join("wrappers"), true).unwrap();
    f.activate(&mut catalog);
    let bridge = f.connect(&mut catalog);
    let connection = bridge.shared_connection();
    assert_eq!(f.runtime.connection_phase(&connection).unwrap(), InstancePhase::Ready);
    let revision = f.manager.revision();
    f.manager.set_enabled(ID, f.package.base_sha256(), false, revision).unwrap();
    assert!(f.runtime.revocation(&connection).unwrap().is_revoked());
    bridge.close(&mut f.runtime, &f.host).unwrap();
    let revision = f.manager.revision();
    f.manager.set_enabled(ID, f.package.base_sha256(), true, revision).unwrap();
    let bridge = f.connect(&mut catalog);
    let revocation = f.runtime.revocation(&bridge.shared_connection()).unwrap();
    drop(catalog);
    assert!(revocation.is_revoked());
    bridge.close(&mut f.runtime, &f.host).unwrap();
}

#[test]
fn r2_catalog_loss_at_last_precommit_sample_cannot_publish_session() {
    let mut f = Fixture::new("morrow_agent_session_exec_v1");
    let mut catalog = Catalog::open(&f.temp.path().join("wrappers"), true).unwrap();
    f.activate(&mut catalog);
    let bridge = f.connect(&mut catalog);
    let mut retained = Some(catalog);
    let request = Request::new_for_generation("r2-profile-precommit", bridge.generation(),
        Action::Create { session_id: "session".into(), parent: None, parent_tail: 0 }).unwrap();
    let domain = hash(b"morrow/agent-session-exec-v1/session/state/2");
    assert!(f.runtime.store_local().load_agent_ledger_local(&domain, "session").unwrap().is_none());
    let mut samples = 0;
    let run = bridge.run(&f.manager, &mut f.runtime, &f.host, request.raw(), || {
        samples += 1; if samples == 2 { drop(retained.take()); } 6
    }).unwrap();
    assert_eq!(samples, 2);
    assert!(run.report.outcome.is_err());
    assert!(run.completion.is_none());
    assert!(f.runtime.store_local().load_agent_ledger_local(&domain, "session").unwrap().is_none());
    bridge.close(&mut f.runtime, &f.host).unwrap();
}
