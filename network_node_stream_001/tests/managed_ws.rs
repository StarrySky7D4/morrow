//! W15 actual managed duplex channel + mature loopback WebSocket integration.
//! NEW three-language fixtures and intentionally constructed WAT are distinct inputs.
#![cfg(all(feature="managed-websocket",not(target_arch="wasm32")))]
#[path="support/managed_ws_support.rs"] mod support;
use support::{Peer,Scenario,TEXT,BINARY,WAIT,until};
use morrow_core::{
    channel::{Action,Budget,Frame,Kind,Request,Response,Status},
    dispatch::HostRuntime,io_intent::Phase,
    plugin_package::{Package,catalog::Catalog,registry::Registry,proto::TransformHandler},
    store::Store,task::{Invocation,Transform},
};
use morrow_plugin_runtime::{
    Limits as RuntimeLimits,
    channel::{ChannelBroker,CleanupProof,Source},manager::{Manager,ManagedInstance},
};
use morrow_network_node_stream::{
    Limits,RawHttpRequest,websocket::{MessageKind,Quotas},
    managed_ws::{Approval,Completion,Error,MessageEnvelope,NetworkProfile,WsSource},
};
use sha2::{Digest,Sha256};
use std::{path::PathBuf,sync::{Arc,atomic::{AtomicBool,AtomicUsize,Ordering}},thread,time::{Duration,Instant}};
const SCOPE:[u8;32]=[0x79;32];
const HANDLER:&str="channel.ws.duplex";
struct Fixture{manager:Manager,host:HostRuntime,instance:ManagedInstance,broker:ChannelBroker,dir:tempfile::TempDir}
impl Fixture{
    fn new(package:&Package,duration_ms:u64)->Self{
        let dir=tempfile::tempdir().unwrap();let declared=package.channel_declaration().unwrap();
        let mut budget=Budget::from_proto(declared.budget.as_ref().unwrap()).unwrap();
        budget.max_duration_ms=duration_ms.min(budget.max_duration_ms);
        let execution=package.manifest().budget.as_ref().unwrap();
        let catalog=Catalog::open(&dir.path().join("catalog")).unwrap();catalog.install(package).unwrap();
        let registry=Registry::open(&dir.path().join("registry"),catalog).unwrap();
        let mut manager=Manager::new(registry,RuntimeLimits{host_calls:execution.host_calls,fuel:execution.fuel,
            memory_bytes:usize::try_from(execution.memory_bytes).unwrap()});
        manager.select(package,manager.revision()).unwrap();
        manager.set_enabled(&package.manifest().package_id,package.digest(),true,manager.revision()).unwrap();
        let mut host=HostRuntime::new(Store::open(&dir.path().join("db"),Default::default()).unwrap()).unwrap();
        let instance=manager.connect(&package.manifest().package_id,&mut host).unwrap();
        let broker=manager.bind_channel(&host,&instance,package.digest(),manager.revision(),Source{
            kind:Kind::Events,duplex:true,checkpoint_scope:Some(SCOPE)},budget,1+budget.max_duration_ms,1).unwrap();
        Self{manager,host,instance,broker,dir}
    }
    fn wat()->Self{Self::new(&wat_package(false),4000)}
    fn request(&self,action:Action,serial:u8)->Request{
        let endpoint=self.broker.endpoint();Request{call_id:[serial;32],reference:endpoint.reference,source_epoch:endpoint.source_epoch,action}
    }
    fn call(&mut self,action:Action,serial:u8)->Response{
        let request=self.request(action,serial);
        self.broker.dispatch(&self.manager,&mut self.host,&self.instance,&request,2).unwrap()
    }
    fn frame(&mut self)->Frame{
        let end=Instant::now()+WAIT;let mut serial=10;
        loop{
            let reply=self.call(Action::Receive{last_acked:self.broker.snapshot().last_acked,
                credit_bytes:self.broker.endpoint().budget.max_frame_bytes},serial);
            if reply.status==Status::Frame{return reply.frame.unwrap()}
            assert_eq!(reply.status,Status::Idle,"no invented terminal");assert!(Instant::now()<end);
            serial=serial.wrapping_add(1).max(10);thread::sleep(Duration::from_millis(1));
        }
    }
    fn ack(&mut self,frame:&Frame,serial:u8)->Response{
        self.call(Action::Ack{sequence:frame.sequence,frame_sha256:frame.digest().unwrap(),cursor:frame.cursor.clone()},serial)
    }
    fn send(&mut self,seq:u64,kind:MessageKind,payload:&[u8],serial:u8)->Response{
        self.call(Action::Send{sequence:seq,bytes:message(kind,payload,None).encode().unwrap()},serial)
    }
    fn no_ack(&self){
        let epoch=self.broker.endpoint().source_epoch;assert_eq!(self.broker.snapshot().last_acked,0);
        assert!(self.host.store_local().channel_checkpoint(&SCOPE,&epoch).unwrap().is_none());
        assert!(self.host.store_local().channel_ack_receipt(&SCOPE,&epoch,1).unwrap().is_none());
    }
    fn active(&self){assert!(!self.host.revocation(self.instance.connection()).unwrap().is_revoked());}
    fn source(&self,a:Approval)->WsSource{
        WsSource::approve(&self.manager,&self.host,&self.instance,&self.broker,self.manager.revision(),a,1).unwrap()
    }
    fn start(&mut self,s:&WsSource){s.start(&self.manager,&mut self.host,&self.instance,&self.broker).unwrap();}
    fn joined(&self,s:&WsSource)->Arc<Completion>{
        let done=s.wait_completion(WAIT).expect("actual bounded source completion");
        until(||{self.broker.reap();self.broker.snapshot().resource_reclaimed});
        assert_eq!(self.broker.snapshot().cleanup_proof,CleanupProof::Joined);
        let ws=done.websocket.as_ref().or_else(||match done.opening_cleanup.as_ref(){
            Some(morrow_network_node_stream::websocket::OpeningCleanup::Worker(c))=>Some(c),_=>None})
            .expect("started socket retains original cleanup receipt");
        assert!(ws.worker_joined,"socket task receipt separate from native broker OS thread");
        self.host.store_local().integrity_check().unwrap();done
    }
    fn unknown(&self,s:&WsSource){
        assert_eq!(self.host.store_local().lookup_matching_io_intent(s.command()).unwrap().unwrap().phase(),Phase::OutcomeUnknown);
        let reopened=Store::open(&self.dir.path().join("db"),Default::default()).unwrap();
        assert_eq!(reopened.lookup_matching_io_intent(s.command()).unwrap().unwrap().phase(),Phase::OutcomeUnknown);
        reopened.integrity_check().unwrap();
    }
    fn invocation(&self)->Invocation{
        let endpoint=self.broker.endpoint();let mut input=vec![3];input.extend(endpoint.reference);input.extend(endpoint.source_epoch);
        Invocation::new_transform("w15-public-ws",Transform{handler:HANDLER.into(),input_type:"bytes".into(),output_type:"bytes".into(),input}).unwrap()
    }
}
fn wat_package(trap:bool)->Package{
    let body=if trap{"unreachable"}else{"i32.const 0"};
    let wasm=wat::parse_str(format!(r#"(module
        (import "morrow_channel_v1" "call" (func(param i32 i32 i32 i32)(result i32)))
        (memory(export "memory")4)(func(export "morrow_run")(result i32){body}))"#)).unwrap();
    let mut manifest=Package::manifest_for_transform("org.example.w15.ws.wat","0.1.0",&wasm,
        vec![TransformHandler{handler:HANDLER.into(),input_type:"bytes".into(),output_type:"bytes".into(),max_input_bytes:65,max_output_bytes:64}]);
    manifest.required_features.push(morrow_core::channel::FEATURE.into());
    let mut declaration=morrow_core::channel::declaration(vec![HANDLER.into()],vec![Kind::Events]);
    declaration.budget=Some(Budget{max_channels:2,max_frame_bytes:32768,max_bytes:1048576,
        max_messages:128,max_requests:4096,max_duration_ms:10000}.to_proto());
    manifest.channel_declaration=Some(declaration);Package::build(manifest,&wasm).unwrap()
}
fn approval(origin:&str,op:&str)->Approval{
    let mut quotas=Quotas::default();quotas.max_message_bytes=1024;quotas.max_frame_bytes=1024;
    quotas.max_pending_bytes=4096;quotas.max_pending_messages=2;
    quotas.max_incoming_wire_bytes=65536;quotas.max_outgoing_wire_bytes=65536;
    Approval{request:RawHttpRequest{method:"GET".into(),target:format!("{origin}/synthetic-ws"),headers:vec![],body:vec![]},
        operation_id:op.into(),approval_epoch:[0x61;32],deadline:Instant::now()+Duration::from_secs(3),
        profile:NetworkProfile::LoopbackWs,limits:Limits{max_request_bytes:65536,max_response_bytes:65536,max_header_bytes:4096,
            max_concurrent:1,timeout:Duration::from_secs(3)},subprotocols:vec![],quotas,max_encoded_bytes:32768}
}
fn message(kind:MessageKind,payload:&[u8],close_code:Option<u16>)->MessageEnvelope{
    MessageEnvelope{kind,payload:payload.to_vec(),close_code}
}
fn pinned_new_guests()->Vec<(&'static str,Package)>{
    [("Rust","RUST","rust"),("C","C","c"),("C++","CPP","cpp")].into_iter().map(|(language,key,id)|{
        let wasm_key=format!("MORROW_W15_WS_{key}_WASM");let package_key=format!("MORROW_W15_WS_{key}_PACKAGE");
        let wasm_path=PathBuf::from(std::env::var_os(&wasm_key).expect("NEW ROOT-sealed Wasm required; missing is FAIL"));
        let package_path=PathBuf::from(std::env::var_os(&package_key).expect("NEW ROOT-sealed package required; missing is FAIL"));
        let wasm=std::fs::read(&wasm_path).unwrap();let archive=std::fs::read(&package_path).unwrap();
        for(key,bytes)in[(&wasm_key,wasm.as_slice()),(&package_key,archive.as_slice())]{
            let pin=std::env::var(format!("{key}_SHA256")).expect("full input SHA mandatory");
            assert!(pin.len()==64&&pin.bytes().all(|b|b.is_ascii_hexdigit()));
            assert_eq!(format!("{:x}",Sha256::digest(bytes)),pin.to_ascii_lowercase());
        }
        let p=Package::decode(&archive).unwrap();assert_eq!(p.module(),wasm);assert_eq!(p.archive(),archive);
        assert_eq!(p.manifest().package_id,format!("org.example.w15.ws.{id}"));
        assert!(p.capabilities().is_empty()&&p.io_declaration().is_none());
        assert_eq!(p.manifest().transform_handlers[0].handler,HANDLER);
        eprintln!("W15 NEW {language}: full Wasm/package hashes verified, not old SDK014/frozen");
        (language,p)
    }).collect()
}
#[test]
fn new_three_language_guests_interleave_real_send_receive_ack_and_peer_close(){
    for(language,package)in pinned_new_guests(){
        let peer=Peer::new(Scenario::EchoDuplex);let mut f=Fixture::new(&package,4000);
        let source=f.source(approval(&peer.ws_origin,"w15-three-language"));f.start(&source);
        let invocation=f.invocation();let result=f.broker.run_invocation(&f.manager,&mut f.host,&f.instance,&invocation,||2);
        assert_eq!(result.execution.outcome,Ok(0),"{language}: real compiled import path");
        assert!(result.failure.is_none());let output=result.output.unwrap();
        let expected=[message(MessageKind::Text,TEXT.as_bytes(),None),message(MessageKind::Binary,BINARY,None),
            message(MessageKind::Close,&[],Some(1000))].map(|m|m.encode().unwrap());
        let mut sha=Sha256::new();for bytes in &expected{sha.update(bytes);}
        let total:usize=expected.iter().map(Vec::len).sum();
        assert_eq!(output.type_id,"bytes");assert_eq!(output.bytes.len(),64);assert_eq!(&output.bytes[..4],b"WSV1");
        assert_eq!(u32::from_le_bytes(output.bytes[4..8].try_into().unwrap()),3);
        assert_eq!(u32::from_le_bytes(output.bytes[8..12].try_into().unwrap()),5);
        assert!(u32::from_le_bytes(output.bytes[12..16].try_into().unwrap())<=1,"wire boolean is not a join proof");
        assert_eq!(u64::from_le_bytes(output.bytes[16..24].try_into().unwrap()),3);
        assert_eq!(u64::from_le_bytes(output.bytes[24..32].try_into().unwrap()),total as u64);
        assert_eq!(&output.bytes[32..],sha.finalize().as_slice());
        let done=f.joined(&source);assert!(done.outcome.is_ok());assert_eq!((done.messages_queued,done.messages_acked,done.outgoing_written),(3,3,3));
        let ws=done.websocket.as_ref().unwrap();assert!(ws.peer_close.is_some());
        let epoch=f.broker.endpoint().source_epoch;
        for(sequence,bytes)in expected.iter().enumerate(){
            let receipt=f.host.store_local().channel_ack_receipt(&SCOPE,&epoch,sequence as u64+1).unwrap().unwrap();
            let frame=Frame::decode(&receipt.frame_wire).unwrap();assert_eq!(frame.bytes,*bytes);
            assert_eq!(receipt.checkpoint.frame_sha256,frame.digest().unwrap());assert_eq!(receipt.checkpoint.cursor,frame.cursor);
            assert_eq!(frame.cursor.len(),32);
        }
        f.unknown(&source);let seen=peer.finish();let observed=seen.messages.lock().unwrap();
        assert_eq!(observed.as_slice(),&[(1,TEXT.as_bytes().to_vec()),(2,BINARY.to_vec())]);
        assert!(seen.closed.load(Ordering::SeqCst));
        // Accepted is only local admission: peer-before-ACK causality is tested separately.
    }
}
#[test]
fn uncommitted_ack_does_not_block_outgoing_socket_write_or_already_parsed_ping_pong(){
    let peer=Peer::new(Scenario::AckHeld);let mut f=Fixture::wat();
    let source=f.source(approval(&peer.ws_origin,"w15-ack-held"));f.start(&source);let frame=f.frame();
    let decoded=MessageEnvelope::decode(&frame.bytes).unwrap();assert!(matches!(decoded.kind,MessageKind::Text));assert_eq!(decoded.payload,TEXT.as_bytes());
    assert_eq!(f.send(1,MessageKind::Binary,BINARY,40).status,Status::Accepted);
    until(||peer.seen.count()==1&&peer.seen.pongs.load(Ordering::SeqCst)==1);
    f.no_ack();assert_eq!(f.broker.snapshot().usage.messages,2,"one inbound frame plus one admitted outgoing");
    let observed=peer.seen.messages.lock().unwrap();assert_eq!(observed[0],(2,BINARY.to_vec()));drop(observed);
    // Zero is rejected by the original wire contract before broker dispatch.
    let zero=f.request(Action::Ack{sequence:frame.sequence,frame_sha256:[0;32],cursor:frame.cursor.clone()},41);
    assert!(matches!(f.broker.dispatch(&f.manager,&mut f.host,&f.instance,&zero,2),
        Err(morrow_plugin_runtime::channel::Error::Invalid)));
    // A validly shaped but incorrect digest reaches exact-ACK comparison.
    let mut wrong_digest=frame.digest().unwrap();wrong_digest[0]^=1;
    assert_eq!(f.call(Action::Ack{sequence:frame.sequence,frame_sha256:wrong_digest,cursor:frame.cursor.clone()},43).status,Status::Invalid);
    assert_eq!(f.call(Action::Ack{sequence:frame.sequence,frame_sha256:frame.digest().unwrap(),cursor:vec![0;32]},42).status,Status::Invalid);
    f.no_ack();source.revoke();let done=f.joined(&source);assert_eq!(done.messages_queued,1);assert_eq!(done.messages_acked,0);
    assert_eq!(done.outgoing_written,1);assert!(done.outcome.is_err());f.unknown(&source);peer.finish();
}
#[test]
fn source_only_revocation_clears_queued_frame_and_suppresses_consumption_ack(){
    for received in [false,true]{
        let peer=Peer::new(Scenario::InitialText);let mut f=Fixture::wat();
        let source=f.source(approval(&peer.ws_origin,"w15-source-revoke"));f.start(&source);
        let frame=if received{Some(f.frame())}else{until(||f.broker.snapshot().usage.messages==1);None};
        source.revoke();f.active();
        if let Some(frame)=frame{
            assert_ne!(f.ack(&frame,50).status,Status::Acked);
        }else{
            assert!(f.call(Action::Receive{last_acked:0,credit_bytes:32768},50).frame.is_none());
        }
        let done=f.joined(&source);assert!(done.outcome.is_err());f.no_ack();f.unknown(&source);peer.finish();
    }
}
#[test]
fn actual_ack_final_store_guard_rolls_back_and_committed_history_survives_delivery_denial(){
    for fail_at in [5usize,6]{
        let peer=Peer::new(Scenario::InitialText);let mut f=Fixture::wat();let main=thread::current().id();
        let armed=Arc::new(AtomicBool::new(false));let checks=Arc::new(AtomicUsize::new(0));
        let arm=armed.clone();let seen=checks.clone();
        let source=WsSource::approve_with_live_guard(&f.manager,&f.host,&f.instance,&f.broker,f.manager.revision(),
            approval(&peer.ws_origin,"w15-real-ack-guard"),1,move||{
                if arm.load(Ordering::SeqCst)&&thread::current().id()==main{seen.fetch_add(1,Ordering::SeqCst)+1<fail_at}else{true}
            }).unwrap();
        f.start(&source);let frame=f.frame();let req=f.request(Action::Ack{sequence:frame.sequence,frame_sha256:frame.digest().unwrap(),cursor:frame.cursor.clone()},50);
        // Recheck current source order before execution: broker1, dispatch2, queue3,
        // Core prewrite4, post-INSERT final5, public result delivery6. No Store mock.
        armed.store(true,Ordering::SeqCst);
        let result=f.broker.dispatch(&f.manager,&mut f.host,&f.instance,&req,2);
        assert_eq!(checks.load(Ordering::SeqCst),fail_at);f.active();
        if fail_at==5{assert!(result.is_err());f.no_ack();}
        else{
            assert_ne!(result.unwrap().status,Status::Acked);
            let epoch=f.broker.endpoint().source_epoch;let receipt=f.host.store_local().channel_ack_receipt(&SCOPE,&epoch,1).unwrap().unwrap();
            assert_eq!(receipt.checkpoint.frame_sha256,frame.digest().unwrap());assert_eq!(receipt.checkpoint.cursor,frame.cursor);
            assert_eq!(f.broker.snapshot().last_acked,1);
            assert_eq!(f.host.store_local().channel_checkpoint(&SCOPE,&epoch).unwrap().unwrap(),receipt.checkpoint);
        }
        assert!(f.joined(&source).outcome.is_err());f.unknown(&source);peer.finish();
    }
}
#[test]
fn original_cancel_close_disable_expiry_and_wat_trap_keep_typed_cause_and_double_join(){
    for mode in ["cancel","close","disable","expiry","trap"]{
        let peer=Peer::new(Scenario::InitialText);let mut f=Fixture::new(&wat_package(mode=="trap"),4000);
        let mut a=approval(&peer.ws_origin,"w15-original-control");if mode=="expiry"{a.deadline=Instant::now()+Duration::from_millis(600);}
        let deadline=a.deadline;let source=f.source(a);f.start(&source);f.frame();
        match mode{
            "cancel"=>f.instance.request_stop(),
            "close"=>{f.call(Action::Close,40);},
            "disable"=>{let p=f.instance.package().package();f.manager.set_enabled(&p.manifest().package_id,p.digest(),false,f.manager.revision()).unwrap();},
            "expiry"=>thread::sleep(deadline.checked_duration_since(Instant::now()).unwrap_or_default()+Duration::from_millis(20)),
            "trap"=>{let input=f.invocation();assert!(f.broker.run_invocation(&f.manager,&mut f.host,&f.instance,&input,||2).execution.outcome.is_err());},
            _=>unreachable!(),
        }
        let expected=match mode{"expiry"=>morrow_plugin_runtime::channel::Error::Expired,
            "close"=>morrow_plugin_runtime::channel::Error::Closed,_=>morrow_plugin_runtime::channel::Error::Denied};
        let done=f.joined(&source);assert_eq!(done.outcome,Err(Error::Source(expected)),"typed source cause separate from socket cleanup");
        f.no_ack();f.unknown(&source);assert_eq!(peer.finish().handshakes.load(Ordering::SeqCst),1);
    }
}
#[test]
fn original_strict_claim_competition_and_repeated_start_never_reconnect_unknown(){
    let peer=Peer::new(Scenario::InitialText);let mut f=Fixture::wat();let a=approval(&peer.ws_origin,"w15-one-claim");
    let first=f.source(a.clone());
    let p=f.instance.package().package();let budget=f.broker.endpoint().budget;
    let second_broker=f.manager.bind_channel(&f.host,&f.instance,p.digest(),f.manager.revision(),Source{kind:Kind::Events,duplex:true,checkpoint_scope:Some([0x7a;32])},budget,1+budget.max_duration_ms,1).unwrap();
    let second=WsSource::approve(&f.manager,&f.host,&f.instance,&second_broker,f.manager.revision(),a,1).unwrap();
    f.start(&first);f.frame();assert_eq!(first.start(&f.manager,&mut f.host,&f.instance,&f.broker),Err(Error::AlreadyStarted));
    assert_eq!(second.start(&f.manager,&mut f.host,&f.instance,&second_broker),Err(Error::OutcomeUnknown));
    second_broker.cleanup();assert_eq!(second_broker.snapshot().cleanup_proof,CleanupProof::NoProducer);
    first.revoke();f.joined(&first);f.unknown(&first);assert_eq!(peer.finish().handshakes.load(Ordering::SeqCst),1);
}
#[test]
fn preclaim_denial_wrong_owner_credentials_epoch_and_profile_never_open_socket(){
    for fault in ["denied","foreign","credential","epoch","method","profile","header","protocol"]{
        let peer=Peer::new(Scenario::InitialText);let mut f=Fixture::wat();
        let mut a=approval(&peer.ws_origin,"w15-denied");
        match fault{
            "credential"=>a.request.headers.push(("Authorization".into(),b"synthetic".to_vec())),
            "epoch"=>a.approval_epoch=[0;32],"method"=>a.request.method="POST".into(),
            "profile"=>a.profile=NetworkProfile::PublicWss,
            "header"=>a.request.headers.push(("Sec-WebSocket-Key".into(),b"fake".to_vec())),
            "protocol"=>a.subprotocols=vec!["bad protocol".into()],_=>{},
        }
        if matches!(fault,"denied"|"foreign"){
            let source=f.source(a);
            if fault=="denied"{source.revoke();assert!(source.start(&f.manager,&mut f.host,&f.instance,&f.broker).is_err());}
            else{let foreign=Fixture::wat();assert!(source.start(&foreign.manager,&mut f.host,&foreign.instance,&foreign.broker).is_err());}
            assert!(f.host.store_local().lookup_matching_io_intent(source.command()).unwrap().is_none());
            f.broker.cleanup();assert_eq!(f.broker.snapshot().cleanup_proof,CleanupProof::NoProducer);
        }else{assert!(WsSource::approve(&f.manager,&f.host,&f.instance,&f.broker,f.manager.revision(),a,1).is_err());}
        assert_eq!(peer.finish().handshakes.load(Ordering::SeqCst),0);
    }
}
#[test]
fn protocol_fault_and_encoded_budget_failure_keep_business_unknown_and_actual_joins(){
    for mode in ["wire","encoded"]{
        let peer=Peer::new(if mode=="wire"{Scenario::InvalidUtf8}else{Scenario::InitialText});let mut f=Fixture::wat();
        let mut a=approval(&peer.ws_origin,"w15-bounded-failure");if mode=="encoded"{a.max_encoded_bytes=1;}
        let source=f.source(a);f.start(&source);let done=f.joined(&source);assert!(done.outcome.is_err());
        f.no_ack();f.unknown(&source);assert_eq!(peer.finish().handshakes.load(Ordering::SeqCst),1);
    }
}

#[test]
fn source_factories_pin_owner_revision_and_keep_legacy_sse_duplex_rejection(){
    use morrow_plugin_runtime::channel::SourceGrant;
    let peer=Peer::new(Scenario::InitialText);let f=Fixture::wat();let foreign=Fixture::wat();
    assert!(WsSource::approve(&f.manager,&f.host,&f.instance,&f.broker,f.manager.revision()+1,approval(&peer.ws_origin,"w15-stale"),1).is_err());
    let source=f.source(approval(&peer.ws_origin,"w15-pinned"));
    assert!(SourceGrant::issue_websocket_with_deadline_and_live_guard(&foreign.manager,&f.host,&f.instance,&f.broker,
        f.manager.revision(),source.command().clone(),1,Instant::now()+Duration::from_secs(1),||true).is_err());
    assert!(WsSource::approve(&f.manager,&f.host,&f.instance,&f.broker,f.manager.revision(),
        approval(&peer.ws_origin,"w15-second-attach"),1).is_err());
    f.broker.cleanup();assert_eq!(f.broker.snapshot().cleanup_proof,CleanupProof::NoProducer);
    assert_eq!(peer.finish().handshakes.load(Ordering::SeqCst),0);

    // A fresh duplex broker is still rejected by the ORIGINAL SSE factory.
    let f=Fixture::wat();
    let a=morrow_network_node_stream::managed_sse::Approval{
        request:RawHttpRequest{method:"POST".into(),target:"http://127.0.0.1:1/synthetic-events".into(),headers:vec![],body:vec![]},
        operation_id:"w15-old-sse-duplex".into(),approval_epoch:[0x62;32],deadline:Instant::now()+Duration::from_secs(1),
        profile:morrow_network_node_stream::managed_sse::NetworkProfile::LoopbackHttp,
        limits:Limits{max_request_bytes:4096,max_response_bytes:32768,max_header_bytes:4096,max_concurrent:1,timeout:Duration::from_secs(2)},
        decoder_limits:morrow_network_node_stream::sse::DecoderLimits{max_line_bytes:1024,max_event_bytes:4096,max_total_bytes:32768,
            max_events:16,max_id_bytes:1024,max_retry_digits:10},max_encoded_bytes:32768,
    };
    let result=morrow_network_node_stream::managed_sse::SseSource::approve(&f.manager,&f.host,&f.instance,&f.broker,f.manager.revision(),a,1);
    assert!(matches!(result,Err(morrow_network_node_stream::managed_sse::Error::Source(morrow_plugin_runtime::channel::Error::Denied))));
    f.broker.cleanup();assert_eq!(f.broker.snapshot().cleanup_proof,CleanupProof::NoProducer);
}
#[test]
fn full_bounded_inbound_pending_queue_pauses_read_without_limit_disconnect_and_outgoing_progresses(){
    let peer=Peer::new(Scenario::Burst);let mut f=Fixture::wat();
    let source=f.source(approval(&peer.ws_origin,"w15-bounded-pending"));f.start(&source);let _frame=f.frame();
    until(||peer.seen.burst_written.load(Ordering::SeqCst));
    assert_eq!(f.send(1,MessageKind::Binary,BINARY,40).status,Status::Accepted);
    until(||peer.seen.count()==1);
    f.no_ack();assert!(source.completion().is_none(),"full bounded pending is flow control, not Limit disconnection");
    assert_eq!(f.broker.snapshot().usage.messages,2,"no second admitted inbound frame before exact ACK");
    source.revoke();let done=f.joined(&source);
    assert_eq!(done.messages_queued,1);assert_eq!(done.messages_acked,0);assert_eq!(done.outgoing_written,1);
    // Inflight + two source pending slots; separate socket result/library bounds are not falsely counted here.
    let one=message(MessageKind::Binary,b"burst",None).encode().unwrap().len() as u64;
    let outgoing=message(MessageKind::Binary,BINARY,None).encode().unwrap().len() as u64;
    assert!(done.encoded_bytes<=3*one+outgoing,"bounded native encoded pending charge");
    f.unknown(&source);peer.finish();
}
