//! Actual protected Windows owner, recipient approval, Wasm task and original close.
//! Private Capnp controls are covered separately from rendered UI/full SDK gates.
#![cfg(windows)]
use morrow_audit::session::{OpenMode, Session};
use morrow_blob_upload_history_v1::{INPUT_TYPE, Packet};
use morrow_core::{
    blob_stage::Command,
    blob_upload_journal as journal,
    plugin_package::{Package, proto::TransformHandler},
};
use morrow_workbench_host::{Workbench, blob_history_delivery::*};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};
#[path = "support/blob_history_delivery_private.rs"]
mod private_controls;
const ID: &str = "test.history.recipient";
struct Fixture {
    w: Workbench,
    p: Package,
    root: PathBuf,
    before: journal::Record,
}
impl Fixture {
    fn new(name: &str, trap: bool) -> Self {
        let root = PathBuf::from(std::env::var("MORROW_HISTORY_DELIVERY_ROOT").unwrap()).join(name);
        assert!(!root.exists());
        std::fs::create_dir(&root).unwrap();
        let db = root.join("library.db");
        let mut session = Session::open(&db, Default::default(), OpenMode::Initialize).unwrap();
        let request = journal::Request {
            submission: [21; 32],
            package_id: "old.uploader".into(),
            package_digest: [22; 32],
            registry_revision: 7,
            nomination_ref: [23; 32],
            command: Command {
                owner_domain: [24; 32],
                operation_id: "original-unknown".into(),
                byte_length: 0,
                sha256: Sha256::digest([]).into(),
            },
            handler: "old.upload".into(),
            input_sha256: [25; 32],
            binding_sha256: [26; 32],
        };
        session
            .runtime()
            .store_local_mut()
            .reserve_blob_upload_local_authorized(&request, 10, || Ok(()))
            .unwrap();
        let before = session
            .runtime()
            .store_local_mut()
            .observe_blob_upload_local_authorized(
                &request,
                journal::OriginalResult {
                    outcome: journal::Outcome::Error(7),
                    unknown: true,
                    calls: 0,
                    wire_bytes: 0,
                    native_work_bytes: 0,
                    execution: None,
                },
                11,
                || Ok(()),
            )
            .unwrap();
        assert!(!session.flush(16).unwrap().more_pending);
        drop(session);
        let module = if trap {
            wat::parse_str("(module (memory (export \"memory\") 4) (func (export \"morrow_run\") (result i32) unreachable))").unwrap()
        } else {
            let bytes = std::fs::read(std::env::var("MORROW_RUST_HISTORY_WASM").unwrap()).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                std::env::var("MORROW_RUST_HISTORY_SHA256").unwrap()
            );
            bytes
        };
        let p = Package::build(
            Package::manifest_for_transform(
                ID,
                "1.0.0",
                &module,
                ["history.inspect", "history.fixture.inspect"]
                    .into_iter()
                    .map(|handler| TransformHandler {
                        handler: handler.into(),
                        input_type: INPUT_TYPE.into(),
                        output_type: INPUT_TYPE.into(),
                        max_input_bytes: 8192,
                        max_output_bytes: 8192,
                    })
                    .collect(),
            ),
            &module,
        )
        .unwrap();
        let mut w = Workbench::open(&db, None).unwrap();
        let rev = w.catalog_page("", None).unwrap().revision;
        w.import_plugin_bytes(p.archive(), &p.digest(), rev)
            .unwrap();
        let rev = w.catalog_page("", None).unwrap().revision;
        w.configure_external(ID, &p.digest(), rev, &[], true)
            .unwrap();
        Self { w, p, root, before }
    }
    fn request(&self, index: u8) -> DeliveryRequest {
        DeliveryRequest {
            request_id: [index; 32],
            submission: [21; 32],
            package_id: ID.into(),
            package_digest: self.p.digest(),
            registry_revision: self.w.catalog_page("", None).unwrap().revision,
            handler: "history.inspect".into(),
            output_type: INPUT_TYPE.into(),
            lifetime: Duration::from_secs(120),
        }
    }
    fn preview(&mut self, index: u8) -> DeliveryPreview {
        let r = self.request(index);
        self.w.prepare_blob_history_delivery(r).unwrap()
    }
    fn unchanged(&self) {
        let current = self.w.inspect_persisted_blob_upload(&[21; 32]).unwrap();
        assert_eq!(current.record, self.before);
        assert!(current.record.original_unknown());
    }
    fn finish(mut self) {
        self.unchanged();
        self.w.finish().unwrap();
    }
}
fn approval(p: &DeliveryPreview) -> DeliveryApproval {
    DeliveryApproval {
        key: p.key,
        packet_sha256: p.packet_sha256,
        package_digest: p.request.package_digest,
        registry_revision: p.request.registry_revision,
    }
}
#[test]
fn selected_history_runs_once_lost_reply_reads_original_and_unknown_stays_unknown() {
    let mut f = Fixture::new("once", false);
    let p = f.preview(1);
    let discovered =
        f.w.discover_blob_history_delivery(&[1; 32])
            .unwrap()
            .unwrap();
    assert!(!discovered.consumed && !discovered.dispatched && discovered.execution.is_none());
    assert_eq!(
        f.w.preview_blob_history_delivery(p.key).unwrap().packet,
        p.packet
    );
    assert!(f.w.prepare_blob_history_delivery(f.request(1)).is_err());
    let r = f.w.approve_blob_history_delivery(approval(&p)).unwrap();
    assert!(r.consumed && r.dispatched && r.finished);
    assert_eq!(r.close, CloseState::Closed);
    assert!(r.error.is_none() && r.delivery_error.is_none());
    assert_eq!(r.execution.unwrap().outcome, Ok(0));
    assert_eq!(r.output.unwrap().bytes, p.packet);
    assert!(
        Packet::decode_task(&p.packet, &p.task_id)
            .unwrap()
            .record()
            .original_unknown()
    );
    assert!(f.w.approve_blob_history_delivery(approval(&p)).is_err());
    let original = f.w.read_blob_history_delivery(p.key).unwrap();
    assert_eq!(original.execution.unwrap().host_calls, 0);
    assert_eq!(original.output.unwrap().bytes, p.packet);
    assert!(f.w.prepare_blob_history_delivery(f.request(2)).is_err());
    f.w.acknowledge_blob_history_delivery(p.key).unwrap();
    assert!(f.w.prepare_blob_history_delivery(f.request(1)).is_err());
    let next = f.preview(2);
    assert_ne!(next.key, p.key);
    assert_ne!(next.task_id, p.task_id);
    assert!(f.w.approve_blob_history_delivery(approval(&p)).is_err());
    f.w.cancel_blob_history_delivery(next.key).unwrap();
    f.finish();
}
#[test]
fn mismatched_approval_and_foreign_owner_do_not_consume_or_dispatch() {
    let mut f = Fixture::new("mismatch", false);
    let p = f.preview(3);
    for changed in 0..3 {
        let mut a = approval(&p);
        match changed {
            0 => a.packet_sha256[0] ^= 1,
            1 => a.package_digest[0] ^= 1,
            _ => a.registry_revision += 1,
        };
        assert!(f.w.approve_blob_history_delivery(a).is_err());
    }
    let mut foreign = Workbench::open(&f.root.join("foreign.db"), None).unwrap();
    assert!(foreign.approve_blob_history_delivery(approval(&p)).is_err());
    assert!(foreign.read_blob_history_delivery(p.key).is_err());
    foreign.finish().unwrap();
    let r =
        f.w.discover_blob_history_delivery(&[3; 32])
            .unwrap()
            .unwrap();
    assert!(!r.consumed && !r.dispatched);
    f.w.cancel_blob_history_delivery(p.key).unwrap();
    f.finish();
}
#[test]
fn cancel_consumes_preview_without_guest_or_original_upload_ack() {
    let mut f = Fixture::new("cancel", false);
    let p = f.preview(4);
    let r = f.w.cancel_blob_history_delivery(p.key).unwrap();
    assert_eq!(r.error, Some(DeliveryError::Cancelled));
    assert!(!r.dispatched);
    assert_eq!(r.close, CloseState::NotCreated);
    assert!(r.execution.is_none());
    assert!(f.w.approve_blob_history_delivery(approval(&p)).is_err());
    f.w.acknowledge_blob_history_delivery(p.key).unwrap();
    f.finish();
}
#[test]
fn disabled_recipient_invalidates_exact_preview_without_dispatch() {
    let mut f = Fixture::new("disabled", false);
    let p = f.preview(5);
    f.w.configure_external(ID, &f.p.digest(), p.request.registry_revision, &[], false)
        .unwrap();
    let r = f.w.approve_blob_history_delivery(approval(&p)).unwrap();
    assert_eq!(r.error, Some(DeliveryError::StaleSelection));
    assert!(!r.dispatched && r.consumed);
    assert!(f.w.prepare_blob_history_delivery(f.request(6)).is_err());
    f.w.read_blob_history_delivery(p.key).unwrap();
    f.w.acknowledge_blob_history_delivery(p.key).unwrap();
    assert!(f.w.prepare_blob_history_delivery(f.request(6)).is_err());
    f.finish();
}
#[test]
fn expired_preview_is_consumed_before_admission() {
    let mut f = Fixture::new("expired", false);
    let mut request = f.request(7);
    request.lifetime = Duration::from_millis(1);
    let p = f.w.prepare_blob_history_delivery(request).unwrap();
    std::thread::sleep(Duration::from_millis(5));
    let r = f.w.approve_blob_history_delivery(approval(&p)).unwrap();
    assert_eq!(r.error, Some(DeliveryError::Expired));
    assert!(!r.dispatched && r.execution.is_none());
    assert!(f.w.approve_blob_history_delivery(approval(&p)).is_err());
    f.finish();
}
#[test]
fn revoked_product_gate_refuses_disclosure_before_dispatch() {
    let mut f = Fixture::new("gate-before", false);
    let p = f.preview(8);
    f.w.product_gate().revoke();
    let r = f.w.approve_blob_history_delivery(approval(&p)).unwrap();
    assert_eq!(r.error, Some(DeliveryError::GateClosed));
    assert!(!r.dispatched && r.execution.is_none());
    assert!(f.w.approve_blob_history_delivery(approval(&p)).is_err());
    f.w.finish().unwrap();
}
#[test]
fn later_gate_loss_preserves_execution_and_withholds_output() {
    let mut f = Fixture::new("gate-after", false);
    let p = f.preview(9);
    let r = f.w.approve_blob_history_delivery(approval(&p)).unwrap();
    assert_eq!(r.close, CloseState::Closed);
    let execution = r.execution.unwrap().outcome;
    f.w.product_gate().revoke();
    let r = f.w.read_blob_history_delivery(p.key).unwrap();
    assert_eq!(r.execution.unwrap().outcome, execution);
    assert_eq!(r.error, None);
    assert_eq!(r.delivery_error, Some(DeliveryError::GateClosed));
    assert!(r.output.is_none());
    assert_eq!(r.close, CloseState::Closed);
    f.w.finish().unwrap();
}
#[test]
fn guest_trap_retains_first_execution_error_and_actual_close() {
    let mut f = Fixture::new("trap", true);
    let p = f.preview(10);
    let r = f.w.approve_blob_history_delivery(approval(&p)).unwrap();
    assert_eq!(r.error, Some(DeliveryError::Execution));
    assert_eq!(r.close, CloseState::Closed);
    assert_eq!(
        r.execution.unwrap().outcome,
        Err(morrow_plugin_runtime::Fault::Trap)
    );
    assert!(r.output.is_none());
    assert!(f.w.approve_blob_history_delivery(approval(&p)).is_err());
    f.finish();
}

