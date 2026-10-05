//! Real typed SDK guests over the unchanged managed native SSE source.
#![cfg(all(feature = "managed-channel", not(target_arch = "wasm32")))]
#[path = "support/sse_sdk_support.rs"]
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
    managed_sse::{Approval, Completion, Error, EventEnvelope, NetworkProfile, SseSource},
    sse::DecoderLimits,
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
use support::{BODY, HEAD, Server, WAIT, complete_stream_parts, first_event_only, until};
const SCOPE: [u8; 32] = [0x7a; 32];
const HANDLER: &str = "channel.sse.sdk";
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
                    duplex: false,
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
    fn competitor(&self, a: Approval) -> (ChannelBroker, SseSource) {
        let endpoint = self.broker.endpoint();
        let broker = self
            .manager
            .bind_channel(
                &self.host,
                &self.instance,
                self.instance.package().package().digest(),
                self.manager.revision(),
                Source {
                    kind: Kind::Events,
                    duplex: false,
                    checkpoint_scope: Some([0x7b; 32]),
                },
                endpoint.budget,
                1 + endpoint.budget.max_duration_ms,
                1,
            )
            .unwrap();
        let source = SseSource::approve(
            &self.manager,
            &self.host,
            &self.instance,
            &broker,
            self.manager.revision(),
            a,
            1,
        )
        .unwrap();
        (broker, source)
    }
    fn source(&self, a: Approval) -> SseSource {
        SseSource::approve(
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
    fn start(&mut self, s: &SseSource) {
        s.start(&self.manager, &mut self.host, &self.instance, &self.broker)
            .unwrap();
    }
    fn joined(&self, s: &SseSource) -> Arc<Completion> {
        let done = s
            .wait_completion(WAIT)
            .expect("actual bounded source completion");
        until(|| {
            self.broker.reap();
            self.broker.snapshot().resource_reclaimed
        });
        assert_eq!(self.broker.snapshot().cleanup_proof, CleanupProof::Joined);
        let transport = done
            .transport
            .as_ref()
            .or_else(|| done.sse.as_ref().map(|s| &s.transport))
            .expect("actual HTTP completion receipt");
        assert!(
            transport.worker_joined,
            "HTTP worker join separate from broker producer join"
        );
        self.host.store_local().integrity_check().unwrap();
        done
    }
    fn unknown(&self, s: &SseSource) {
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
            "c05-typed-sse",
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
fn packages() -> Vec<(&'static str, Package)> {
    ["RUST", "C", "CPP"]
        .into_iter()
        .map(|language| {
            let mut inputs = Vec::new();
            for kind in ["WASM", "PACKAGE"] {
                let key = format!("MORROW_C05_SSE_{language}_{kind}");
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
                format!("org.morrow.sse.sdk.{}", language.to_ascii_lowercase())
            );
            assert_eq!(
                package.manifest().required_features,
                vec!["transform-handlers-v1", "channel-v1"]
            );
            assert!(package.capabilities().is_empty() && package.io_declaration().is_none());
            assert_eq!(package.manifest().transform_handlers[0].handler, HANDLER);
            eprintln!(
                "C05 typed {language}: pinned fresh Wasm and package, no guest socket authority"
            );
            (language, package)
        })
        .collect()
}

fn approval(origin: &str, operation: &str) -> Approval {
    Approval {
        request: RawHttpRequest {
            method: "POST".into(),
            target: format!("{origin}/synthetic-events"),
            headers: vec![("Content-Type".into(), b"application/json".to_vec())],
            body: BODY.to_vec(),
        },
        operation_id: operation.into(),
        approval_epoch: [0x51; 32],
        deadline: Instant::now() + Duration::from_secs(3),
        profile: NetworkProfile::LoopbackHttp,
        limits: Limits {
            max_request_bytes: 4096,
            max_response_bytes: 32768,
            max_header_bytes: 4096,
            max_concurrent: 1,
            timeout: Duration::from_secs(3),
        },
        decoder_limits: DecoderLimits {
            max_line_bytes: 1024,
            max_event_bytes: 4096,
            max_total_bytes: 32768,
            max_events: 16,
            max_id_bytes: 1024,
            max_retry_digits: 20,
        },
        max_encoded_bytes: 32768,
    }
}
fn expected() -> Vec<EventEnvelope> {
    [
        ("你\nsecond", "delta", "i".repeat(300), None),
        ("", "message", String::new(), Some(0)),
        ("最终 🙂", "更新", String::new(), Some(u64::MAX)),
        ("[DONE]", "message", String::new(), Some(u64::MAX)),
        ("after-DONE", "message", String::new(), Some(u64::MAX)),
    ]
    .into_iter()
    .map(|(data, event, id, retry)| EventEnvelope {
        data: data.into(),
        event: event.into(),
        id,
        retry,
    })
    .collect()
}
fn check_receipts(f: &Fixture, count: usize) -> Vec<Vec<u8>> {
    let epoch = f.broker.endpoint().source_epoch;
    let events = expected();
    let mut wires = Vec::new();
    for (i, event) in events.iter().take(count).enumerate() {
        let receipt = f
            .host
            .store_local()
            .channel_ack_receipt(&SCOPE, &epoch, i as u64 + 1)
            .unwrap()
            .unwrap();
        let frame = Frame::decode(&receipt.frame_wire).unwrap();
        let decoded = EventEnvelope::decode(&frame.bytes).unwrap();
        assert_eq!(frame.sequence, i as u64 + 1);
        assert_eq!(decoded.data, event.data);
        assert_eq!(decoded.event, event.event);
        assert_eq!(decoded.id, event.id);
        assert_eq!(decoded.retry, event.retry);
        assert_eq!(frame.bytes, event.encode().unwrap());
        assert_eq!(receipt.checkpoint.frame_sha256, frame.digest().unwrap());
        assert_eq!(receipt.checkpoint.cursor, frame.cursor);
        assert_eq!(frame.cursor.len(), 32);
        eprintln!(
            "C05 original ACK{} wire={} digest={:x?} metadata data={:?} event={:?} id_len={} retry={:?}",
            i + 1,
            frame.bytes.len(),
            receipt.checkpoint.frame_sha256,
            decoded.data,
            decoded.event,
            decoded.id.len(),
            decoded.retry
        );
        eprintln!(
            "C05 original frame_wire={:02x?} payload_wire={:02x?} cursor={:02x?} full_id={:?}",
            receipt.frame_wire, frame.bytes, frame.cursor, decoded.id
        );
        wires.push(frame.bytes);
    }
    assert_eq!(f.broker.snapshot().last_acked, count as u64);
    wires
}
fn single_post(observed: &support::Observations) {
    assert_eq!(observed.posts(), 1);
    let requests = observed.requests.lock().unwrap();
    let request = String::from_utf8_lossy(&requests[0]).to_ascii_lowercase();
    assert!(
        !request.contains("last-event-id:")
            && !request.contains("authorization:")
            && !request.contains("cookie:")
    );
}
#[test]
fn three_typed_sdk_guests_preserve_full_event_metadata_and_ack_before_http_eof() {
    for (language, package) in packages() {
        let server = Server::new(HEAD, complete_stream_parts(), true);
        let mut f = Fixture::new(&package, 4000);
        let a = approval(&server.origin, "c05-typed-full-events");
        let source = f.source(a.clone());
        let (other, competitor) = f.competitor(a);
        assert_eq!(source.command(), competitor.command());
        f.start(&source);
        let invocation = f.invocation();
        let result = thread::scope(|scope| {
            let broker = &f.broker;
            let observed = server.observed.clone();
            let eof = scope.spawn(move || {
                until(|| broker.snapshot().last_acked == 5);
                assert!(!observed.eof_written.load(Ordering::SeqCst));
                observed.release();
            });
            let result =
                f.broker
                    .run_invocation(&f.manager, &mut f.host, &f.instance, &invocation, || 2);
            eof.join().unwrap();
            result
        });
        eprintln!(
            "C05 cold {language} execution={:?} calls={} failure={:?}",
            result.execution.outcome, result.execution.host_calls, result.failure
        );
        assert_eq!(result.execution.outcome, Ok(0), "{language}");
        assert!(result.failure.is_none());
        let output = result.output.unwrap();
        let wires = check_receipts(&f, 5);
        let mut digest = Sha256::new();
        for wire in &wires {
            digest.update(wire);
        }
        assert_eq!(output.type_id, "bytes");
        assert_eq!(output.bytes.len(), 64);
        assert_eq!(&output.bytes[..4], b"SES1");
        assert_eq!(
            u32::from_le_bytes(output.bytes[4..8].try_into().unwrap()),
            5
        );
        assert_eq!(
            u32::from_le_bytes(output.bytes[8..12].try_into().unwrap()),
            5
        );
        assert!(u32::from_le_bytes(output.bytes[12..16].try_into().unwrap()) <= 1);
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
        assert_eq!((done.events_queued, done.events_acked), (5, 5));
        assert_eq!(
            done.encoded_bytes,
            wires.iter().map(Vec::len).sum::<usize>() as u64
        );
        let sse = done.sse.as_ref().unwrap();
        assert_eq!(sse.delivered_events, 5);
        assert!(sse.decoder_reached_http_eof && !sse.truncated);
        f.unknown(&source);
        assert_eq!(
            source.start(&f.manager, &mut f.host, &f.instance, &f.broker),
            Err(Error::AlreadyStarted)
        );
        assert_eq!(
            competitor.start(&f.manager, &mut f.host, &f.instance, &other),
            Err(Error::OutcomeUnknown)
        );
        other.cleanup();
        assert_eq!(other.snapshot().cleanup_proof, CleanupProof::NoProducer);
        let observed = server.finish();
        assert!(observed.eof_written.load(Ordering::SeqCst));
        single_post(&observed);
    }
}
#[test]
fn live_source_revoke_and_original_instance_stop_preserve_ack1_without_retry_for_each_guest() {
    for (language, package) in packages() {
        for stop in [false, true] {
            let server = Server::new(HEAD, first_event_only(), true);
            let mut f = Fixture::new(&package, 4000);
            let source = f.source(approval(&server.origin, "c05-typed-live-control"));
            f.start(&source);
            let invocation = f.invocation();
            let revoke = source.clone();
            let result = thread::scope(|scope| {
                let instance = &f.instance;
                let broker = &f.broker;
                let controller = scope.spawn(move || {
                    until(|| broker.snapshot().last_acked == 1);
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
            assert_ne!(result.execution.outcome, Ok(0));
            assert!(result.output.is_none());
            check_receipts(&f, 1);
            let epoch = f.broker.endpoint().source_epoch;
            let before = f
                .host
                .store_local()
                .channel_ack_receipt(&SCOPE, &epoch, 1)
                .unwrap()
                .unwrap();
            let done = f.joined(&source);
            eprintln!(
                "C05 actual control first cause {language} stop={stop}: done={:?} grant={:?} execution={:?} task_failure={:?}",
                done,
                source.grant().check(),
                result.execution.outcome,
                result.failure
            );
            // The same original grant can be observed first at the producer or
            // its awaited HTTP pull. Preserve either exact native first denial.
            let original_denial = matches!(
                done.outcome,
                Err(Error::Source(morrow_plugin_runtime::channel::Error::Denied))
                    | Err(Error::Sse(
                        morrow_network_node_stream::sse::Error::Transport(
                            morrow_network_node_stream::Error::Denied
                        )
                    ))
            );
            // source.revoke directly cancels the original HTTP token; this path
            // was observed retaining Cancelled first. Guard::check can also cancel.
            let explicit_source_cancel = !stop
                && done.outcome
                    == Err(Error::Sse(
                        morrow_network_node_stream::sse::Error::Transport(
                            morrow_network_node_stream::Error::Cancelled,
                        ),
                    ));
            assert!(original_denial || explicit_source_cancel);
            assert_eq!(
                source.grant().check(),
                Err(morrow_plugin_runtime::channel::Error::Denied)
            );
            assert_eq!(done.events_queued, 1);
            assert!(done.events_acked <= 1);
            assert_eq!(
                f.host
                    .store_local()
                    .channel_ack_receipt(&SCOPE, &epoch, 1)
                    .unwrap()
                    .unwrap(),
                before
            );
            assert!(
                f.host
                    .store_local()
                    .channel_ack_receipt(&SCOPE, &epoch, 2)
                    .unwrap()
                    .is_none()
            );
            eprintln!(
                "C05 {language} stop={stop} durable_ack=1 completion_ack_receipts={} source={:?} failure={:?}",
                done.events_acked, done.outcome, result.failure
            );
            f.unknown(&source);
            assert_eq!(
                source.start(&f.manager, &mut f.host, &f.instance, &f.broker),
                Err(Error::AlreadyStarted)
            );
            let observed = server.finish();
            assert!(!observed.eof_written.load(Ordering::SeqCst));
            single_post(&observed);
        }
    }
}
