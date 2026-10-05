//! Real typed SDK guests over the unchanged managed native WebSocket source.
#![cfg(all(feature = "managed-websocket", not(target_arch = "wasm32")))]
#[path = "support/ws_sdk_support.rs"]
mod support;
use morrow_core::{
    channel::{Budget, Frame, Kind},
    dispatch::HostRuntime,
    io_intent::Phase,
    plugin_package::{Package, catalog::Catalog, registry::Registry},
    store::Store,
    task::{Invocation, Transform},
};
use morrow_network_node_stream::{
    Limits, RawHttpRequest,
    managed_ws::{Approval, Completion, Error, MessageEnvelope, NetworkProfile, WsSource},
    websocket::{MessageKind, Quotas},
};
use morrow_plugin_runtime::{
    Limits as RuntimeLimits,
    channel::{ChannelBroker, CleanupProof, Source},
    manager::{ManagedInstance, Manager},
};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};
use support::{BINARY, CONTROL, Peer, TEXT, WAIT, until};
const SCOPE: [u8; 32] = [0x79; 32];
const HANDLER: &str = "channel.ws.sdk";
struct Fixture {
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    broker: ChannelBroker,
    dir: tempfile::TempDir,
}
impl Fixture {
    fn new(package: &Package, duration_ms: u64) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let declared = package.channel_declaration().unwrap();
        let mut budget = Budget::from_proto(declared.budget.as_ref().unwrap()).unwrap();
        budget.max_duration_ms = duration_ms.min(budget.max_duration_ms);
        let execution = package.manifest().budget.as_ref().unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(
            registry,
            RuntimeLimits {
                host_calls: execution.host_calls,
                fuel: execution.fuel,
                memory_bytes: usize::try_from(execution.memory_bytes).unwrap(),
            },
        );
        manager.select(package, manager.revision()).unwrap();
        manager
            .set_enabled(
                &package.manifest().package_id,
                package.digest(),
                true,
                manager.revision(),
            )
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap();
        let instance = manager
            .connect(&package.manifest().package_id, &mut host)
            .unwrap();
        let broker = manager
            .bind_channel(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                Source {
                    kind: Kind::Events,
                    duplex: true,
                    checkpoint_scope: Some(SCOPE),
                },
                budget,
                1 + budget.max_duration_ms,
                1,
            )
            .unwrap();
        Self {
            manager,
            host,
            instance,
            broker,
            dir,
        }
    }
    fn no_ack(&self) {
        let epoch = self.broker.endpoint().source_epoch;
        assert_eq!(self.broker.snapshot().last_acked, 0);
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
    fn source(&self, a: Approval) -> WsSource {
        WsSource::approve(
            &self.manager,
            &self.host,
            &self.instance,
            &self.broker,
            self.manager.revision(),
            a,
            1,
        )
        .unwrap()
    }
    fn start(&mut self, s: &WsSource) {
        s.start(&self.manager, &mut self.host, &self.instance, &self.broker)
            .unwrap();
    }
    fn joined(&self, s: &WsSource) -> Arc<Completion> {
        let done = s
            .wait_completion(WAIT)
            .expect("actual bounded source completion");
        until(|| {
            self.broker.reap();
            self.broker.snapshot().resource_reclaimed
        });
        assert_eq!(self.broker.snapshot().cleanup_proof, CleanupProof::Joined);
        let ws = done
            .websocket
            .as_ref()
            .or_else(|| match done.opening_cleanup.as_ref() {
                Some(morrow_network_node_stream::websocket::OpeningCleanup::Worker(c)) => Some(c),
                _ => None,
            })
            .expect("started socket retains original cleanup receipt");
        assert!(
            ws.worker_joined,
            "socket task receipt separate from native broker OS thread"
        );
        self.host.store_local().integrity_check().unwrap();
        done
    }
    fn unknown(&self, s: &WsSource) {
        assert_eq!(
            self.host
                .store_local()
                .lookup_matching_io_intent(s.command())
                .unwrap()
                .unwrap()
                .phase(),
            Phase::OutcomeUnknown
        );
        let reopened = Store::open(&self.dir.path().join("db"), Default::default()).unwrap();
        assert_eq!(
            reopened
                .lookup_matching_io_intent(s.command())
                .unwrap()
                .unwrap()
                .phase(),
            Phase::OutcomeUnknown
        );
        reopened.integrity_check().unwrap();
    }
    fn invocation(&self) -> Invocation {
        let endpoint = self.broker.endpoint();
        let mut input = vec![5];
        input.extend(endpoint.reference);
        input.extend(endpoint.source_epoch);
        Invocation::new_transform(
            "c04-typed-ws",
            Transform {
                handler: HANDLER.into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                input,
            },
        )
        .unwrap()
    }
}
fn approval(origin: &str, op: &str) -> Approval {
    let mut quotas = Quotas::default();
    quotas.max_message_bytes = 1024;
    quotas.max_frame_bytes = 1024;
    quotas.max_pending_bytes = 4096;
    quotas.max_pending_messages = 2;
    quotas.max_incoming_wire_bytes = 65536;
    quotas.max_outgoing_wire_bytes = 65536;
    Approval {
        request: RawHttpRequest {
            method: "GET".into(),
            target: format!("{origin}/synthetic-ws"),
            headers: vec![],
            body: vec![],
        },
        operation_id: op.into(),
        approval_epoch: [0x61; 32],
        deadline: Instant::now() + Duration::from_secs(3),
        profile: NetworkProfile::LoopbackWs,
        limits: Limits {
            max_request_bytes: 65536,
            max_response_bytes: 65536,
            max_header_bytes: 4096,
            max_concurrent: 1,
            timeout: Duration::from_secs(3),
        },
        subprotocols: vec![],
        quotas,
        max_encoded_bytes: 32768,
    }
}
fn message(kind: MessageKind, payload: &[u8], close_code: Option<u16>) -> MessageEnvelope {
    MessageEnvelope {
        kind,
        payload: payload.to_vec(),
        close_code,
    }
}

