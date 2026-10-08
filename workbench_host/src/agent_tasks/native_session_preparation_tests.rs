// Ordinary real Core + canonical admin + original Manager/CatalogOwner only.
// No ProtectedSession, DPAPI, native backend, OS child or replacement runtime.
use super::*;
use morrow_agent_catalog_admin_v1 as admin;
use morrow_agent_session_process_v1_host::{AgentProcessPackage, Declaration};
use morrow_core::{
    lifecycle::InstancePhase,
    plugin_package::{Package, catalog::Catalog as BaseCatalog, registry::Registry},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{Limits, manager::Manager};
use std::time::Duration;

const ID: &str = "org.example.c27-native-preparation";
fn ordinary() -> (
    tempfile::TempDir,
    HostRuntime,
    Manager,
    crate::agent_catalog::Slot,
    Arc<SessionExecHost>,
    [u8; 32],
    admin::Revisions,
) {
    let temp = tempfile::tempdir().unwrap();
    let wasm = wat::parse_str(r#"(module
        (import "morrow_task_v1" "read_input" (func (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func (param i32 i32) (result i32)))
        (import "morrow_agent_session_process_v1" "call" (func (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 0))"#).unwrap();
    let base = Package::build(
        Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]),
        &wasm,
    )
    .unwrap();
    let registry = Registry::open(
        &temp.path().join("registry"),
        BaseCatalog::open(&temp.path().join("packages")).unwrap(),
    )
    .unwrap();
    let mut manager = Manager::new(
        registry,
        Limits {
            fuel: 100_000_000,
            memory_bytes: 16 * 1024 * 1024,
            host_calls: 16,
        },
    );
    manager.install_package(base.archive()).unwrap();
    manager.select(&base, manager.revision()).unwrap();
    manager
        .set_enabled(ID, base.digest(), true, manager.revision())
        .unwrap();
    let rights = Capabilities {
        session_read: true,
        session_write: true,
        ..Default::default()
    };
    let wrapper = AgentProcessPackage::build(
        Package::decode(base.archive()).unwrap(),
        Declaration {
            session: rights,
            process: Default::default(),
            sessions: vec!["session".into()],
            execution_domain: "domain".into(),
        },
    )
    .unwrap();
    let digest = wrapper.review_sha256();
    let path = temp.path().join("approved-wrapper.bin");
    std::fs::write(&path, wrapper.archive()).unwrap();
    let mut slot = crate::agent_catalog::Slot::new(temp.path().join("wrapper-catalog"));
    let catalog = slot.opened(vec![]).unwrap();
    for n in 1u8..=4 {
        let revisions = catalog.revisions(&manager);
        let action = match n {
            1 => admin::Action::Install {
                path: path.to_str().unwrap().into(),
                full_sha256: digest,
                revisions,
            },
            2 => admin::Action::WrapperSelect {
                full_sha256: digest,
                revisions,
            },
            3 => admin::Action::Approve {
                id: ID.into(),
                full_sha256: digest,
                revisions,
                approval: admin::Approval {
                    session_bits: 3,
                    process_bits: 0,
                    sessions: vec!["session".into()],
                    domain: "domain".into(),
                },
            },
            4 => admin::Action::WrapperEnable {
                id: ID.into(),
                full_sha256: digest,
                revisions,
                enabled: true,
            },
            _ => unreachable!(),
        };
        let request = admin::Request {
            id: [n; 16],
            action,
        };
        // Actual public admin validation, canonical framing and correlated response.
        let raw = admin::respond(
            &mut CatalogTarget {
                catalog: &mut *catalog,
                manager: &mut manager,
            },
            &request.encode().unwrap(),
        )
        .unwrap();
        let reply = admin::Reply::decode_for(&request, &raw).unwrap();
        assert_eq!(reply.outcome.status, admin::Status::Ok);
    }
    let revisions = catalog.revisions(&manager);
    let mut runtime = HostRuntime::new(
        Store::open(&temp.path().join("ordinary.sqlite"), EventBudget::default()).unwrap(),
    )
    .unwrap();
    let host = Arc::new(SessionExecHost::new(&mut runtime).unwrap());
    (temp, runtime, manager, slot, host, digest, revisions)
}
struct CatalogTarget<'a> {
    catalog: &'a mut morrow_agent_catalog_owner_v1::CatalogOwner,
    manager: &'a mut Manager,
}
impl admin::Target for CatalogTarget<'_> {
    fn execute(&mut self, request: &admin::Request) -> admin::Outcome {
        self.catalog.execute(self.manager, request)
    }
}
fn prepare_executor(preparation: &mut AgentPreparation<'_>) -> Arc<Connection> {
    let host = preparation.session_host().unwrap();
    let connection = preparation.connect().unwrap();
    let caps = Capabilities {
        session_read: true,
        execute: true,
        ..Default::default()
    };
    let expires = preparation.expires_after(30_000).unwrap();
    preparation
        .admit(
            &host,
            &connection,
            caps,
            caps,
            vec!["session".into()],
            "domain".into(),
            expires,
        )
        .unwrap();
    connection
}