#[test]
fn original_owner_outlives_instance_budget_across_acknowledged_deliveries() {
    let mut f = Fixture::new("same-owner-capacity", false);
    // Both the runtime Manager and core HostPolicy cap live instances at 128.
    // Cross that actual ceiling on this one owner, not on replacement stores.
    for index in 1..=130u8 {
        let p = f.preview(index);
        let r = f.w.approve_blob_history_delivery(approval(&p)).unwrap();
        assert_eq!(r.close, CloseState::Closed);
        assert!(r.close_failure.is_none() && r.error.is_none());
        assert_eq!(r.execution.unwrap().outcome, Ok(0));
        assert_eq!(r.output.unwrap().bytes, p.packet);
        assert!(f.w.approve_blob_history_delivery(approval(&p)).is_err());
        f.w.acknowledge_blob_history_delivery(p.key).unwrap();
        f.unchanged();
    }
    f.finish();
}

#[test]
fn guest_failure_data_is_withheld_after_gate_owner_or_recipient_veto() {
    for mode in ["gate", "owner", "recipient"] {
        let mut f = Fixture::new(&format!("failure-veto-{mode}"), false);
        let mut request = f.request(1);
        // The fixture handler returns a real correlated InvalidInput failure
        // for these non-fixture IDs; its text is untrusted plugin data.
        request.handler = "history.fixture.inspect".into();
        let p = f.w.prepare_blob_history_delivery(request).unwrap();
        let r = f.w.approve_blob_history_delivery(approval(&p)).unwrap();
        assert_eq!(r.close, CloseState::Closed);
        assert_eq!(r.error, Some(DeliveryError::PluginFailure));
        assert_eq!(r.execution.unwrap().outcome, Ok(0));
        assert_eq!(r.failure.unwrap().message, "History fixture rejected");
        let expected = match mode {
            "gate" => {
                f.w.product_gate().revoke();
                DeliveryError::GateClosed
            }
            "owner" => {
                f.w.bind_product_gate(Default::default());
                DeliveryError::StaleOwner
            }
            _ => {
                f.w.configure_external(ID, &f.p.digest(), p.request.registry_revision, &[], false)
                    .unwrap();
                DeliveryError::StaleSelection
            }
        };
        let r = f.w.read_blob_history_delivery(p.key).unwrap();
        assert_eq!(r.delivery_error, Some(expected));
        assert!(r.failure.is_none() && r.output.is_none());
        assert_eq!(r.error, Some(DeliveryError::PluginFailure));
        assert_eq!(r.execution.unwrap().outcome, Ok(0));
        assert!(
            f.w.discover_blob_history_delivery(&[1; 32])
                .unwrap()
                .unwrap()
                .failure
                .is_none()
        );
        f.w.finish().unwrap();
    }
}
#[test]
#[ignore = "separate explicit fault-injection process; retains original close debt"]
fn original_close_refusal_blocks_replacement_and_keeps_first_failure() {
    assert!(cfg!(feature = "fault-injection"));
    assert_eq!(
        std::env::var("MORROW_HISTORY_DELIVERY_FAULT").unwrap(),
        "retired-original-close"
    );
    for trap in [false, true] {
        let mut f = Fixture::new(if trap { "close-trap" } else { "close-success" }, trap);
        let p = f.preview(11);
        let r = f.w.approve_blob_history_delivery(approval(&p)).unwrap();
        assert_eq!(r.close, CloseState::Failed);
        assert_eq!(
            r.close_failure,
            Some(CloseFailure::Core(morrow_core::Error::Invalid(
                "stale instance"
            )))
        );
        assert!(r.output.is_none());
        assert_eq!(
            r.error,
            Some(if trap {
                DeliveryError::Execution
            } else {
                DeliveryError::Close
            })
        );
        assert_eq!(
            r.execution.unwrap().outcome,
            if trap {
                Err(morrow_plugin_runtime::Fault::Trap)
            } else {
                Ok(0)
            }
        );
        assert!(f.w.acknowledge_blob_history_delivery(p.key).is_err());
        assert!(f.w.approve_blob_history_delivery(approval(&p)).is_err());
        assert!(f.w.finish().is_err());
        assert!(f.w.maintenance_warning().is_some());
        assert!(f.w.catalog_page("", None).is_err());
        let again =
            f.w.discover_blob_history_delivery(&[11; 32])
                .unwrap()
                .unwrap();
        assert_eq!(again.close, CloseState::Failed);
        assert_eq!(
            again.close_failure,
            Some(CloseFailure::Core(morrow_core::Error::Invalid(
                "stale instance"
            )))
        );
        assert!(again.execution.is_some());
        private_controls::retained_close_reply(&mut f, p.key, trap);
        // Drop is only a stop signal, never credited as successful closure.
    }
}