fn packages() -> Vec<(&'static str, Package)> {
    ["RUST", "C", "CPP"]
        .into_iter()
        .map(|language| {
            let mut inputs = Vec::new();
            for kind in ["WASM", "PACKAGE"] {
                let key = format!("MORROW_C04_WS_{language}_{kind}");
                let path = PathBuf::from(
                    std::env::var_os(&key).expect("fresh typed SDK artifact path mandatory"),
                );
                let bytes = std::fs::read(&path).unwrap();
                let pin =
                    std::env::var(format!("{key}_SHA256")).expect("exact artifact SHA mandatory");
                assert_eq!(pin.len(), 64);
                assert_eq!(
                    format!("{:x}", Sha256::digest(&bytes)),
                    pin.to_ascii_lowercase()
                );
                inputs.push(bytes);
            }
            let package = Package::decode(&inputs[1]).unwrap();
            assert_eq!(package.module(), inputs[0]);
            assert_eq!(
                package.manifest().package_id,
                format!("org.morrow.ws.sdk.{}", language.to_ascii_lowercase())
            );
            assert_eq!(
                package.manifest().required_features,
                vec!["transform-handlers-v1", "channel-v1"]
            );
            assert!(package.capabilities().is_empty() && package.io_declaration().is_none());
            assert_eq!(package.manifest().transform_handlers[0].handler, HANDLER);
            eprintln!(
                "C04 typed {language}: pinned fresh Wasm and package, no guest socket authority"
            );
            (language, package)
        })
        .collect()
}
#[test]
fn three_typed_sdk_guests_interleave_text_binary_controls_close_with_exact_durable_ack() {
    for (language, package) in packages() {
        let peer = Peer::new(false);
        let mut f = Fixture::new(&package, 4000);
        let source = f.source(approval(&peer.origin, "c04-typed-mixed"));
        f.start(&source);
        let invocation = f.invocation();
        let result =
            f.broker
                .run_invocation(&f.manager, &mut f.host, &f.instance, &invocation, || 2);
        eprintln!(
            "C04 {language} execution={:?} host_calls={} fuel={} broker_messages={}",
            result.execution.outcome,
            result.execution.host_calls,
            result.execution.fuel_remaining,
            f.broker.snapshot().usage.messages
        );
        if result.execution.outcome != Ok(0) {
            eprintln!(
                "C04 cold failure {language}: task_failure={:?} peer_handshakes={} peer_messages={:?} broker={:?} source_before={:?}",
                result.failure,
                peer.seen.handshakes.load(Ordering::SeqCst),
                peer.seen.messages.lock().unwrap(),
                f.broker.snapshot(),
                source.completion()
            );
            eprintln!(
                "C04 cold failure {language}: source_after={:?}",
                source.wait_completion(WAIT)
            );
        }
        assert_eq!(result.execution.outcome, Ok(0), "{language}");
        assert!(result.failure.is_none());
        let expected = [
            message(MessageKind::Text, TEXT.as_bytes(), None),
            message(MessageKind::Binary, BINARY, None),
            message(MessageKind::Pong, CONTROL, None),
            message(MessageKind::Ping, CONTROL, None),
            message(MessageKind::Close, &[], Some(1000)),
        ];
        let wires: Vec<_> = expected.iter().map(|m| m.encode().unwrap()).collect();
        let mut digest = Sha256::new();
        for wire in &wires {
            digest.update(wire);
        }
        let output = result.output.unwrap();
        assert_eq!(output.type_id, "bytes");
        assert_eq!(output.bytes.len(), 64);
        assert_eq!(&output.bytes[..4], b"WSS1");
        assert_eq!(
            u32::from_le_bytes(output.bytes[4..8].try_into().unwrap()),
            5
        );
        assert_eq!(
            u32::from_le_bytes(output.bytes[8..12].try_into().unwrap()),
            5
        );
        assert!(
            u32::from_le_bytes(output.bytes[12..16].try_into().unwrap()) <= 1,
            "wire flag cannot prove join"
        );
        assert_eq!(
            u64::from_le_bytes(output.bytes[16..24].try_into().unwrap()),
            5
        );
        assert_eq!(
            u64::from_le_bytes(output.bytes[24..32].try_into().unwrap()),
            wires.iter().map(Vec::len).sum::<usize>() as u64
        );
        assert_eq!(&output.bytes[32..], digest.finalize().as_slice());
        let done = f.joined(&source);
        assert!(done.outcome.is_ok());
        assert_eq!(
            (
                done.messages_queued,
                done.messages_acked,
                done.outgoing_written
            ),
            (5, 5, 5)
        );
        assert!(done.websocket.as_ref().unwrap().peer_close.is_some());
        let epoch = f.broker.endpoint().source_epoch;
        for (i, (wire, semantic)) in wires.iter().zip(&expected).enumerate() {
            let receipt = f
                .host
                .store_local()
                .channel_ack_receipt(&SCOPE, &epoch, i as u64 + 1)
                .unwrap()
                .unwrap();
            let frame = Frame::decode(&receipt.frame_wire).unwrap();
            assert_eq!(frame.sequence, i as u64 + 1);
            assert_eq!(&frame.bytes, wire);
            assert_eq!(MessageEnvelope::decode(&frame.bytes).unwrap(), *semantic);
            assert_eq!(receipt.checkpoint.frame_sha256, frame.digest().unwrap());
            assert_eq!(receipt.checkpoint.cursor, frame.cursor);
            assert_eq!(frame.cursor.len(), 32);
        }
        f.unknown(&source);
        assert_eq!(
            source.start(&f.manager, &mut f.host, &f.instance, &f.broker),
            Err(Error::AlreadyStarted)
        );
        let seen = peer.finish();
        assert_eq!(seen.handshakes.load(Ordering::SeqCst), 1);
        assert_eq!(
            seen.messages.lock().unwrap().as_slice(),
            &[
                (1, TEXT.as_bytes().to_vec()),
                (2, BINARY.to_vec()),
                (9, CONTROL.to_vec()),
                (10, CONTROL.to_vec()),
                (10, CONTROL.to_vec()),
                (8, 1000u16.to_be_bytes().to_vec())
            ]
        );
        // Five admitted guest sends include one explicit Pong. The second peer-observed
        // Pong is the mature backend's automatic reply to incoming Ping, not business success.
        assert_eq!(seen.auto_pongs.load(Ordering::SeqCst), 1);
        assert!(seen.closed.load(Ordering::SeqCst));
    }
}
#[test]
fn live_source_revoke_and_original_instance_stop_interrupt_each_actual_sdk_guest_without_retry() {
    for (language, package) in packages() {
        for stop in [false, true] {
            let peer = Peer::new(true);
            let mut f = Fixture::new(&package, 4000);
            let a = approval(&peer.origin, "c04-typed-live-control");
            let source = f.source(a.clone());
            f.start(&source);
            until(|| peer.seen.handshakes.load(Ordering::SeqCst) == 1);
            let invocation = f.invocation();
            let seen = peer.seen.clone();
            let revoke = source.clone();
            let result = thread::scope(|scope| {
                let instance = &f.instance;
                let controller = scope.spawn(move || {
                    until(|| seen.messages.lock().unwrap().len() == 1);
                    if stop {
                        instance.request_stop()
                    } else {
                        revoke.revoke()
                    }
                });
                let result = f.broker.run_invocation(
                    &f.manager,
                    &mut f.host,
                    &f.instance,
                    &invocation,
                    || 2,
                );
                controller.join().unwrap();
                result
            });
            assert_ne!(
                result.execution.outcome,
                Ok(0),
                "{language}: live guest transport must fail closed"
            );
            assert!(result.output.is_none());
            let done = f.joined(&source);
            assert_eq!(
                done.outcome,
                Err(Error::Source(morrow_plugin_runtime::channel::Error::Denied))
            );
            assert_eq!(done.messages_acked, 0);
            // Peer receipt is established before revocation. The managed post-send guard
            // may deny the local completion receipt; zero does not mean unsent or retryable.
            assert!(done.outgoing_written <= 1);
            eprintln!(
                "C04 {language} stop={stop} peer_received=1 outgoing_completion_receipts={}",
                done.outgoing_written
            );
            f.no_ack();
            f.unknown(&source);
            assert_eq!(
                source.start(&f.manager, &mut f.host, &f.instance, &f.broker),
                Err(Error::AlreadyStarted)
            );
            let seen = peer.finish();
            assert_eq!(seen.handshakes.load(Ordering::SeqCst), 1);
            assert_eq!(
                seen.messages.lock().unwrap().as_slice(),
                &[(1, TEXT.as_bytes().to_vec())]
            );
        }
    }
}