#[test]
fn genuine_preparation_tracks_independently_approved_native_session_package() {
    let (_temp, mut runtime, mut manager, mut slot, host, digest, revisions) = ordinary();
    let binding = runtime.binding();
    let ledger = super::super::context_lifecycle::ContextLifecycleLedger::new(binding);
    let mut preparation = AgentPreparation::with_catalog(
        &mut runtime,
        Instant::now() - Duration::from_millis(10),
        Some(&mut manager),
        Some(&mut slot),
        vec![],
        host,
        ledger.clone(),
    );
    let executor = prepare_executor(&mut preparation);
    let expires = preparation.expires_after(30_000).unwrap();
    let package = preparation
        .prepare_native_session_package(ID, digest, revisions, expires)
        .unwrap();
    let secondary = preparation
        .debt
        .native_session_package
        .as_ref()
        .unwrap()
        .shared_connection();
    assert!(!Arc::ptr_eq(&executor, &secondary));
    assert!(
        preparation
            .prepare_native_session_package(ID, digest, revisions, expires)
            .is_err(),
        "factory reminted authority"
    );
    let token = preparation
        .context_with_prepared_native_session(package, Default::default(), None)
        .unwrap();
    assert!(preparation.debt.native_session_package.is_none());
    let context = preparation.release_context(token).unwrap();
    ledger.validate_external(&context, binding).unwrap();
    assert!(context.native_session_obligations());
    let mut debt = preparation.into_debt(Some(context));
    debt.cleanup(&mut runtime).unwrap();
    assert!(!ledger.outstanding());
    assert_eq!(runtime.binding(), binding);
    for connection in [&executor, &secondary] {
        assert_ne!(
            runtime.connection_phase(connection),
            Ok(InstancePhase::Ready)
        );
    }
}

#[test]
fn genuine_preparation_rejects_unapproved_and_foreign_native_package_without_take() {
    let (_temp, mut runtime, mut manager, mut slot, host, digest, revisions) = ordinary();
    let (
        _foreign_temp,
        mut foreign,
        mut foreign_manager,
        mut foreign_slot,
        foreign_host,
        foreign_digest,
        foreign_revisions,
    ) = ordinary();
    let foreign_package = foreign_slot
        .connect(
            &mut foreign_manager,
            &mut foreign,
            &foreign_host,
            ID,
            foreign_digest,
            foreign_revisions,
            60_000,
            1,
            vec![],
        )
        .unwrap();
    let original = foreign_package.shared_connection();
    let binding = runtime.binding();
    let ledger = super::super::context_lifecycle::ContextLifecycleLedger::new(binding);
    let mut preparation = AgentPreparation::with_catalog(
        &mut runtime,
        Instant::now() - Duration::from_millis(10),
        Some(&mut manager),
        Some(&mut slot),
        vec![],
        host,
        ledger.clone(),
    );
    let executor = prepare_executor(&mut preparation);
    let expires = preparation.expires_after(30_000).unwrap();
    let mut wrong = digest;
    wrong[0] ^= 1;
    assert!(
        preparation
            .prepare_native_session_package(ID, wrong, revisions, expires)
            .is_err()
    );
    assert!(preparation.debt.native_session_package.is_none());
    assert!(
        preparation
            .prepare_native_session_package(ID, digest, revisions, expires)
            .is_err(),
        "failed factory replayed"
    );
    let mut package = Some(foreign_package);
    assert!(
        preparation
            .context_with_native_session_package(&mut package, Default::default(), None)
            .is_err()
    );
    assert!(Arc::ptr_eq(
        &package.as_ref().unwrap().shared_connection(),
        &original
    ));
    assert!(preparation.debt.retained_context.is_some());
    assert!(!ledger.outstanding());
    let mut debt = preparation.into_debt(None);
    debt.cleanup(&mut runtime).unwrap();
    assert_ne!(
        runtime.connection_phase(&executor),
        Ok(InstancePhase::Ready)
    );
    assert_eq!(
        foreign.connection_phase(&original),
        Ok(InstancePhase::Ready)
    );
    package
        .as_ref()
        .unwrap()
        .close_session(&mut foreign, &foreign_host)
        .unwrap();
    assert_ne!(
        foreign.connection_phase(&original),
        Ok(InstancePhase::Ready)
    );
}