#[test]
fn persisted_enabled_unsupported_guest_refuses_before_connection() {
    use morrow_core::plugin_package::{catalog::Catalog, registry::Registry};
    use morrow_plugin_runtime::{
        Fault, Limits,
        manager::{Manager, ManagerError},
    };
    let mut f = Fixture::new("preparation-refusal", false);
    f.w.finish().unwrap();
    drop(f.w);
    // A trusted fixture represents an enabled persisted selection whose module
    // cannot be prepared. Seed it before opening the actual delivery owner;
    // production configure_external intentionally rejects this module earlier.
    let module = wat::parse_str("(module (memory (export \"memory\") 4))").unwrap();
    f.p = Package::build(
        Package::manifest_for_transform(
            ID,
            "2.0.0",
            &module,
            vec![TransformHandler {
                handler: "history.inspect".into(),
                input_type: INPUT_TYPE.into(),
                output_type: INPUT_TYPE.into(),
                max_input_bytes: 8192,
                max_output_bytes: 8192,
            }],
        ),
        &module,
    )
    .unwrap();
    let catalog = Catalog::open(&f.root.join("plugin-manager/packages")).unwrap();
    catalog.install(&f.p).unwrap();
    let registry = Registry::open(&f.root.join("plugin-manager/state"), catalog).unwrap();
    let mut manager = Manager::new(registry, Limits::default());
    manager.select(&f.p, manager.revision()).unwrap();
    manager
        .approve(ID, f.p.digest(), Default::default(), manager.revision())
        .unwrap();
    manager
        .set_enabled(ID, f.p.digest(), true, manager.revision())
        .unwrap();
    assert!(matches!(
        manager.validate_preparation(ID, f.p.digest(), manager.revision()),
        Err(ManagerError::Prepare(Fault::UnsupportedAbi))
    ));
    drop(manager);
    f.w = Workbench::open(&f.root.join("library.db"), None).unwrap();
    let p = f.preview(1);
    let r = f.w.approve_blob_history_delivery(approval(&p)).unwrap();
    assert_eq!(r.error, Some(DeliveryError::Preparation));
    assert_eq!(r.close, CloseState::NotCreated);
    assert!(r.consumed && r.finished && !r.dispatched);
    assert!(r.close_failure.is_none() && r.execution.is_none() && r.output.is_none());
    assert!(f.w.approve_blob_history_delivery(approval(&p)).is_err());
    assert!(f.w.maintenance_warning().is_none());
    assert!(f.w.catalog_page("", None).is_ok());
    f.w.read_blob_history_delivery(p.key).unwrap();
    f.w.acknowledge_blob_history_delivery(p.key).unwrap();
    f.finish();
}

