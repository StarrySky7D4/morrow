//! G06 ordinary disposable Store integration, not protected Linux product proof.
//! All events originate in actual permanent Core commits; no host-crafted source frames.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    changes_metadata::{self, Metadata},
    channel::{Action, Budget, Frame, Kind, Request, Response, Status},
    content::CardRecord,
    content_change::ContentChange,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{Package, catalog::Catalog, proto::TransformHandler, registry::Registry},
    store::{ChangesBudget, EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Limits,
    channel::{
        CleanupProof, Error, ProducerOutcome, Source,
        changes::{ChangesApproval, ChangesSource, HistoryStart},
    },
    manager::{ManagedInstance, Manager},
};
use std::{
    thread,
    time::{Duration, Instant},
};
const ID: &str = "org.example.changes.runtime";
const SCOPE: [u8; 32] = [0x61; 32];
const WAIT: Duration = Duration::from_secs(5);
fn budget(duration: u64) -> Budget {
    Budget {
        max_channels: 4,
        max_frame_bytes: 1024,
        max_bytes: 65536,
        max_messages: 64,
        max_requests: 4096,
        max_duration_ms: duration,
    }
}
fn package(version: &str, changes: bool) -> Package {
    let wasm = wat::parse_str(
        r#"(module
        (import "morrow_channel_v1" "call" (func (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 4)
        (func (export "morrow_run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        ID,
        version,
        &wasm,
        vec![TransformHandler {
            handler: "changes.metadata".into(),
            input_type: "morrow.channel.directory.v1".into(),
            output_type: "morrow.changes.metadata.count.v1".into(),
            max_input_bytes: 65536,
            max_output_bytes: 8,
        }],
    );
    manifest
        .required_features
        .push(morrow_core::channel::FEATURE.into());
    if changes {
        manifest
            .required_features
            .push(changes_metadata::FEATURE.into());
    }
    manifest.channel_declaration = Some(morrow_core::channel::declaration(
        vec!["changes.metadata".into()],
        vec![Kind::Events],
    ));
    Package::build(manifest, &wasm).unwrap()
}
struct Fixture {
    _dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    package: Package,
}
impl Fixture {
    fn new() -> Self {
        Self::with_package(package("1.0.0", true))
    }
    fn with_package(package: Package) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(
            registry,
            Limits {
                host_calls: 1024,
                ..Limits::default()
            },
        );
        manager.select(&package, manager.revision()).unwrap();
        manager
            .set_enabled(
                &package.manifest().package_id,
                package.digest(),
                true,
                manager.revision(),
            )
            .unwrap();
        let host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let mut host = host;
        let instance = manager
            .connect(&package.manifest().package_id, &mut host)
            .unwrap();
        Self {
            _dir: dir,
            manager,
            host,
            instance,
            package,
        }
    }
    fn seed(&mut self) {
        self.create("a", "create-a", b"private body A");
        self.create("SECRET-CARD", "SECRET-OPERATION", b"SECRET BODY AND TITLE");
        self.create("b", "create-b", b"private body B");
    }
    fn create(&mut self, card: &str, op: &str, body: &[u8]) {
        self.host
            .store_local_mut()
            .create_local(
                op,
                &CardRecord::new(card, "test.note", 1, "private title", body.to_vec()).unwrap(),
            )
            .unwrap();
    }
    fn issue(
        &self,
        cards: Vec<String>,
        start: HistoryStart,
        duration: u64,
    ) -> Result<ChangesApproval, Error> {
        ChangesApproval::issue(
            &self.manager,
            &self.host,
            &self.instance,
            self.manager.revision(),
            cards,
            start,
            ChangesBudget::default(),
            duration + 1,
            1,
            vec![],
        )
    }
    fn source(&self, cards: Vec<String>, start: HistoryStart, duration: u64) -> ChangesSource {
        let ceiling = Budget::from_proto(
            self.package
                .channel_declaration()
                .unwrap()
                .budget
                .as_ref()
                .unwrap(),
        )
        .unwrap();
        let mut approved = budget(duration);
        approved.max_channels = approved.max_channels.min(ceiling.max_channels);
        approved.max_frame_bytes = approved.max_frame_bytes.min(ceiling.max_frame_bytes);
        approved.max_bytes = approved.max_bytes.min(ceiling.max_bytes);
        approved.max_messages = approved.max_messages.min(ceiling.max_messages);
        approved.max_requests = approved.max_requests.min(ceiling.max_requests);
        self.issue(cards, start, duration)
            .unwrap()
            .bind(
                &self.manager,
                &self.host,
                &self.instance,
                SCOPE,
                approved,
                2,
            )
            .unwrap()
    }
    fn call(
        &mut self,
        source: &ChangesSource,
        action: Action,
        serial: u8,
    ) -> Result<Response, Error> {
        let endpoint = source.broker().endpoint();
        source.broker().dispatch(
            &self.manager,
            &mut self.host,
            &self.instance,
            &Request {
                call_id: [serial; 32],
                reference: endpoint.reference,
                source_epoch: endpoint.source_epoch,
                action,
            },
            2,
        )
    }
    fn frame(&mut self, source: &ChangesSource, last: u64) -> Frame {
        let deadline = Instant::now() + WAIT;
        loop {
            let response = self
                .call(
                    source,
                    Action::Receive {
                        last_acked: last,
                        credit_bytes: 1024,
                    },
                    1,
                )
                .unwrap();
            if response.status == Status::Frame {
                return response.frame.unwrap();
            }
            assert_eq!(response.status, Status::Idle);
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn ack(&mut self, source: &ChangesSource, frame: &Frame) -> Response {
        self.call(source, ack(frame), 2).unwrap()
    }
    fn no_ack(&self, epoch: [u8; 32]) {
        assert!(
            self.host
                .store_local()
                .channel_checkpoint(&SCOPE, &epoch)
                .unwrap()
                .is_none()
        );
        assert!(
            self.host
                .store_local()
                .channel_ack_receipt(&SCOPE, &epoch, 1)
                .unwrap()
                .is_none()
        );
    }
    fn edit(&mut self, card: &str, op: &str, revision: u64) -> morrow_core::transaction::Receipt {
        let mut writer = self.host.connect().unwrap();
        self.host
            .grant(&mut writer, GrantKind::EditContent, card, 30_000, 1)
            .unwrap();
        self.host
            .edit_content(
                &writer,
                &ContentChange {
                    operation_id: op.into(),
                    card_id: card.into(),
                    expected_revision: revision,
                    title: "changed title".into(),
                    body: b"changed private body".to_vec(),
                    preview_text: "".into(),
                    attachments: None,
                },
                || 1,
            )
            .unwrap()
    }
}
fn cards() -> Vec<String> {
    vec!["a".into(), "b".into()]
}
fn ack(frame: &Frame) -> Action {
    Action::Ack {
        sequence: frame.sequence,
        frame_sha256: frame.digest().unwrap(),
        cursor: frame.cursor.clone(),
    }
}
fn joined(source: &ChangesSource) -> Status {
    let deadline = Instant::now() + WAIT;
    loop {
        let status = source.broker().cleanup_handle().reap();
        let snapshot = source.broker().snapshot();
        if snapshot.resource_reclaimed {
            assert_eq!(snapshot.cleanup_proof, CleanupProof::Joined);
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "actual source thread did not join"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn actual_store_metadata_is_filtered_durable_and_finite() {
    let mut f = Fixture::new();
    f.seed();
    let edited = f.edit("a", "edit-a", 1);
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let mut seen = vec![];
    for last in 0..3 {
        let frame = f.frame(&source, last);
        let metadata = Metadata::decode(&frame.bytes).unwrap();
        assert_eq!(metadata.window_id, source.broker().endpoint().source_epoch);
        assert_eq!(metadata.scope_digest, source.scope_digest());
        assert_eq!(frame.cursor, metadata.cursor().unwrap());
        for forbidden in [
            b"SECRET-CARD".as_slice(),
            b"SECRET-OPERATION",
            b"SECRET BODY AND TITLE",
            b"private body",
            b"private title",
        ] {
            assert!(
                !frame
                    .encode()
                    .unwrap()
                    .windows(forbidden.len())
                    .any(|s| s == forbidden)
            );
        }
        if metadata.operation_id == "edit-a" {
            assert_eq!(metadata.revision, edited.revision);
            assert_eq!(metadata.card_sha256, edited.content_sha256);
        }
        seen.push((metadata.card_id, metadata.operation_id));
        let response = f.ack(&source, &frame);
        assert_eq!(response.status, Status::Acked);
        let checkpoint = f
            .host
            .store_local()
            .channel_checkpoint(&SCOPE, &frame.source_epoch)
            .unwrap()
            .unwrap();
        let receipt = f
            .host
            .store_local()
            .channel_ack_receipt(&SCOPE, &frame.source_epoch, frame.sequence)
            .unwrap()
            .unwrap();
        assert_eq!(checkpoint, receipt.checkpoint);
        assert_eq!(receipt.frame_wire, frame.encode().unwrap());
        assert_eq!(receipt.response_wire, response.encode().unwrap());
    }
    assert_eq!(
        seen,
        [
            ("a".into(), "create-a".into()),
            ("b".into(), "create-b".into()),
            ("a".into(), "edit-a".into())
        ]
    );
    assert_eq!(joined(&source), Status::Closed);
    assert_eq!(
        source.broker().snapshot().producer_outcome,
        ProducerOutcome::Eof
    );
    f.host.store_local().integrity_check().unwrap();
}
#[test]
fn upper_bound_is_frozen_before_spawn_and_store_is_not_borrowed_while_waiting() {
    let mut f = Fixture::new();
    f.seed();
    let approval = f.issue(cards(), HistoryStart::Beginning, 10_000).unwrap();
    f.edit("a", "after-upper", 1);
    let source = approval
        .bind(&f.manager, &f.host, &f.instance, SCOPE, budget(10_000), 1)
        .unwrap();
    let first = f.frame(&source, 0);
    // This same Store mutation succeeds while producer waits on exact frame ACK.
    f.edit("b", "while-awaiting-ack", 1);
    f.ack(&source, &first);
    let second = f.frame(&source, 1);
    f.ack(&source, &second);
    assert_eq!(joined(&source), Status::Closed);
    assert_eq!(source.broker().snapshot().last_acked, 2);
}
#[test]
fn slow_credit_wrong_ack_and_duplicate_do_not_advance_or_refund() {
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let frame = f.frame(&source, 0);
    let usage = source.broker().snapshot().usage;
    assert_eq!(
        f.call(
            &source,
            Action::Receive {
                last_acked: 0,
                credit_bytes: 1
            },
            3
        )
        .unwrap()
        .status,
        Status::Limit
    );
    thread::sleep(Duration::from_millis(40));
    assert_eq!(source.broker().snapshot().usage.messages, 1);
    for action in [
        Action::Ack {
            sequence: 2,
            frame_sha256: frame.digest().unwrap(),
            cursor: frame.cursor.clone(),
        },
        Action::Ack {
            sequence: 1,
            frame_sha256: [0x22; 32],
            cursor: frame.cursor.clone(),
        },
        Action::Ack {
            sequence: 1,
            frame_sha256: frame.digest().unwrap(),
            cursor: vec![1; 32],
        },
    ] {
        assert_eq!(f.call(&source, action, 4).unwrap().status, Status::Invalid);
    }
    f.no_ack(frame.source_epoch);
    assert_eq!(f.ack(&source, &frame).status, Status::Acked);
    assert_ne!(
        f.call(&source, ack(&frame), 5).unwrap().status,
        Status::Acked
    );
    assert!(source.broker().snapshot().usage.bytes >= usage.bytes);
    source.revoke();
    assert_eq!(joined(&source), Status::Revoked);
}
#[test]
fn empty_window_has_eof_and_actual_join_but_no_public_durable_progress() {
    let mut f = Fixture::new();
    f.create("secret", "secret-op", b"secret");
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    assert_eq!(joined(&source), Status::Closed);
    f.no_ack(source.broker().endpoint().source_epoch);
    assert!(
        f.issue(
            cards(),
            HistoryStart::LatestAck {
                subscription: SCOPE,
                source_epoch: source.broker().endpoint().source_epoch
            },
            10_000
        )
        .is_err()
    );
}
#[test]
fn arbitrary_local_producer_is_denied_for_the_opt_in_profile_before_approval() {
    let f = Fixture::new();
    let broker = f
        .manager
        .bind_channel(
            &f.host,
            &f.instance,
            f.package.digest(),
            f.manager.revision(),
            Source {
                kind: Kind::Events,
                duplex: false,
                checkpoint_scope: Some(SCOPE),
            },
            budget(10_000),
            10_001,
            1,
        )
        .unwrap();
    assert_eq!(
        broker.spawn(|_| panic!("unapproved producer ran")),
        Err(Error::Denied)
    );
    assert_eq!(broker.snapshot().usage.messages, 0);
    broker.cleanup();
    assert_eq!(broker.snapshot().cleanup_proof, CleanupProof::NoProducer);
}
#[test]
fn legacy_channel_package_does_not_gain_changes_from_content_probe_or_declaration() {
    let mut f = Fixture::with_package(package("1.0.0", false));
    f.seed();
    let mut owner = f.host.connect().unwrap();
    f.host
        .grant(&mut owner, GrantKind::ReadContent, "a", 20_000, 1)
        .unwrap();
    let probe = f
        .host
        .content_authorization(&owner, GrantKind::ReadContent, "a", None, 1)
        .unwrap();
    assert!(
        ChangesApproval::issue(
            &f.manager,
            &f.host,
            &f.instance,
            f.manager.revision(),
            vec!["a".into()],
            HistoryStart::Beginning,
            ChangesBudget::default(),
            10_001,
            1,
            vec![probe]
        )
        .is_err()
    );
}
#[test]
fn fixed_set_invalidity_stale_revision_and_foreign_targets_reject() {
    let mut f = Fixture::new();
    f.seed();
    for values in [vec![], vec!["a".into(), "a".into()], vec!["a/b".into()]] {
        assert!(f.issue(values, HistoryStart::Beginning, 10_000).is_err());
    }
    assert!(
        ChangesApproval::issue(
            &f.manager,
            &f.host,
            &f.instance,
            f.manager.revision() - 1,
            cards(),
            HistoryStart::Beginning,
            ChangesBudget::default(),
            10_001,
            1,
            vec![]
        )
        .is_err()
    );
    let approval = f.issue(cards(), HistoryStart::Beginning, 10_000).unwrap();
    let other = f.manager.connect(ID, &mut f.host).unwrap();
    assert!(
        approval
            .bind(&f.manager, &f.host, &other, SCOPE, budget(10_000), 1)
            .is_err()
    );
    let approval = f.issue(cards(), HistoryStart::Beginning, 10_000).unwrap();
    let foreign =
        HostRuntime::new(Store::open(&f._dir.path().join("other-db"), Default::default()).unwrap())
            .unwrap();
    assert!(
        approval
            .bind(&f.manager, &foreign, &f.instance, SCOPE, budget(10_000), 1)
            .is_err()
    );
    let g = Fixture::new();
    let approval = f.issue(cards(), HistoryStart::Beginning, 10_000).unwrap();
    assert!(
        approval
            .bind(&g.manager, &f.host, &f.instance, SCOPE, budget(10_000), 1)
            .is_err()
    );
}
#[test]
fn aggregate_scan_event_output_decode_and_channel_budgets_reject_without_producer() {
    let mut f = Fixture::new();
    f.seed();
    for read_budget in [
        ChangesBudget {
            max_candidates: 2,
            ..Default::default()
        },
        ChangesBudget {
            max_events: 1,
            ..Default::default()
        },
        ChangesBudget {
            max_metadata_bytes: 1,
            ..Default::default()
        },
        ChangesBudget {
            max_decoded_bytes: 1,
            ..Default::default()
        },
        ChangesBudget {
            max_container_bytes: 1,
            ..Default::default()
        },
        ChangesBudget {
            max_single_decoded_bytes: 1,
            ..Default::default()
        },
    ] {
        assert_eq!(
            ChangesApproval::issue(
                &f.manager,
                &f.host,
                &f.instance,
                f.manager.revision(),
                cards(),
                HistoryStart::Beginning,
                read_budget,
                10_001,
                1,
                vec![]
            )
            .err(),
            Some(Error::Limit)
        );
    }
    let approval = f.issue(cards(), HistoryStart::Beginning, 10_000).unwrap();
    assert_eq!(
        approval
            .bind(
                &f.manager,
                &f.host,
                &f.instance,
                SCOPE,
                Budget {
                    max_messages: 1,
                    ..budget(10_000)
                },
                1
            )
            .err(),
        Some(Error::Limit)
    );
    let approval = f.issue(cards(), HistoryStart::Beginning, 10_000).unwrap();
    assert_eq!(
        approval
            .bind(
                &f.manager,
                &f.host,
                &f.instance,
                SCOPE,
                Budget {
                    max_frame_bytes: 1,
                    ..budget(10_000)
                },
                1
            )
            .err(),
        Some(Error::Limit)
    );
}
#[test]
fn revoke_one_fixed_card_cancels_whole_source_waiting_for_ack_and_joins() {
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let frame = f.frame(&source, 0);
    assert_eq!(
        source.revocation().revoke_card("SECRET-CARD"),
        Err(Error::Denied)
    );
    assert_eq!(source.broker().snapshot().terminal_cause, None);
    source.revocation().revoke_card("b").unwrap();
    assert_eq!(joined(&source), Status::Revoked);
    f.no_ack(frame.source_epoch);
    assert!(f.call(&source, ack(&frame), 3).unwrap().frame.is_none());
}
#[test]
fn real_monotonic_expiry_while_waiting_without_new_guest_calls_joins() {
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 100);
    let frame = f.frame(&source, 0);
    thread::sleep(Duration::from_millis(120));
    assert_eq!(joined(&source), Status::Expired);
    f.no_ack(frame.source_epoch);
}
#[test]
fn approval_duration_is_not_renewed_when_binding_later() {
    let mut f = Fixture::new();
    f.seed();
    let approval = f.issue(cards(), HistoryStart::Beginning, 50).unwrap();
    thread::sleep(Duration::from_millis(65));
    assert_eq!(
        approval
            .bind(&f.manager, &f.host, &f.instance, SCOPE, budget(50), 1)
            .err(),
        Some(Error::Expired)
    );
}
#[test]
fn instance_cancel_and_disconnect_terminate_waiting_producer_without_guest_progress() {
    for disconnect in [false, true] {
        let mut f = Fixture::new();
        f.seed();
        let source = f.source(cards(), HistoryStart::Beginning, 10_000);
        let frame = f.frame(&source, 0);
        if disconnect {
            f.host.disconnect(f.instance.connection()).unwrap();
        } else {
            f.instance.request_stop();
        }
        assert_eq!(joined(&source), Status::Revoked);
        f.no_ack(frame.source_epoch);
    }
}
#[test]
fn manager_disable_remove_switch_and_drop_revoke_exact_original_source() {
    for mode in 0..3 {
        let mut f = Fixture::new();
        f.seed();
        let source = f.source(cards(), HistoryStart::Beginning, 10_000);
        let frame = f.frame(&source, 0);
        match mode {
            0 => f
                .manager
                .set_enabled(ID, f.package.digest(), false, f.manager.revision())
                .unwrap(),
            1 => f.manager.remove(ID, f.manager.revision()).unwrap(),
            _ => {
                let next = package("2.0.0", true);
                f.manager.install_package(next.archive()).unwrap();
                f.manager.select(&next, f.manager.revision()).unwrap();
            }
        }
        assert_eq!(joined(&source), Status::Revoked);
        f.no_ack(frame.source_epoch);
    }
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let _ = f.frame(&source, 0);
    drop(f.manager);
    assert_eq!(joined(&source), Status::Revoked);
}
#[test]
fn original_content_restriction_replacement_and_revocation_do_not_revive() {
    for replace in [false, true] {
        let mut f = Fixture::new();
        f.seed();
        let mut owner = f.host.connect().unwrap();
        f.host
            .grant(&mut owner, GrantKind::ReadContent, "a", 20_000, 1)
            .unwrap();
        let probe = f
            .host
            .content_authorization(&owner, GrantKind::ReadContent, "a", None, 1)
            .unwrap();
        let approval = ChangesApproval::issue(
            &f.manager,
            &f.host,
            &f.instance,
            f.manager.revision(),
            cards(),
            HistoryStart::Beginning,
            ChangesBudget::default(),
            10_001,
            1,
            vec![probe],
        )
        .unwrap();
        let source = approval
            .bind(&f.manager, &f.host, &f.instance, SCOPE, budget(10_000), 1)
            .unwrap();
        let frame = f.frame(&source, 0);
        if replace {
            f.host
                .grant(&mut owner, GrantKind::ReadContent, "a", 20_000, 100)
                .unwrap();
        } else {
            f.host
                .revoke(&mut owner, GrantKind::ReadContent, "a")
                .unwrap();
        }
        assert_eq!(joined(&source), Status::Revoked);
        f.no_ack(frame.source_epoch);
    }
}
#[test]
fn latest_ack_reopens_only_new_window_and_rejects_missing_or_changed_scope() {
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let first = f.frame(&source, 0);
    f.ack(&source, &first);
    let last = f.frame(&source, 1);
    f.ack(&source, &last);
    joined(&source);
    let old = source.broker().endpoint();
    drop(source);
    f.edit("a", "after-ack", 1);
    let history = HistoryStart::LatestAck {
        subscription: SCOPE,
        source_epoch: old.source_epoch,
    };
    assert!(f.issue(vec!["a".into()], history, 10_000).is_err());
    assert!(
        f.issue(
            cards(),
            HistoryStart::LatestAck {
                subscription: [0x62; 32],
                source_epoch: old.source_epoch
            },
            10_000
        )
        .is_err()
    );
    // Original receiver remains live, but a new approval / broker / epoch is mandatory.
    let new = f.source(cards(), history, 10_000);
    let endpoint = new.broker().endpoint();
    assert_ne!(endpoint.source_epoch, old.source_epoch);
    assert_ne!(endpoint.reference, old.reference);
    let frame = f.frame(&new, 0);
    assert_eq!(
        Metadata::decode(&frame.bytes).unwrap().operation_id,
        "after-ack"
    );
    f.ack(&new, &frame);
    assert_eq!(joined(&new), Status::Closed);
    assert_eq!(
        f.host
            .store_local()
            .channel_checkpoint(&SCOPE, &old.source_epoch)
            .unwrap()
            .unwrap()
            .sequence,
        2
    );
}
#[test]
fn dropping_adapter_while_waiting_retains_actual_join_cleanup_owner() {
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let _ = f.frame(&source, 0);
    let cleanup = source.broker().cleanup_handle();
    drop(source);
    let deadline = Instant::now() + WAIT;
    loop {
        cleanup.reap();
        if cleanup.resource_reclaimed() {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(cleanup.cleanup_proof(), CleanupProof::Joined);
}
#[cfg(feature = "fault-injection")]
#[test]
fn final_original_store_commit_revocation_rolls_back_checkpoint_and_receipt() {
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let frame = f.frame(&source, 0);
    source.revoke_at_final_commit_for_fault_test();
    assert_eq!(f.call(&source, ack(&frame), 3), Err(Error::Denied));
    assert_eq!(source.broker().snapshot().last_acked, 0);
    f.no_ack(frame.source_epoch);
    assert_eq!(joined(&source), Status::Revoked);
    f.host.store_local().integrity_check().unwrap();
}
#[test]
#[ignore = "requires exact newly built three-language extension packages; no fallback or rebuilding old SDK"]
fn compiled_three_language_guests_consume_actual_store_metadata_and_original_acks() {
    use sha2::{Digest, Sha256};
    for language in ["RUST", "C", "CPP"] {
        let key = format!("MORROW_CHANGES_PACKAGE_{language}");
        let path = std::env::var_os(&key).unwrap_or_else(|| panic!("missing mandatory {key}"));
        let bytes = std::fs::read(path).unwrap();
        let actual = format!("{:x}", Sha256::digest(&bytes));
        let expected = std::env::var(format!("{key}_SHA256"))
            .expect("mandatory independently recorded original package hash");
        assert_eq!(
            actual, expected,
            "{language}: immutable original package identity"
        );
        eprintln!("G06 {language} original package sha256={actual}");
        let package = Package::decode(&bytes).unwrap();
        assert_eq!(package.archive(), bytes);
        let mut f = Fixture::with_package(package);
        f.seed();
        f.edit("a", "actual-edit", 1);
        let source = f.source(cards(), HistoryStart::Beginning, 10_000);
        let input = Invocation::new_transform(
            "changes-actual-store",
            Transform {
                handler: "changes.metadata".into(),
                input_type: "morrow.channel.directory.v1".into(),
                output_type: "morrow.changes.metadata.count.v1".into(),
                input: source.broker().directory().encode().unwrap(),
            },
        )
        .unwrap();
        let report =
            source
                .broker()
                .run_invocation(&f.manager, &mut f.host, &f.instance, &input, || 2);
        assert_eq!(report.execution.outcome, Ok(0), "{language}");
        assert!(report.failure.is_none(), "{language}");
        let output = report.output.unwrap();
        assert_eq!(output.type_id, "morrow.changes.metadata.count.v1");
        assert_eq!(output.bytes, 3u64.to_le_bytes());
        assert_eq!(joined(&source), Status::Closed);
        assert_eq!(source.broker().snapshot().last_acked, 3);
        let epoch = source.broker().endpoint().source_epoch;
        for sequence in 1..=3 {
            let receipt = f
                .host
                .store_local()
                .channel_ack_receipt(&SCOPE, &epoch, sequence)
                .unwrap()
                .unwrap();
            let frame = Frame::decode(&receipt.frame_wire).unwrap();
            let metadata = Metadata::decode(&frame.bytes).unwrap();
            assert!(cards().contains(&metadata.card_id));
            assert_eq!(frame.cursor, metadata.cursor().unwrap());
        }
        f.host.store_local().integrity_check().unwrap();
    }
}

fn reopen(f: Fixture, store_budget: EventBudget) -> Fixture {
    let Fixture {
        _dir,
        manager,
        host,
        instance,
        package,
    } = f;
    drop(instance);
    drop(host);
    let mut host =
        HostRuntime::new(Store::open_existing(&_dir.path().join("db"), store_budget).unwrap())
            .unwrap();
    let mut manager = manager;
    let instance = manager
        .connect(&package.manifest().package_id, &mut host)
        .unwrap();
    Fixture {
        _dir,
        manager,
        host,
        instance,
        package,
    }
}
fn material(frame: &Frame, reference: [u8; 32], serial: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let request = Request {
        call_id: [serial; 32],
        reference,
        source_epoch: frame.source_epoch,
        action: ack(frame),
    };
    let response = Response {
        call_id: request.call_id,
        request_sha256: request.digest().unwrap(),
        reference,
        source_epoch: frame.source_epoch,
        status: Status::Acked,
        frame: None,
        last_acked: frame.sequence,
        accepted_sequence: 0,
        resource_reclaimed: false,
    };
    (
        frame.encode().unwrap(),
        request.encode().unwrap(),
        response.encode().unwrap(),
    )
}
#[test]
fn actual_store_close_reopen_uses_latest_durable_anchor_with_fresh_receiver_and_epoch() {
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let first = f.frame(&source, 0);
    f.ack(&source, &first);
    let unacked = f.frame(&source, 1);
    assert_eq!(Metadata::decode(&unacked.bytes).unwrap().card_id, "b");
    source.revoke();
    joined(&source);
    let old = source.broker().endpoint();
    drop(source);
    let original = f
        .host
        .store_local()
        .channel_ack_receipt(&SCOPE, &old.source_epoch, 1)
        .unwrap()
        .unwrap();
    let mut f = reopen(f, EventBudget::default());
    assert_eq!(
        f.host
            .store_local()
            .channel_ack_receipt(&SCOPE, &old.source_epoch, 1)
            .unwrap(),
        Some(original)
    );
    f.edit("a", "new-after-reopen", 1);
    let source = f.source(
        cards(),
        HistoryStart::LatestAck {
            subscription: SCOPE,
            source_epoch: old.source_epoch,
        },
        10_000,
    );
    let endpoint = source.broker().endpoint();
    assert_ne!(endpoint.source_epoch, old.source_epoch);
    assert_ne!(endpoint.reference, old.reference);
    let second = f.frame(&source, 0);
    assert_eq!(
        Metadata::decode(&second.bytes).unwrap().operation_id,
        "create-b"
    );
    f.ack(&source, &second);
    let third = f.frame(&source, 1);
    assert_eq!(
        Metadata::decode(&third.bytes).unwrap().operation_id,
        "new-after-reopen"
    );
    f.ack(&source, &third);
    assert_eq!(joined(&source), Status::Closed);
    f.host.store_local().integrity_check().unwrap();
}
#[test]
fn latest_history_rejects_wrong_profile_cursor_epoch_missing_anchor_and_modified_receipt_facts() {
    // Invalid-history fixtures are deliberately written through the original
    // generic channel journal. They are not claimed as real source output.
    for mode in 0..7 {
        let mut f = Fixture::new();
        f.seed();
        let source = f.source(cards(), HistoryStart::Beginning, 10_000);
        let mut frame = f.frame(&source, 0);
        let mut metadata = Metadata::decode(&frame.bytes).unwrap();
        source.revoke();
        joined(&source);
        drop(source);
        match mode {
            0 => metadata.scope_digest = [0x55; 32],
            1 => metadata.window_id = [0x56; 32],
            2 => metadata.operation_id = "missing-anchor".into(),
            3 => metadata.revision += 1,
            4 => metadata.card_sha256 = [0x58; 32],
            _ => {}
        }
        frame.bytes = metadata.encode().unwrap();
        frame.cursor = metadata.cursor().unwrap().to_vec();
        if mode == 5 {
            frame.bytes[10] ^= 1;
        }
        if mode == 6 {
            frame.cursor[0] ^= 1;
        }
        let (fw, rw, aw) = material(&frame, [0x70; 32], 31);
        f.host
            .store_local_mut()
            .commit_channel_ack(&SCOPE, None, &fw, &rw, &aw)
            .unwrap();
        assert!(
            f.issue(
                cards(),
                HistoryStart::LatestAck {
                    subscription: SCOPE,
                    source_epoch: frame.source_epoch
                },
                10_000
            )
            .is_err(),
            "mode {mode}"
        );
    }
}
#[test]
fn original_receipt_cannot_reopen_in_foreign_logical_store_or_different_package() {
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let frame = f.frame(&source, 0);
    let response = f.ack(&source, &frame);
    assert_eq!(response.status, Status::Acked);
    let receipt = f
        .host
        .store_local()
        .channel_ack_receipt(&SCOPE, &frame.source_epoch, 1)
        .unwrap()
        .unwrap();
    source.revoke();
    joined(&source);
    drop(source);
    let mut g = Fixture::new();
    g.seed();
    g.host
        .store_local_mut()
        .commit_channel_ack(
            &SCOPE,
            None,
            &receipt.frame_wire,
            &receipt.request_wire,
            &receipt.response_wire,
        )
        .unwrap();
    assert!(
        g.issue(
            cards(),
            HistoryStart::LatestAck {
                subscription: SCOPE,
                source_epoch: frame.source_epoch
            },
            10_000
        )
        .is_err()
    );
    let next = package("2.0.0", true);
    f.manager.install_package(next.archive()).unwrap();
    f.manager.select(&next, f.manager.revision()).unwrap();
    f.manager
        .set_enabled(ID, next.digest(), true, f.manager.revision())
        .unwrap();
    f.instance = f.manager.connect(ID, &mut f.host).unwrap();
    f.package = next;
    assert!(
        f.issue(
            cards(),
            HistoryStart::LatestAck {
                subscription: SCOPE,
                source_epoch: frame.source_epoch
            },
            10_000
        )
        .is_err()
    );
}
#[test]
fn durable_duplicate_preserves_original_and_stale_cas_is_rejected_for_actual_source_frame() {
    use morrow_core::store::ChannelCommit;
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let first = f.frame(&source, 0);
    f.ack(&source, &first);
    let original = f
        .host
        .store_local()
        .channel_ack_receipt(&SCOPE, &first.source_epoch, 1)
        .unwrap()
        .unwrap();
    let second = f.frame(&source, 1);
    let (fw, rw, aw) = material(&second, source.broker().endpoint().reference, 32);
    assert_eq!(
        f.host
            .store_local_mut()
            .commit_channel_ack(&SCOPE, None, &fw, &rw, &aw),
        Err(morrow_core::Error::RevisionConflict)
    );
    let (_, duplicate_request, duplicate_response) =
        material(&first, source.broker().endpoint().reference, 33);
    assert!(matches!(
        f.host.store_local_mut().commit_channel_ack(
            &SCOPE,
            None,
            &original.frame_wire,
            &duplicate_request,
            &duplicate_response
        ),
        Ok(ChannelCommit::Duplicate(_))
    ));
    assert_eq!(
        f.host
            .store_local()
            .channel_ack_receipt(&SCOPE, &first.source_epoch, 1)
            .unwrap(),
        Some(original)
    );
    f.ack(&source, &second);
    assert_eq!(joined(&source), Status::Closed);
}
#[test]
fn actual_sqlite_writer_busy_rejects_original_source_ack_and_never_advances() {
    use std::sync::mpsc;
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let frame = f.frame(&source, 0);
    let mut lock_frame = frame.clone();
    lock_frame.source_epoch = [0x79; 32];
    let (fw, rw, aw) = material(&lock_frame, [0x78; 32], 34);
    let path = f._dir.path().join("db");
    let (acquired, ready) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let locker = thread::spawn(move || {
        let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
        let mut checks = 0;
        let result = store.commit_channel_ack_guarded(&[0x77; 32], None, &fw, &rw, &aw, || {
            checks += 1;
            assert_eq!(checks, 1);
            acquired.send(()).unwrap();
            held.recv_timeout(WAIT).unwrap();
            Err(morrow_core::Error::Invalid(
                "synthetic actual writer lock rollback",
            ))
        });
        assert!(result.is_err());
    });
    ready.recv_timeout(WAIT).unwrap();
    assert_eq!(f.call(&source, ack(&frame), 35), Err(Error::Unknown));
    release.send(()).unwrap();
    locker.join().unwrap();
    assert_eq!(joined(&source), Status::Unknown);
    f.no_ack(frame.source_epoch);
    assert_eq!(source.broker().snapshot().last_acked, 0);
}
#[test]
fn actual_store_byte_capacity_rejects_ack_without_refunding_or_releasing_second_event() {
    let mut f = Fixture::new();
    f.seed();
    let mut f = reopen(
        f,
        EventBudget {
            max_count: 1024,
            max_bytes: 1,
        },
    );
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let frame = f.frame(&source, 0);
    assert_eq!(f.call(&source, ack(&frame), 36), Err(Error::Unknown));
    f.no_ack(frame.source_epoch);
    assert_eq!(joined(&source), Status::Unknown);
    let snapshot = source.broker().snapshot();
    assert_eq!(snapshot.last_acked, 0);
    assert_eq!(snapshot.usage.messages, 1);
}
#[test]
fn separate_current_content_read_conflicts_after_revision_changes_and_does_not_read_history() {
    use morrow_core::{
        response::{Failure, Outcome, Response as CoreResponse},
        runtime::{Command, ReadContent},
    };
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let frame = f.frame(&source, 0);
    let metadata = Metadata::decode(&frame.bytes).unwrap();
    f.edit("a", "changed-after-notification", 1);
    let request = Command::ReadContent(ReadContent {
        request_id: "refresh".into(),
        card_id: "a".into(),
        expected_revision: metadata.revision,
        offset: 0,
        length: 32768,
    })
    .encode()
    .unwrap();
    let mut reader = f.host.connect().unwrap();
    let denied = CoreResponse::decode(&f.host.dispatch(&reader, &request, || 2).unwrap()).unwrap();
    assert_eq!(denied.outcome, Outcome::Rejected(Failure::Denied));
    f.host
        .grant(&mut reader, GrantKind::ReadContent, "a", 20_000, 2)
        .unwrap();
    let conflict =
        CoreResponse::decode(&f.host.dispatch(&reader, &request, || 2).unwrap()).unwrap();
    assert_eq!(
        conflict.outcome,
        Outcome::Rejected(Failure::RevisionConflict)
    );
    // Metadata receiver remains content-empty even after a separate reader gets a grant.
    assert!(
        f.host
            .content_authorization(
                f.instance.connection(),
                GrantKind::ReadContent,
                "a",
                None,
                2
            )
            .is_err()
    );
    source.revoke();
    joined(&source);
}
#[test]
fn actual_registry_publication_failure_invalidates_old_approval_before_failed_write() {
    use sha2::Digest;
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let frame = f.frame(&source, 0);
    // Obstruct the actual publication target, without moving the live owner
    // directory or its non-delete-sharing registry.lock lease on Windows.
    let root = f._dir.path().join("registry");
    let target = root.join("selection.morrow");
    let saved = root.join("selection-held.morrow");
    let original = std::fs::read(&target).unwrap();
    let original_sha256: [u8; 32] = sha2::Sha256::digest(&original).into();
    std::fs::rename(&target, &saved).unwrap();
    std::fs::create_dir(&target).unwrap();
    let revision = f.manager.revision();
    assert!(matches!(
        f.manager.set_enabled(ID, f.package.digest(), false, revision),
        Err(morrow_plugin_runtime::manager::ManagerError::Core(
            morrow_core::Error::Invalid("registry file type")
        ))
    ));
    assert_eq!(f.manager.revision(), revision);
    assert_eq!(joined(&source), Status::Revoked);
    f.no_ack(frame.source_epoch);
    assert_eq!(std::fs::read(&saved).unwrap(), original);
    std::fs::remove_dir(&target).unwrap();
    std::fs::rename(&saved, &target).unwrap();
    let restored = std::fs::read(&target).unwrap();
    assert_eq!(restored, original);
    assert_eq!(<[u8; 32]>::from(sha2::Sha256::digest(&restored)), original_sha256);
    assert!(!saved.exists());
    assert!(
        f.issue(cards(), HistoryStart::Beginning, 10_000).is_err(),
        "old instance never revives after rollback"
    );
    f.instance = f.manager.connect(ID, &mut f.host).unwrap();
    let fresh = f.source(cards(), HistoryStart::Beginning, 10_000);
    fresh.revoke();
    joined(&fresh);
}
#[test]
fn full_global_4096_receipts_does_not_refund_capacity_in_a_new_source_epoch() {
    use morrow_core::store::{ChannelCommit, MAX_CHANNEL_RECEIPTS};
    let mut f = Fixture::new();
    f.seed();
    let mut checkpoint = None;
    for sequence in 1..=MAX_CHANNEL_RECEIPTS as u64 {
        let frame = Frame {
            sequence,
            source_epoch: [0x93; 32],
            bytes: b"bounded old journal fixture".to_vec(),
            cursor: vec![],
        };
        let (fw, rw, aw) = material(&frame, [0x92; 32], 37);
        let result = f
            .host
            .store_local_mut()
            .commit_channel_ack(&[0x91; 32], checkpoint.as_ref(), &fw, &rw, &aw)
            .unwrap();
        let ChannelCommit::Committed(next) = result else {
            panic!("expected new bounded fixture receipt")
        };
        checkpoint = Some(next);
    }
    assert_eq!(checkpoint.unwrap().sequence, 4096);
    let original = f
        .host
        .store_local()
        .channel_ack_receipt(&[0x91; 32], &[0x93; 32], 1)
        .unwrap()
        .unwrap();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let frame = f.frame(&source, 0);
    assert_eq!(f.call(&source, ack(&frame), 38), Err(Error::Unknown));
    f.no_ack(frame.source_epoch);
    assert_eq!(joined(&source), Status::Unknown);
    assert_eq!(
        f.host
            .store_local()
            .channel_ack_receipt(&[0x91; 32], &[0x93; 32], 1)
            .unwrap(),
        Some(original)
    );
    assert_eq!(
        f.host
            .store_local()
            .channel_checkpoint(&[0x91; 32], &[0x93; 32])
            .unwrap()
            .unwrap()
            .sequence,
        4096
    );
    f.host.store_local().integrity_check().unwrap();
}
#[test]
fn dropping_original_host_terminates_and_joins_waiting_source_without_reentry() {
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let _ = f.frame(&source, 0);
    drop(f.host);
    assert_eq!(joined(&source), Status::Revoked);
}
#[test]
fn original_content_probe_deadline_caps_source_and_expires_while_waiting() {
    let mut f = Fixture::new();
    f.seed();
    let mut owner = f.host.connect().unwrap();
    f.host
        .grant(&mut owner, GrantKind::ReadContent, "a", 101, 1)
        .unwrap();
    let probe = f
        .host
        .content_authorization(&owner, GrantKind::ReadContent, "a", None, 1)
        .unwrap();
    let approval = ChangesApproval::issue(
        &f.manager,
        &f.host,
        &f.instance,
        f.manager.revision(),
        cards(),
        HistoryStart::Beginning,
        ChangesBudget::default(),
        10_001,
        1,
        vec![probe],
    )
    .unwrap();
    let source = approval
        .bind(&f.manager, &f.host, &f.instance, SCOPE, budget(10_000), 1)
        .unwrap();
    let frame = f.frame(&source, 0);
    thread::sleep(Duration::from_millis(125));
    assert_eq!(joined(&source), Status::Expired);
    f.no_ack(frame.source_epoch);
}
#[test]
fn actual_registry_commit_unknown_keeps_old_source_revoked_and_requires_fresh_reopen() {
    use morrow_core::plugin_package::registry::{RegistryStorage, sqlite::SqliteRegistryStorage};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Uncertain {
        storage: SqliteRegistryStorage,
        fail: Arc<AtomicBool>,
    }
    impl RegistryStorage for Uncertain {
        fn read(&self) -> morrow_core::Result<Option<Vec<u8>>> {
            self.storage.read()
        }
        fn load_package(&self, digest: [u8; 32]) -> morrow_core::Result<Package> {
            self.storage.load_package(digest)
        }
        fn install_package(&self, package: &Package) -> morrow_core::Result<()> {
            self.storage.install_package(package)
        }
        fn publish(&mut self, bytes: &[u8]) -> morrow_core::Result<()> {
            self.storage.publish(bytes)?; // Real durable commit precedes synthetic uncertain reply.
            if self.fail.load(Ordering::Acquire) {
                Err(morrow_core::Error::CommitUnknown)
            } else {
                Ok(())
            }
        }
    }
    let mut f = Fixture::new();
    f.seed();
    let fail = Arc::new(AtomicBool::new(false));
    let path = f._dir.path().join("uncertain-registry.db");
    let registry = Registry::from_storage(Box::new(Uncertain {
        storage: SqliteRegistryStorage::open(&path, true).unwrap(),
        fail: Arc::clone(&fail),
    }))
    .unwrap();
    let mut manager = Manager::new(registry, Limits::default());
    manager.install_package(f.package.archive()).unwrap();
    manager.select(&f.package, manager.revision()).unwrap();
    manager
        .set_enabled(ID, f.package.digest(), true, manager.revision())
        .unwrap();
    f.instance = manager.connect(ID, &mut f.host).unwrap();
    f.manager = manager;
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let frame = f.frame(&source, 0);
    fail.store(true, Ordering::Release);
    assert!(matches!(
        f.manager
            .set_enabled(ID, f.package.digest(), false, f.manager.revision()),
        Err(morrow_plugin_runtime::manager::ManagerError::Core(
            morrow_core::Error::CommitUnknown
        ))
    ));
    assert_eq!(joined(&source), Status::Revoked);
    f.no_ack(frame.source_epoch);
    assert!(f.issue(cards(), HistoryStart::Beginning, 10_000).is_err());
    drop(source);
    let replacement = Manager::new(
        Registry::from_storage(Box::new(
            SqliteRegistryStorage::open(&f._dir.path().join("spare-registry.db"), true).unwrap(),
        ))
        .unwrap(),
        Limits::default(),
    );
    drop(std::mem::replace(&mut f.manager, replacement));
    let registry =
        Registry::from_storage(Box::new(SqliteRegistryStorage::open(&path, true).unwrap()))
            .unwrap();
    let mut manager = Manager::new(registry, Limits::default());
    assert!(!manager.selection(ID).unwrap().enabled);
    manager
        .set_enabled(ID, f.package.digest(), true, manager.revision())
        .unwrap();
    f.instance = manager.connect(ID, &mut f.host).unwrap();
    f.manager = manager;
    let fresh = f.source(cards(), HistoryStart::Beginning, 10_000);
    fresh.revoke();
    joined(&fresh);
}
#[test]
fn replacing_store_inside_same_host_denies_delivery_and_ack_even_with_original_alive() {
    for same_logical_store in [false, true] {
        let mut f = Fixture::new();
        f.seed();
        let source = f.source(cards(), HistoryStart::Beginning, 10_000);
        let frame = f.frame(&source, 0);
        let replacement = if same_logical_store {
            Store::open_existing(&f._dir.path().join("db"), EventBudget::default()).unwrap()
        } else {
            Store::open(&f._dir.path().join("foreign"), EventBudget::default()).unwrap()
        };
        let original = std::mem::replace(f.host.store_local_mut(), replacement);
        // Original remains genuinely open and alive. Pure lifetime alone cannot
        // reject this: preflight must compare the current exact Store owner.
        let action = if same_logical_store {
            Action::Receive {
                last_acked: 0,
                credit_bytes: 1024,
            }
        } else {
            ack(&frame)
        };
        assert_eq!(f.call(&source, action, 40), Err(Error::Denied));
        assert!(
            original
                .channel_checkpoint(&SCOPE, &frame.source_epoch)
                .unwrap()
                .is_none()
        );
        f.no_ack(frame.source_epoch);
        assert_eq!(joined(&source), Status::Revoked);
        drop(std::mem::replace(f.host.store_local_mut(), original));
        assert_ne!(
            f.call(
                &source,
                Action::Receive {
                    last_acked: 0,
                    credit_bytes: 1024
                },
                41
            )
            .unwrap()
            .status,
            Status::Frame
        );
        f.no_ack(frame.source_epoch);
    }
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 10_000);
    let frame = f.frame(&source, 0);
    let replacement = Store::open(
        &f._dir.path().join("drop-replacement"),
        EventBudget::default(),
    )
    .unwrap();
    drop(std::mem::replace(f.host.store_local_mut(), replacement));
    // Without another guest call, dropping the original Store kills the pure
    // weak lifetime probe and the actual waiting producer exits and joins.
    assert_eq!(joined(&source), Status::Revoked);
    f.no_ack(frame.source_epoch);
}
#[cfg(feature = "fault-injection")]
#[test]
fn original_deadline_expiring_at_final_store_guard_rolls_back_both_rows() {
    let mut f = Fixture::new();
    f.seed();
    let source = f.source(cards(), HistoryStart::Beginning, 500);
    let frame = f.frame(&source, 0);
    source.expire_at_final_commit_for_fault_test().unwrap();
    assert_eq!(f.call(&source, ack(&frame), 42), Err(Error::Expired));
    assert!(
        source.final_commit_expiry_observed_for_fault_test(),
        "expiry must occur at the actual second Store callback after both SQL writes"
    );
    f.no_ack(frame.source_epoch);
    assert_eq!(source.broker().snapshot().last_acked, 0);
    assert_eq!(joined(&source), Status::Expired);
    f.host.store_local().integrity_check().unwrap();
}