#[test]
fn genuine_prepared_native_package_token_drop_and_context_drop_reconcile_original_owner() {
    let (_temp, mut runtime, mut manager, mut slot, host, digest, revisions) = ordinary();
    let binding = runtime.binding();
    let ledger = super::super::context_lifecycle::ContextLifecycleLedger::new(binding);
    // Dropping an unconsumed opaque package token must keep the actual connection.
    let mut preparation = AgentPreparation::with_catalog(
        &mut runtime,
        Instant::now() - Duration::from_millis(10),
        Some(&mut manager),
        Some(&mut slot),
        vec![],
        host.clone(),
        ledger.clone(),
    );
    let expires = preparation.expires_after(30_000).unwrap();
    let token = preparation
        .prepare_native_session_package(ID, digest, revisions, expires)
        .unwrap();
    let first = preparation
        .debt
        .native_session_package
        .as_ref()
        .unwrap()
        .shared_connection();
    drop(token);
    assert!(preparation.debt.native_session_package.is_some());
    let abandoned_executor = prepare_executor(&mut preparation);
    let ordinary_token = preparation.context(Default::default(), None).unwrap();
    assert!(
        preparation.release_context(ordinary_token).is_err(),
        "untransferred package debt escaped successful preparation"
    );
    assert!(preparation.debt.native_session_package.is_some());
    assert!(preparation.debt.retained_context.is_some());
    let mut debt = preparation.into_debt(None);
    debt.cleanup(&mut runtime).unwrap();
    assert!(!ledger.outstanding());
    assert_ne!(runtime.connection_phase(&first), Ok(InstancePhase::Ready));
    assert_ne!(
        runtime.connection_phase(&abandoned_executor),
        Ok(InstancePhase::Ready)
    );
    // A fresh explicitly prepared connection follows a separate lifecycle; it
    // cannot restore the old grant or its one-shot native factory.
    let mut preparation = AgentPreparation::with_catalog(
        &mut runtime,
        Instant::now() - Duration::from_millis(20),
        Some(&mut manager),
        Some(&mut slot),
        vec![],
        host,
        ledger.clone(),
    );
    let executor = prepare_executor(&mut preparation);
    let expires = preparation.expires_after(30_000).unwrap();
    let token = preparation
        .prepare_native_session_package(ID, digest, revisions, expires)
        .unwrap();
    let secondary = preparation
        .debt
        .native_session_package
        .as_ref()
        .unwrap()
        .shared_connection();
    assert!(!Arc::ptr_eq(&first, &secondary));
    let token = preparation
        .context_with_prepared_native_session(token, Default::default(), None)
        .unwrap();
    let context = preparation.release_context(token).unwrap();
    drop(preparation.into_debt(None));
    drop(context);
    assert!(ledger.outstanding() && ledger.needs_repair());
    assert_eq!(
        runtime.connection_phase(&secondary),
        Ok(InstancePhase::Ready)
    );
    let rescued = ledger.take_abandoned(binding).unwrap().unwrap();
    assert!(rescued.native_session_obligations());
    let mut debt = AgentPreparationDebt::for_context(rescued);
    debt.cleanup(&mut runtime).unwrap();
    debt.cleanup(&mut runtime).unwrap();
    assert!(!ledger.outstanding());
    assert_eq!(runtime.binding(), binding);
    for connection in [&executor, &secondary] {
        assert_ne!(
            runtime.connection_phase(connection),
            Ok(InstancePhase::Ready)
        );
    }
}