#[test]
#[ignore = "separate explicit close-boundary unwind process; original debt retained"]
fn original_close_unwind_is_unknown_and_never_becomes_cleanup_proof() {
    assert!(cfg!(feature = "fault-injection"));
    assert_eq!(
        std::env::var("MORROW_HISTORY_DELIVERY_FAULT").unwrap(),
        "original-close-unwind"
    );
    for trap in [false, true] {
        let mut f = Fixture::new(
            if trap {
                "unwind-trap"
            } else {
                "unwind-success"
            },
            trap,
        );
        let p = f.preview(1);
        let r = f.w.approve_blob_history_delivery(approval(&p)).unwrap();
        assert_eq!(r.close, CloseState::Unknown);
        assert_eq!(r.close_failure, Some(CloseFailure::Unwind));
        assert_eq!(
            r.error,
            Some(if trap {
                DeliveryError::Execution
            } else {
                DeliveryError::Close
            })
        );
        assert_eq!(
            r.execution.unwrap().outcome,
            if trap {
                Err(morrow_plugin_runtime::Fault::Trap)
            } else {
                Ok(0)
            }
        );
        assert!(r.output.is_none());
        assert!(f.w.acknowledge_blob_history_delivery(p.key).is_err());
        assert!(f.w.approve_blob_history_delivery(approval(&p)).is_err());
        assert!(f.w.finish().is_err());
        assert!(f.w.maintenance_warning().is_some());
        assert!(f.w.catalog_page("", None).is_err());
        let again =
            f.w.discover_blob_history_delivery(&[1; 32])
                .unwrap()
                .unwrap();
        assert_eq!(again.close, CloseState::Unknown);
        assert_eq!(again.close_failure, Some(CloseFailure::Unwind));
        private_controls::retained_unwind_reply(&mut f, p.key, trap);
        // This process-ending synthetic fault is not proof of native OS cleanup.
    }
}
