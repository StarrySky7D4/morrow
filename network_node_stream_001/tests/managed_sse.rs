//! W14 real managed public channel/ACK integration on disposable localhost HTTP.
//! The three languages use ROOT-built NEW artifacts from unchanged SDK examples;
//! these are not old SDK014 or the frozen57 packages. Other guests are labeled WAT.
#![cfg(all(feature = "managed-channel", not(target_arch = "wasm32")))]
#[path = "support/managed_sse_support.rs"] mod support;
use support::{Server, HEAD, WAIT, until};
use morrow_core::{
    channel::{Action, Budget, Frame, Kind, Request, Response, Status},
    dispatch::HostRuntime, io_intent::Phase,
    plugin_package::{Package, catalog::Catalog, registry::Registry, proto::TransformHandler},
    store::Store, task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Limits as RuntimeLimits,
    channel::{ChannelBroker, CleanupProof, Source}, manager::{ManagedInstance, Manager},
};
use morrow_network_node_stream::{
    Limits, RawHttpRequest, sse::DecoderLimits,
    managed_sse::{Approval, Completion, EventEnvelope, NetworkProfile, SseSource},
};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}}, thread, time::{Duration, Instant}};
const SCOPE: [u8;32] = [0x76;32];
const HANDLER: &str = "channel.exercise";
const BODY: &[u8] = b"{\"synthetic\":true}";
struct Fixture {
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    broker: ChannelBroker,
    // Windows: every Store/host and actual worker is released before TempDir.
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
        let mut manager = Manager::new(registry, RuntimeLimits {
            host_calls: execution.host_calls, fuel: execution.fuel,
            memory_bytes: usize::try_from(execution.memory_bytes).unwrap(),
        });
        manager.select(package, manager.revision()).unwrap();
        manager.set_enabled(&package.manifest().package_id, package.digest(), true, manager.revision()).unwrap();
        let mut host = HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap()).unwrap();
        let instance = manager.connect(&package.manifest().package_id, &mut host).unwrap();
        let broker = manager.bind_channel(&host, &instance, package.digest(), manager.revision(), Source {
            kind: Kind::Events, duplex: false, checkpoint_scope: Some(SCOPE),
        }, budget, 1 + budget.max_duration_ms, 1).unwrap();
        Self { manager, host, instance, broker, dir }
    }
    fn wat() -> Self { Self::new(&wat_package(false), 4000) }
    fn request(&self, action: Action, serial: u8) -> Request {
        let endpoint = self.broker.endpoint();
        Request { call_id: [serial;32], reference: endpoint.reference, source_epoch: endpoint.source_epoch, action }
    }
    fn call(&mut self, action: Action, serial: u8) -> Response {
        let request = self.request(action, serial);
        self.broker.dispatch(&self.manager, &mut self.host, &self.instance, &request, 2).unwrap()
    }
    fn frame(&mut self) -> Frame {
        let mut serial = 10;
        let until_at = Instant::now() + WAIT;
        loop {
            let reply = self.call(Action::Receive { last_acked: self.broker.snapshot().last_acked,
                credit_bytes: self.broker.endpoint().budget.max_frame_bytes }, serial);
            if reply.status == Status::Frame { return reply.frame.unwrap(); }
            assert_eq!(reply.status, Status::Idle, "no false terminal before expected frame");
            assert!(Instant::now() < until_at, "frame arrival bound");
            serial = serial.wrapping_add(1).max(10); thread::sleep(Duration::from_millis(1));
        }
    }
    fn ack(&mut self, frame: &Frame, serial: u8) -> Response {
        self.call(Action::Ack { sequence: frame.sequence, frame_sha256: frame.digest().unwrap(), cursor: frame.cursor.clone() }, serial)
    }
    fn no_ack(&self) {
        let epoch = self.broker.endpoint().source_epoch;
        assert_eq!(self.broker.snapshot().last_acked, 0);
        assert!(self.host.store_local().channel_checkpoint(&SCOPE, &epoch).unwrap().is_none());
        assert!(self.host.store_local().channel_ack_receipt(&SCOPE, &epoch, 1).unwrap().is_none());
    }
    fn original_control_active(&self) { assert!(!self.host.revocation(self.instance.connection()).unwrap().is_revoked()); }
    fn source(&self, approval: Approval) -> SseSource {
        SseSource::approve(&self.manager, &self.host, &self.instance, &self.broker,
            self.manager.revision(), approval, 1).unwrap()
    }
    fn start(&mut self, source: &SseSource) {
        source.start(&self.manager, &mut self.host, &self.instance, &self.broker).unwrap();
    }
    fn joined(&self, source: &SseSource) -> Arc<Completion> {
        let done = source.wait_completion(WAIT).expect("bounded actual source completion");
        until(|| { self.broker.reap(); self.broker.snapshot().resource_reclaimed });
        assert_eq!(self.broker.snapshot().cleanup_proof, CleanupProof::Joined);
        let transport = done.transport.as_ref().or_else(|| done.sse.as_ref().map(|s| &s.transport))
            .expect("started localhost HTTP retains actual cleanup receipt");
        assert!(transport.worker_joined, "HTTP worker join, separate from producer join");
        self.host.store_local().integrity_check().unwrap();
        done
    }
    fn assert_unknown(&self, source: &SseSource) {
        let record = self.host.store_local().lookup_matching_io_intent(source.command()).unwrap().unwrap();
        assert_eq!(record.phase(), Phase::OutcomeUnknown, "HTTP EOF is not business Observed");
        let reopened = Store::open(&self.dir.path().join("db"), Default::default()).unwrap();
        assert_eq!(reopened.lookup_matching_io_intent(source.command()).unwrap().unwrap().phase(), Phase::OutcomeUnknown);
        reopened.integrity_check().unwrap();
    }
    fn invocation(&self) -> Invocation {
        let endpoint = self.broker.endpoint();
        let mut input = vec![2]; input.extend(endpoint.reference); input.extend(endpoint.source_epoch);
        Invocation::new_transform("w14-public-sse", Transform { handler: HANDLER.into(), input_type: "bytes".into(),
            output_type: "bytes".into(), input }).unwrap()
    }
}
fn wat_package(trap: bool) -> Package {
    // Newly constructed WAT is a fault fixture, never an original language guest.
    let body = if trap { "unreachable" } else { "i32.const 0" };
    let wasm = wat::parse_str(format!(r#"(module
        (import "morrow_channel_v1" "call" (func (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 4) (func (export "morrow_run") (result i32) {body}))"#)).unwrap();
    let mut manifest = Package::manifest_for_transform("org.example.w14.sse.wat", "0.1.0", &wasm,
        vec![TransformHandler { handler: HANDLER.into(), input_type: "bytes".into(), output_type: "bytes".into(), max_input_bytes:65, max_output_bytes:64 }]);
    manifest.required_features.push(morrow_core::channel::FEATURE.into());
    let mut declaration = morrow_core::channel::declaration(vec![HANDLER.into()], vec![Kind::Events]);
    declaration.budget = Some(Budget { max_channels:2, max_frame_bytes:32768, max_bytes:1048576,
        max_messages:128, max_requests:4096, max_duration_ms:10000 }.to_proto());
    manifest.channel_declaration = Some(declaration);
    Package::build(manifest, &wasm).unwrap()
}
fn approval(origin: &str, operation: &str) -> Approval {
    Approval { request: RawHttpRequest { method:"POST".into(), target:format!("{origin}/synthetic-events"),
        headers:vec![("Content-Type".into(), b"application/json".to_vec())], body:BODY.to_vec() },
        operation_id:operation.into(), approval_epoch:[0x51;32], deadline:Instant::now()+Duration::from_secs(3),
        profile:NetworkProfile::LoopbackHttp,
        limits:Limits { max_request_bytes:4096, max_response_bytes:32768, max_header_bytes:4096,
            max_concurrent:1, timeout:Duration::from_secs(3) },
        decoder_limits:DecoderLimits { max_line_bytes:1024, max_event_bytes:4096, max_total_bytes:32768,
            max_events:16, max_id_bytes:1024, max_retry_digits:10 }, max_encoded_bytes:32768 }
}
fn envelope(data: &str, event: &str, id: &str, retry: Option<u64>) -> EventEnvelope {
    EventEnvelope { data:data.into(), event:event.into(), id:id.into(), retry }
}
fn pinned_new_guests() -> Vec<(&'static str, Package)> {
    [("Rust", "MORROW_RUST_CHANNEL_WASM", "MORROW_RUST_CHANNEL_PACKAGE"),
        ("C", "MORROW_SDK_CHANNEL_GUEST_C", "MORROW_SDK_CHANNEL_PACKAGE_C"),
        ("C++", "MORROW_SDK_CHANNEL_GUEST_CPP", "MORROW_SDK_CHANNEL_PACKAGE_CPP")]
    .into_iter().map(|(language, wasm_key, package_key)| {
        let wasm_path = PathBuf::from(std::env::var_os(wasm_key).expect("ROOT-sealed NEW language wasm mandatory; missing is FAIL"));
        let package_path = PathBuf::from(std::env::var_os(package_key).expect("ROOT-sealed NEW original archive mandatory; missing is FAIL"));
        let wasm = std::fs::read(&wasm_path).unwrap(); let archive = std::fs::read(&package_path).unwrap();
        for (key,bytes) in [(wasm_key,wasm.as_slice()),(package_key,archive.as_slice())] {
            let pin_key=format!("{key}_SHA256");
            let pin=std::env::var(&pin_key).expect("ROOT-sealed full input SHA256 pin mandatory; missing is FAIL");
            assert!(pin.len()==64 && pin.bytes().all(|b|b.is_ascii_hexdigit()),"{pin_key}: complete SHA256 required");
            assert_eq!(format!("{:x}",Sha256::digest(bytes)),pin.to_ascii_lowercase(),"{pin_key}: entire input differs from ROOT pin");
        }
        let package = Package::decode(&archive).unwrap();
        assert_eq!(package.module(), wasm); assert_eq!(package.archive(), archive);
        assert!(package.manifest().package_id.starts_with("org.example.w14.channel."), "NEW artifact identity");
        assert!(package.capabilities().is_empty()); assert!(package.io_declaration().is_none());
        assert_eq!(package.manifest().transform_handlers[0].handler, HANDLER);
        eprintln!("W14 NEW {language}: wasm={:?} sha256={:x} package={:?} sha256={:x}; not SDK014/frozen57",
            wasm_path,Sha256::digest(&wasm),package_path,Sha256::digest(&archive));
        (language, package)
    }).collect()
}

#[test]
fn new_three_language_guests_consume_complete_envelopes_and_ack_before_http_eof() {
    for (language, package) in pinned_new_guests() {
        let id = "i".repeat(300);
        let first = format!("id: {id}\r\nevent: delta\r\nretry: 7\r\ndata: ");
        let server = Server::new(HEAD, vec![ [first.as_bytes(), b"\xe4"].concat(), b"\xbd\xa0\r".to_vec(),
            b"\ndata: second\r\n\r\ndata:\n\ndata: [DONE]\n\n".to_vec() ], true);
        let mut f = Fixture::new(&package, 4000);
        let source = f.source(approval(&server.origin, "w14-three-language")); f.start(&source);
        let invocation = f.invocation();
        let expected:Vec<Vec<u8>> = [envelope("你\nsecond","delta",&id,Some(7)), envelope("","message",&id,Some(7)),
            envelope("[DONE]","message",&id,Some(7))].iter().map(|e|e.encode().unwrap()).collect();
        let mut digest = Sha256::new(); for event in &expected { digest.update(event); }
        let bytes:usize = expected.iter().map(Vec::len).sum();
        let observed = server.observed.clone();
        let broker = &f.broker;
        let result = thread::scope(|scope| {
            let first_ack = scope.spawn(move || {
                until(|| broker.snapshot().last_acked >= 1);
                assert!(!observed.eof_written.load(Ordering::SeqCst), "guest consumed+ACKed first event before EOF permitted");
                observed.release();
            });
            let result = f.broker.run_invocation(&f.manager, &mut f.host, &f.instance, &invocation, || 2);
            first_ack.join().unwrap(); result
        });
        assert_eq!(result.execution.outcome, Ok(0), "{language}: real compiled guest");
        assert!(result.failure.is_none()); let output = result.output.unwrap();
        assert_eq!(output.type_id,"bytes"); assert_eq!(output.bytes.len(),64); assert_eq!(&output.bytes[..4],b"CHV1");
        assert_eq!(u32::from_le_bytes(output.bytes[4..8].try_into().unwrap()),2,"original SDK event-subscription mode");
        // sdk/rust/src/channel.rs Status::Closed and MP_CHANNEL_CLOSED are both 5.
        assert_eq!(u32::from_le_bytes(output.bytes[8..12].try_into().unwrap()),5,"original SDK Closed status");
        assert!(u32::from_le_bytes(output.bytes[12..16].try_into().unwrap())<=1,"wire boolean only; not an actual join proof");
        assert_eq!(u64::from_le_bytes(output.bytes[16..24].try_into().unwrap()),3);
        assert_eq!(u64::from_le_bytes(output.bytes[24..32].try_into().unwrap()),bytes as u64);
        assert_eq!(&output.bytes[32..],digest.finalize().as_slice(), "owned full envelope digest");
        let done = f.joined(&source); assert!(done.outcome.is_ok()); assert_eq!(done.events_queued,3); assert_eq!(done.events_acked,3);
        assert_eq!(done.encoded_bytes,bytes as u64); let sse=done.sse.as_ref().unwrap();
        assert_eq!(sse.delivered_events,3); assert!(sse.transport.http_eof && !sse.truncated);
        let epoch=f.broker.endpoint().source_epoch;
        let checkpoint=f.host.store_local().channel_checkpoint(&SCOPE,&epoch).unwrap().unwrap();
        assert_eq!(checkpoint.sequence,3); assert_eq!(checkpoint.cursor.len(),32);
        for seq in 1..=3 { let receipt=f.host.store_local().channel_ack_receipt(&SCOPE,&epoch,seq).unwrap().unwrap();
            let frame=Frame::decode(&receipt.frame_wire).unwrap(); assert_eq!(frame.bytes,expected[seq as usize-1]);
            let decoded=EventEnvelope::decode(&frame.bytes).unwrap(); assert_eq!(decoded.id,id); assert_eq!(decoded.retry,Some(7));
            assert_eq!(decoded.data,["你\nsecond","","[DONE]"][seq as usize-1]);
            assert_eq!(receipt.checkpoint.frame_sha256,frame.digest().unwrap()); assert_eq!(frame.cursor.len(),32); }
        f.assert_unknown(&source); assert_eq!(server.finish().posts(),1);
    }
}

#[test]
fn bad_ack_and_unreceived_ack_never_release_upstream_event_credit() {
    let server=Server::new(HEAD,vec![b"id: stable\ndata: first\n\ndata: second\n\n".to_vec()],true);
    let mut f=Fixture::wat(); let source=f.source(approval(&server.origin,"w14-ack-credit")); f.start(&source);
    until(|| server.observed.posts()==1);
    assert_eq!(f.call(Action::Ack { sequence:1,frame_sha256:[1;32],cursor:vec![2;32] },2).status,Status::Invalid);
    let frame=f.frame(); let mut wrong=frame.digest().unwrap(); wrong[0]^=1;
    assert_eq!(f.call(Action::Ack { sequence:frame.sequence,frame_sha256:wrong,cursor:frame.cursor.clone() },3).status,Status::Invalid);
    assert_eq!(f.call(Action::Ack { sequence:frame.sequence,frame_sha256:frame.digest().unwrap(),cursor:vec![0;32] },4).status,Status::Invalid);
    f.no_ack(); thread::sleep(Duration::from_millis(40)); source.revoke(); f.original_control_active();
    let done=f.joined(&source); assert!(done.outcome.is_err()); assert_eq!(done.events_queued,1); assert_eq!(done.events_acked,0);
    assert_eq!(done.sse.as_ref().unwrap().delivered_events,1,"no second event delivery while original ACK is absent");
    f.no_ack(); f.assert_unknown(&source); assert_eq!(server.finish().posts(),1);
}

#[test]
fn source_only_revoke_after_receive_suppresses_ack_and_preserves_active_control() {
    for received in [false,true] {
        let server=Server::new(HEAD,vec![b"data: queued\n\n".to_vec()],true);
        let mut f=Fixture::wat(); let source=f.source(approval(&server.origin,"w14-revoke-received")); f.start(&source);
        // Snapshot takes the original queue lock: the one charged message is now queued.
        until(|| f.broker.snapshot().usage.messages==1);
        let frame=received.then(|| f.frame()); source.revoke(); f.original_control_active();
        if let Some(frame)=frame { assert_ne!(f.ack(&frame,30).status,Status::Acked); }
        assert_ne!(f.call(Action::Receive { last_acked:0,credit_bytes:32768 },31).status,Status::Frame);
        f.no_ack(); let done=f.joined(&source); assert!(done.outcome.is_err()); f.assert_unknown(&source);
        assert_eq!(done.events_queued,1); assert_eq!(server.finish().posts(),1);
    }
}

#[test]
fn committed_ack_survives_later_source_revoke_without_delivering_next_event() {
    let server=Server::new(HEAD,vec![b"data: committed\n\ndata: undelivered\n\n".to_vec()],true);
    let mut f=Fixture::wat(); let source=f.source(approval(&server.origin,"w14-postcommit-revoke")); f.start(&source);
    let frame=f.frame(); assert_eq!(f.ack(&frame,30).status,Status::Acked);
    let epoch=f.broker.endpoint().source_epoch;
    let before=f.host.store_local().channel_ack_receipt(&SCOPE,&epoch,1).unwrap().unwrap();
    source.revoke(); f.original_control_active();
    assert_ne!(f.call(Action::Receive { last_acked:1,credit_bytes:32768 },31).status,Status::Frame);
    let done=f.joined(&source); assert!(done.outcome.is_err());
    let after=f.host.store_local().channel_ack_receipt(&SCOPE,&epoch,1).unwrap().unwrap();
    assert_eq!(after,before);
    assert!(f.host.store_local().channel_ack_receipt(&SCOPE,&epoch,2).unwrap().is_none());
    assert_eq!(f.broker.snapshot().last_acked,1); f.assert_unknown(&source); assert_eq!(server.finish().posts(),1);
}

#[test]
fn denied_preclaim_and_repeated_start_have_no_extra_post_or_restored_authority() {
    let server=Server::new(HEAD,vec![b"data: never\n\n".to_vec()],true);
    let mut denied=Fixture::wat(); let source=denied.source(approval(&server.origin,"w14-denied-before-claim")); source.revoke();
    assert!(source.start(&denied.manager,&mut denied.host,&denied.instance,&denied.broker).is_err());
    assert!(denied.host.store_local().lookup_matching_io_intent(source.command()).unwrap().is_none());
    denied.original_control_active(); assert_eq!(server.finish().posts(),0);
    let server=Server::new(HEAD,vec![b"data: once\n\n".to_vec()],true);
    let mut f=Fixture::wat(); let source=f.source(approval(&server.origin,"w14-start-once")); f.start(&source);
    let _frame=f.frame(); assert!(source.start(&f.manager,&mut f.host,&f.instance,&f.broker).is_err());
    source.revoke(); let done=f.joined(&source); assert!(done.outcome.is_err()); f.assert_unknown(&source);
    assert!(source.start(&f.manager,&mut f.host,&f.instance,&f.broker).is_err());
    assert_eq!(server.finish().posts(),1,"reopened Unknown never grants another POST");
}

#[test]
fn independent_adapters_compete_for_one_durable_original_claim() {
    let server=Server::new(HEAD,vec![b"data: winner\n\n".to_vec()],true);
    let mut f=Fixture::wat(); let endpoint=f.broker.endpoint();
    let second=f.manager.bind_channel(&f.host,&f.instance,f.instance.package().package().digest(),f.manager.revision(),
        Source { kind:Kind::Events,duplex:false,checkpoint_scope:Some([0x77;32]) },endpoint.budget,1+endpoint.budget.max_duration_ms,1).unwrap();
    let original=approval(&server.origin,"w14-claim-competition");
    let second_approval=original.clone();
    let first=f.source(original);
    let second_source=SseSource::approve(&f.manager,&f.host,&f.instance,&second,f.manager.revision(),
        second_approval,1).unwrap();
    assert_eq!(first.command(),second_source.command(),"same exact original command across distinct source brokers");
    f.start(&first); let _frame=f.frame();
    assert!(second_source.start(&f.manager,&mut f.host,&f.instance,&second).is_err());
    first.revoke(); let done=f.joined(&first); assert!(done.outcome.is_err()); f.assert_unknown(&first);
    let unknown=Store::open(&f.dir.path().join("db"),Default::default()).unwrap()
        .lookup_matching_io_intent(first.command()).unwrap().unwrap(); assert_eq!(unknown.phase(),Phase::OutcomeUnknown);
    second.cleanup(); assert_eq!(second.snapshot().cleanup_proof,CleanupProof::NoProducer);
    assert_eq!(server.finish().posts(),1);
}

#[test]
fn original_cancel_close_disable_expiry_and_wat_trap_all_join_real_workers() {
    for mode in ["cancel","close","disable","expiry","trap"] {
        let server=Server::new(HEAD,vec![b"data: original-control\n\n".to_vec()],true);
        let mut f=Fixture::new(&wat_package(mode=="trap"),4000);
        let mut a=approval(&server.origin,"w14-original-control"); if mode=="expiry" { a.deadline=Instant::now()+Duration::from_millis(400); }
        let original_deadline=a.deadline;
        let source=f.source(a); f.start(&source); let _frame=f.frame();
        match mode {
            "cancel"=>f.instance.request_stop(),
            "close"=>{let _=f.call(Action::Close,40);},
            "disable"=>{let p=f.instance.package().package(); f.manager.set_enabled(&p.manifest().package_id,p.digest(),false,f.manager.revision()).unwrap();},
            "expiry"=>{thread::sleep(original_deadline.checked_duration_since(Instant::now()).unwrap_or_default()+Duration::from_millis(20));
                assert!(Instant::now()>=original_deadline,"only original absolute expiry; no renewed deadline");},
            "trap"=>{let invocation=f.invocation(); let result=f.broker.run_invocation(&f.manager,&mut f.host,&f.instance,&invocation,||2);
                assert!(result.execution.outcome.is_err(),"new WAT intentional trap");},
            _=>unreachable!(),
        }
        let done=f.joined(&source);
        let expected=match mode {
            "expiry"=>morrow_plugin_runtime::channel::Error::Expired,
            "close"=>morrow_plugin_runtime::channel::Error::Closed,
            "cancel"|"disable"|"trap"=>morrow_plugin_runtime::channel::Error::Denied,
            _=>unreachable!(),
        };
        assert_eq!(done.outcome,Err(morrow_network_node_stream::managed_sse::Error::Source(expected)),"{mode}: exact original source first cause");
        if mode=="expiry" { assert_eq!(f.broker.snapshot().terminal_cause,Some(Status::Expired)); }
        // Actual HTTP Timeout/cancellation is its own cleanup fact, not the typed source cause.
        eprintln!("W14 {mode}: source={:?} http={:?}",done.outcome,done.transport);
        f.no_ack(); f.assert_unknown(&source); assert_eq!(server.finish().posts(),1);
    }
}

#[test]
fn mime_status_invalid_utf8_truncated_eof_and_decoder_limit_keep_first_failure_and_join() {
    for (label,head,parts,small_line) in [
        ("mime","HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/plain\r\n\r\n",vec![b"data: x\n\n".to_vec()],false),
        ("status","HTTP/1.1 503 Unavailable\r\nTransfer-Encoding: chunked\r\nContent-Type: text/event-stream\r\n\r\n",vec![b"data: x\n\n".to_vec()],false),
        ("utf8",HEAD,vec![b": invalid-comment \xff\n\n".to_vec()],false),
        ("half-utf8",HEAD,vec![b"data: \xe4".to_vec()],false),
        ("truncated",HEAD,vec![b"data: no-delimiter".to_vec()],false),
        ("limit",HEAD,vec![b": oversized-comment\n\n".to_vec()],true),
    ] {
        let server=Server::new(head,parts,false); let mut f=Fixture::wat(); let mut a=approval(&server.origin,"w14-parser-failure");
        if small_line { a.decoder_limits.max_line_bytes=8; a.decoder_limits.max_id_bytes=8; }
        let source=f.source(a); f.start(&source); let done=f.joined(&source);
        let expected=match label {
            "mime"=>morrow_network_node_stream::managed_sse::Error::Sse(morrow_network_node_stream::sse::Error::InvalidMime),
            "status"=>morrow_network_node_stream::managed_sse::Error::Sse(morrow_network_node_stream::sse::Error::HttpStatus(503)),
            "utf8"|"half-utf8"=>morrow_network_node_stream::managed_sse::Error::Sse(morrow_network_node_stream::sse::Error::Decoder(morrow_network_node_stream::sse::decoder::Error::InvalidUtf8)),
            "truncated"=>morrow_network_node_stream::managed_sse::Error::Truncated,
            "limit"=>morrow_network_node_stream::managed_sse::Error::Sse(morrow_network_node_stream::sse::Error::Decoder(morrow_network_node_stream::sse::decoder::Error::LineLimit)),
            _=>unreachable!(),
        };
        assert_eq!(done.outcome,Err(expected),"{label}: retain actual first cause instead of cleanup cancellation");
        assert_eq!(done.events_acked,0); assert_eq!(done.events_queued,0);
        f.no_ack(); f.assert_unknown(&source); assert_eq!(server.finish().posts(),1);
    }
}

#[test]
fn raw_network_and_encoded_channel_budgets_are_independent_and_not_refunded() {
    // Pure comments use network quota but produce no channel/event allocation.
    let server=Server::new(HEAD,vec![b": bounded raw bytes consumed\n\n".to_vec()],false);
    let mut f=Fixture::wat(); let source=f.source(approval(&server.origin,"w14-comment-accounting")); f.start(&source);
    let done=f.joined(&source); assert!(done.outcome.is_ok()); assert_eq!(done.events_queued,0); assert_eq!(done.encoded_bytes,0);
    assert_eq!(done.sse.as_ref().unwrap().transport.received_bytes,b": bounded raw bytes consumed\n\n".len()); assert_eq!(f.broker.snapshot().usage.bytes,0);
    f.assert_unknown(&source); assert_eq!(server.finish().posts(),1);
    // One admitted event remains charged even though its ACK is never given.
    let server=Server::new(HEAD,vec![b"data: charged\n\n".to_vec()],true);
    let mut f=Fixture::wat(); let source=f.source(approval(&server.origin,"w14-encoded-accounting")); f.start(&source);
    let frame=f.frame(); let charged=f.broker.snapshot().usage.bytes; assert_eq!(charged,frame.bytes.len() as u64);
    assert!(charged > b"data: charged\n\n".len() as u64); source.revoke(); let done=f.joined(&source);
    assert_eq!(done.encoded_bytes,charged); assert_eq!(f.broker.snapshot().usage.bytes,charged,"revoke cannot refund already admitted encoding");
    assert_eq!(done.events_acked,0); f.no_ack(); f.assert_unknown(&source); assert_eq!(server.finish().posts(),1);
}

#[test]
fn actual_ack_final_store_guard_rolls_back_and_postcommit_guard_keeps_history_but_denies_delivery() {
    for fail_at in [5usize,6] {
        let server=Server::new(HEAD,vec![b"data: guarded\n\n".to_vec()],true);
        let mut f=Fixture::wat(); let main_thread=thread::current().id();
        let armed=Arc::new(AtomicBool::new(false)); let checks=Arc::new(AtomicUsize::new(0));
        let arm=armed.clone(); let seen=checks.clone();
        let source=SseSource::approve_with_live_guard(&f.manager,&f.host,&f.instance,&f.broker,f.manager.revision(),
            approval(&server.origin,"w14-actual-store-guard"),1,move || {
                // Pure, bounded probe: other real worker checks cannot advance this sequence.
                if arm.load(Ordering::SeqCst) && thread::current().id()==main_thread {
                    seen.fetch_add(1,Ordering::SeqCst)+1 < fail_at
                } else { true }
            }).unwrap();
        f.start(&source); let frame=f.frame();
        let request=f.request(Action::Ack { sequence:frame.sequence,frame_sha256:frame.digest().unwrap(),cursor:frame.cursor.clone() },50);
        // Current production ordering: broker gate(1), dispatch's additional
        // resource gate(2), locked queue gate(3), original Store pre-write
        // guard(4), post-INSERT final guard(5), public result-delivery gate(6).
        // No Store/queue callback is mocked.
        // Changes to this sequence require static re-review, never weakening a failure.
        armed.store(true,Ordering::SeqCst);
        let response=f.broker.dispatch(&f.manager,&mut f.host,&f.instance,&request,2);
        assert_eq!(checks.load(Ordering::SeqCst),fail_at,"exact original boundary selected");
        f.original_control_active();
        if fail_at==5 {
            assert!(response.is_err(),"final Store guard failure must not return an ACK"); f.no_ack();
        } else {
            assert_ne!(response.unwrap().status,Status::Acked,"committed ACK cannot be delivered after source revoke");
            let epoch=f.broker.endpoint().source_epoch;
            let receipt=f.host.store_local().channel_ack_receipt(&SCOPE,&epoch,1).unwrap().unwrap();
            assert_eq!(receipt.checkpoint.frame_sha256,frame.digest().unwrap());
            assert_eq!(receipt.checkpoint.cursor,frame.cursor); assert_eq!(f.broker.snapshot().last_acked,1);
            assert_eq!(f.host.store_local().channel_checkpoint(&SCOPE,&epoch).unwrap().unwrap(),receipt.checkpoint);
        }
        let done=f.joined(&source); assert!(done.outcome.is_err()); f.assert_unknown(&source);
        assert_eq!(server.finish().posts(),1);
    }
}

#[test]
fn rejected_credentials_profiles_and_foreign_instance_never_post() {
    for label in ["authorization","cookie","last-event-id","public-http","changed-method","zero-epoch"] {
        let server=Server::new(HEAD,vec![b"data: denied\n\n".to_vec()],false); let f=Fixture::wat();
        let mut a=approval(&server.origin,"w14-declared-denials");
        match label {
            "authorization"=>a.request.headers.push(("Authorization".into(),b"synthetic-no-real-key".to_vec())),
            "cookie"=>a.request.headers.push(("Cookie".into(),b"synthetic=no-real-cookie".to_vec())),
            "last-event-id"=>a.request.headers.push(("Last-Event-ID".into(),b"old-id-is-not-a-grant".to_vec())),
            "public-http"=>a.profile=NetworkProfile::PublicHttps,
            "changed-method"=>a.request.method="GET".into(),
            "zero-epoch"=>a.approval_epoch=[0;32], _=>unreachable!(),
        }
        assert!(SseSource::approve(&f.manager,&f.host,&f.instance,&f.broker,f.manager.revision(),a,1).is_err(),"{label}");
        assert_eq!(server.finish().posts(),0);
    }
    let server=Server::new(HEAD,vec![b"data: bound\n\n".to_vec()],true); let mut f=Fixture::wat();
    let source=f.source(approval(&server.origin,"w14-wrong-instance"));
    let id=f.instance.package().package().manifest().package_id.clone();
    let foreign=f.manager.connect(&id,&mut f.host).unwrap();
    assert!(source.start(&f.manager,&mut f.host,&foreign,&f.broker).is_err(),"same package different actual instance denied");
    assert!(f.host.store_local().lookup_matching_io_intent(source.command()).unwrap().is_none());
    assert_eq!(server.finish().posts(),0); foreign.close(&mut f.host).unwrap();
}

#[test]
fn encoded_limit_and_raw_limit_fail_independently_with_real_cleanup() {
    let server=Server::new(HEAD,vec![b"data: envelope\n\n".to_vec()],false); let mut f=Fixture::wat();
    let mut a=approval(&server.origin,"w14-encoded-limit"); a.max_encoded_bytes=8;
    let source=f.source(a); f.start(&source); let done=f.joined(&source);
    assert_eq!(done.outcome,Err(morrow_network_node_stream::managed_sse::Error::Limit)); assert_eq!(done.events_queued,0); assert_eq!(done.events_acked,0);
    assert_eq!(done.sse.as_ref().unwrap().delivered_events,1,"decoder succeeded; encoded budget separately rejected");
    assert_eq!(f.broker.snapshot().usage.bytes,0); f.no_ack(); f.assert_unknown(&source); assert_eq!(server.finish().posts(),1);
    let server=Server::new(HEAD,vec![b": 123456789012345678901234567890\n\n".to_vec()],false); let mut f=Fixture::wat();
    let mut a=approval(&server.origin,"w14-raw-limit"); a.limits.max_response_bytes=32;
    a.decoder_limits.max_total_bytes=32; a.decoder_limits.max_event_bytes=32;
    a.decoder_limits.max_line_bytes=32; a.decoder_limits.max_id_bytes=32;
    let source=f.source(a); f.start(&source); let done=f.joined(&source);
    assert!(done.outcome.is_err()); assert_eq!(done.events_queued,0); assert_eq!(done.encoded_bytes,0);
    f.no_ack(); f.assert_unknown(&source); assert_eq!(server.finish().posts(),1);
}

#[test]
fn preheader_failure_retains_actual_http_join_and_validation_failure_proves_no_worker() {
    let server=Server::new("not-http\r\n\r\n",vec![b"synthetic malformed response".to_vec()],false);
    let mut f=Fixture::wat(); let source=f.source(approval(&server.origin,"w14-preheader-failure")); f.start(&source);
    let done=f.joined(&source); assert!(done.outcome.is_err()); assert!(done.sse.is_none());
    assert!(matches!(&done.opening_cleanup,Some(morrow_network_node_stream::stream::OpeningCleanup::Worker(receipt)) if receipt.worker_joined));
    f.no_ack(); f.assert_unknown(&source); assert_eq!(server.finish().posts(),1);
    let server=Server::new(HEAD,vec![b"data: should-not-send\n\n".to_vec()],false);
    let client=morrow_network_node_stream::client::Client::new(
        morrow_network_node_stream::client::EndpointPolicy::new(&server.origin,&["POST"],true).unwrap(),Limits::default()).unwrap();
    let mut invalid=approval(&server.origin,"unused-no-worker").request; invalid.method="GET".into();
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let context=morrow_network_node_stream::stream::SendContext::trusted(tokio::time::Instant::now()+Duration::from_secs(1),
        tokio_util::sync::CancellationToken::new());
    let result=runtime.block_on(client.send_stream_with_receipt(invalid,context));
    let error=match result { Err(error)=>error,Ok(_)=>panic!("invalid request created a worker") };
    assert!(matches!(error.cleanup,morrow_network_node_stream::stream::OpeningCleanup::NoWorker));
    assert_eq!(server.finish().posts(),0);
}

#[test]
fn stale_revision_bad_command_and_foreign_broker_cannot_replace_original_source_grant() {
    let server=Server::new(HEAD,vec![b"data: bound once\n\n".to_vec()],true); let mut f=Fixture::wat();
    let original=approval(&server.origin,"w14-immutable-grant");
    assert!(SseSource::approve(&f.manager,&f.host,&f.instance,&f.broker,f.manager.revision()+1,original.clone(),0).is_err());
    let source=f.source(original); // Failed stale/older factory did not poison the valid original clock.
    let expected=source.command().clone(); let endpoint=f.broker.endpoint();
    let other=f.manager.bind_channel(&f.host,&f.instance,f.instance.package().package().digest(),f.manager.revision(),
        Source { kind:Kind::Events,duplex:false,checkpoint_scope:Some([0x78;32]) },endpoint.budget,1+endpoint.budget.max_duration_ms,1).unwrap();
    assert!(source.grant().validate_owner(&f.manager,&f.host,&f.instance,&other).is_err());
    for field in ["package","subject","protocol"] {
        let mut changed=expected.clone();
        match field { "package"=>changed.package_sha256[0]^=1,"subject"=>changed.subject="org.example.foreign.subject".into(),
            "protocol"=>changed.protocol_sha256[0]^=1,_=>unreachable!() }
        assert!(morrow_plugin_runtime::channel::SourceGrant::issue(&f.manager,&f.host,&f.instance,&other,
            f.manager.revision(),changed,0).is_err(),"{field}: exact binding rejected before clock mutation");
    }
    assert!(morrow_plugin_runtime::channel::SourceGrant::issue(&f.manager,&f.host,&f.instance,&f.broker,
        f.manager.revision(),expected.clone(),0).is_err(),"cannot reattach or change the existing source");
    assert_eq!(source.command(),&expected); f.original_control_active(); f.start(&source); let _frame=f.frame();
    source.revoke(); let done=f.joined(&source); assert!(done.outcome.is_err()); f.assert_unknown(&source);
    other.cleanup(); assert_eq!(other.snapshot().cleanup_proof,CleanupProof::NoProducer);
    assert_eq!(server.finish().posts(),1);
}