#[test]
fn genuine_raw_native_attachment_detach_preserves_package_and_tracked_context() {
    let (_temp, mut runtime, mut manager, mut slot, host, digest, revisions) = ordinary();
    let binding = runtime.binding();
    let package = slot
        .connect(
            &mut manager,
            &mut runtime,
            &host,
            ID,
            digest,
            revisions,
            60_000,
            1,
            vec![],
        )
        .unwrap();
    let exact = package.shared_connection();
    let revoke = runtime.revocation(&exact).unwrap();
    let executor = Arc::new(runtime.connect().unwrap());
    let caps = Capabilities {
        session_read: true,
        execute: true,
        ..Default::default()
    };
    let admission = host
        .admit(
            &runtime,
            &executor,
            caps,
            caps,
            vec!["session".into()],
            "domain".into(),
            60_000,
            1,
        )
        .unwrap();
    let resources = BorrowedNativeResources::for_owner(&runtime).unwrap();
    let mut context = AgentContext::new(
        host.clone(),
        Default::default(),
        resources,
        None,
        executor.clone(),
        admission.clone(),
    )
    .unwrap_or_else(|_| panic!("original raw context rejected"));
    let mut package = Some(package);
    context
        .attach_native_session_package(&manager, &runtime, &mut package, 1)
        .unwrap();
    assert!(package.is_none());
    // Failed disposal returns the exact unstarted context, not an artificial spawn failure.
    let mut context = match context.try_dispose() {
        Err(context) => *context,
        Ok(()) => panic!("attached package discarded"),
    };
    let returned = context.detach_native_session_package().unwrap().unwrap();
    assert!(Arc::ptr_eq(&returned.shared_connection(), &exact));
    assert_eq!(runtime.connection_phase(&exact), Ok(InstancePhase::Ready));
    assert!(
        !revoke.is_revoked(),
        "detach revoked or replaced the original grant"
    );
    returned.close_session(&mut runtime, &host).unwrap();
    assert_ne!(runtime.connection_phase(&exact), Ok(InstancePhase::Ready));
    host.revoke(&admission).unwrap();
    runtime.disconnect(&executor).unwrap();
    assert!(context.try_dispose().is_ok());
    // The same API cannot take a registered context's package or invalidate it.
    let ledger = super::super::context_lifecycle::ContextLifecycleLedger::new(binding);
    let mut preparation = AgentPreparation::with_catalog(
        &mut runtime,
        Instant::now() - Duration::from_millis(10),
        Some(&mut manager),
        Some(&mut slot),
        vec![],
        host,
        ledger.clone(),
    );
    let executor = prepare_executor(&mut preparation);
    let expires = preparation.expires_after(30_000).unwrap();
    let token = preparation
        .prepare_native_session_package(ID, digest, revisions, expires)
        .unwrap();
    let exact = preparation
        .debt
        .native_session_package
        .as_ref()
        .unwrap()
        .shared_connection();
    let token = preparation
        .context_with_prepared_native_session(token, Default::default(), None)
        .unwrap();
    let mut context = preparation.release_context(token).unwrap();
    assert!(context.detach_native_session_package().is_err());
    assert!(context.native_session_obligations());
    ledger.validate_external(&context, binding).unwrap();
    let mut debt = preparation.into_debt(Some(context));
    // Rejection left the actual package in original-owner cleanup debt.
    debt.cleanup(&mut runtime).unwrap();
    assert!(!ledger.outstanding());
    assert_eq!(runtime.binding(), binding);
    for connection in [&exact, &executor] {
        assert_ne!(
            runtime.connection_phase(connection),
            Ok(InstancePhase::Ready)
        );
    }
}
