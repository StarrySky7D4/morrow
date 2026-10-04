//! W15 actual synthetic WebSocket transport; no host or SDK qualification is inferred.
#![cfg(all(feature="managed-websocket",not(target_arch="wasm32")))]
#[path = "support/managed_ws_support.rs"] mod support;
use support::{Peer, Scenario, TEXT, BINARY, WAIT};
use morrow_network_node_stream::{
    client::{Client, EndpointPolicy}, Limits, RawHttpRequest,
    stream::SendContext,
    websocket::{self, Message, MessageKind, Quotas, Error, OpeningCleanup},
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
fn client_for(peer: &Peer) -> Client {
    Client::new(EndpointPolicy::new(&peer.http_origin, &["GET"], true).unwrap(),
        Limits { timeout: WAIT, ..Limits::default() }).unwrap()
}
fn request(peer: &Peer) -> RawHttpRequest {
    RawHttpRequest { method: "GET".into(), target: format!("{}/synthetic-ws",peer.http_origin),
        headers: vec![], body: vec![] }
}
fn message(kind: MessageKind, payload: &[u8]) -> Message {
    Message { kind, payload: payload.to_vec(), close_code: None }
}
#[tokio::test(flavor="multi_thread", worker_threads=2)]
async fn mature_peer_observes_text_binary_and_actual_close_with_owned_join_receipt() {
    let peer=Peer::new(Scenario::EchoDuplex); let client=client_for(&peer);
    let mut lease=websocket::open(&client,request(&peer),vec![],Quotas::default(),
        SendContext::trusted(tokio::time::Instant::now()+WAIT,CancellationToken::new())).await.unwrap();
    lease.send_message(message(MessageKind::Text,TEXT.as_bytes())).await.unwrap();
    let first=lease.next_message().await.unwrap().unwrap();
    assert!(matches!(first.kind,MessageKind::Text)); assert_eq!(first.payload,TEXT.as_bytes());
    lease.send_message(message(MessageKind::Binary,BINARY)).await.unwrap();
    let second=lease.next_message().await.unwrap().unwrap();
    assert!(matches!(second.kind,MessageKind::Binary)); assert_eq!(second.payload,BINARY);
    lease.send_message(Message{kind:MessageKind::Close,payload:vec![],close_code:Some(1000)}).await.unwrap();
    let close=lease.next_message().await.unwrap().unwrap();
    assert!(matches!(close.kind,MessageKind::Close)); assert_eq!(close.close_code,Some(1000));
    assert!(lease.next_message().await.unwrap().is_none(),"Close alone is not transport EOF");
    let done=lease.finish().await; assert!(done.worker_joined);
    assert!(done.outcome.is_ok(),"valid mature peer close must complete");
    assert!(done.peer_close.is_some()); assert!(done.outgoing_messages>=3);
    let seen=peer.finish(); let messages=seen.messages.lock().unwrap();
    assert_eq!(messages.len(),2); assert_eq!(messages[0],(1,TEXT.as_bytes().to_vec()));
    assert_eq!(messages[1],(2,BINARY.to_vec())); assert!(seen.closed.load(std::sync::atomic::Ordering::SeqCst));
}
#[tokio::test(flavor="multi_thread", worker_threads=2)]
async fn raw_fragmentation_preserves_split_utf8_and_control_before_assembled_message() {
    let peer=Peer::new(Scenario::Fragmented); let client=client_for(&peer);
    let mut lease=websocket::open(&client,request(&peer),vec![],Quotas::default(),
        SendContext::trusted(tokio::time::Instant::now()+WAIT,CancellationToken::new())).await.unwrap();
    let mut ping=false; let mut text=false;
    for _ in 0..3 {
        let next=lease.next_message().await.unwrap().unwrap();
        match next.kind {
            MessageKind::Ping => { assert_eq!(next.payload,b"fragment-ping");ping=true; }
            MessageKind::Text => { assert_eq!(next.payload,TEXT.as_bytes());text=true; }
            MessageKind::Close => { assert_eq!(next.close_code,Some(1000)); }
            _ => panic!("unexpected synthetic fragmented message kind"),
        }
        if ping&&text {break}
    }
    assert!(ping&&text,"control and fully assembled UTF-8 text are separate observations");
    let done=lease.cancel_and_wait().await; assert!(done.worker_joined);
    peer.finish();
}
#[tokio::test(flavor="multi_thread", worker_threads=2)]
async fn malformed_handshake_and_unapproved_subprotocol_fail_before_message_delivery() {
    for scenario in [Scenario::WrongAccept,Scenario::MissingUpgrade,Scenario::UnrequestedProtocol] {
        let peer=Peer::new(scenario); let client=client_for(&peer);
        let failed=websocket::open(&client,request(&peer),vec![],Quotas::default(),
            SendContext::trusted(tokio::time::Instant::now()+WAIT,CancellationToken::new())).await;
        let error=match failed { Err(error)=>error,Ok(_)=>panic!("invalid handshake accepted") };
        assert!(matches!(error.cause,Error::Handshake|Error::Protocol));
        match error.cleanup {OpeningCleanup::Worker(done)=>assert!(done.worker_joined),
            OpeningCleanup::NoWorker=>panic!("actual attempted handshake must retain cleanup evidence")}
        assert_eq!(peer.finish().count(),0);
    }
}
#[tokio::test(flavor="multi_thread", worker_threads=2)]
async fn malicious_raw_frames_fail_closed_with_real_worker_join_and_no_reconnect() {
    for scenario in [Scenario::InvalidUtf8,Scenario::MaskedServer,Scenario::InvalidControl,Scenario::UnknownOpcode,Scenario::AbruptEof] {
        let peer=Peer::new(scenario); let client=client_for(&peer);
        let mut lease=websocket::open(&client,request(&peer),vec![],Quotas::default(),
            SendContext::trusted(tokio::time::Instant::now()+WAIT,CancellationToken::new())).await.unwrap();
        let error=match lease.next_message().await {Err(error)=>error,Ok(_)=>panic!("invalid wire accepted as message or clean EOF")};
        if matches!(scenario,Scenario::InvalidUtf8) { assert!(matches!(error,Error::Utf8)); }
        else if !matches!(scenario,Scenario::AbruptEof) { assert!(matches!(error,Error::Protocol)); }
        let done=lease.cancel_and_wait().await; assert!(done.worker_joined); assert!(done.outcome.is_err());
        let seen=peer.finish(); assert_eq!(seen.handshakes.load(std::sync::atomic::Ordering::SeqCst),1);
    }
}
#[tokio::test(flavor="multi_thread", worker_threads=2)]
async fn message_and_cumulative_wire_bounds_refuse_without_refund_or_false_delivery() {
    let peer=Peer::new(Scenario::InitialText); let client=client_for(&peer);
    let mut quotas=Quotas::default();quotas.max_message_bytes=4;quotas.max_frame_bytes=4;
    let mut lease=websocket::open(&client,request(&peer),vec![],quotas,
        SendContext::trusted(tokio::time::Instant::now()+WAIT,CancellationToken::new())).await.unwrap();
    assert!(matches!(lease.next_message().await,Err(Error::Limit)));
    let done=lease.cancel_and_wait().await;assert!(done.worker_joined);assert!(done.outcome.is_err());peer.finish();

    let peer=Peer::new(Scenario::EchoDuplex);let client=client_for(&peer);let mut quotas=Quotas::default();
    quotas.max_outgoing_wire_bytes=8;
    let lease=websocket::open(&client,request(&peer),vec![],quotas,
        SendContext::trusted(tokio::time::Instant::now()+WAIT,CancellationToken::new())).await.unwrap();
    assert!(matches!(lease.send_message(message(MessageKind::Binary,BINARY)).await,Err(Error::Limit)));
    let done=lease.cancel_and_wait().await;assert!(done.worker_joined);
    assert_eq!(peer.finish().count(),0,"wire reservation denies before peer sees a message");
}
#[tokio::test(flavor="multi_thread", worker_threads=2)]
async fn absolute_deadline_cancel_and_preflight_have_distinct_cleanup_facts() {
    let peer=Peer::new(Scenario::InitialText);let client=client_for(&peer);
    let deadline=tokio::time::Instant::now()+Duration::from_millis(500);
    let mut lease=websocket::open(&client,request(&peer),vec![],Quotas::default(),
        SendContext::trusted(deadline,CancellationToken::new())).await.unwrap();
    lease.next_message().await.unwrap().unwrap();
    tokio::time::sleep_until(deadline+Duration::from_millis(20)).await;
    assert!(matches!(lease.next_message().await,Err(Error::Transport(morrow_network_node_stream::Error::Timeout))));
    assert!(lease.cancel_and_wait().await.worker_joined);peer.finish();

    let peer=Peer::new(Scenario::InitialText);let client=client_for(&peer);let cancel=CancellationToken::new();
    let mut lease=websocket::open(&client,request(&peer),vec![],Quotas::default(),
        SendContext::trusted(tokio::time::Instant::now()+WAIT,cancel.clone())).await.unwrap();
    cancel.cancel();assert!(matches!(lease.next_message().await,Err(Error::Transport(morrow_network_node_stream::Error::Cancelled))));
    assert!(lease.cancel_and_wait().await.worker_joined);peer.finish();

    let peer=Peer::new(Scenario::InitialText);let client=client_for(&peer);
    let mut bad=request(&peer);bad.body.push(1);
    let failed=websocket::open(&client,bad,vec![],Quotas::default(),
        SendContext::trusted(tokio::time::Instant::now()+WAIT,CancellationToken::new())).await;
    let error=match failed {Err(error)=>error,Ok(_)=>panic!("nonempty upgrade body accepted")};
    assert!(matches!(error.cleanup,OpeningCleanup::NoWorker));assert_eq!(peer.finish().count(),0);
}

#[tokio::test(flavor="multi_thread", worker_threads=2)]
async fn dropping_opening_cancels_actual_handshake_without_waiting_for_deadline() {
    let peer=Peer::new(Scenario::StallOpening);let client=client_for(&peer);let request=request(&peer);
    let opening=tokio::spawn(async move {
        websocket::open(&client,request,vec![],Quotas::default(),
            SendContext::trusted(tokio::time::Instant::now()+WAIT,CancellationToken::new())).await
    });
    tokio::time::timeout(Duration::from_secs(2),async {
        while !peer.seen.opening_started.load(std::sync::atomic::Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }).await.expect("actual opening reached peer");
    opening.abort();assert!(opening.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(2),async {
        while !peer.seen.closed.load(std::sync::atomic::Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }).await.expect("opening drop closes the socket before the original deadline");
    assert_eq!(peer.finish().handshakes.load(std::sync::atomic::Ordering::SeqCst),0);
}
#[tokio::test(flavor="multi_thread", worker_threads=2)]
async fn cancelled_receive_preserves_demand_and_local_lease_cancel_denies_further_delivery() {
    let peer=Peer::new(Scenario::DelayedText);let client=client_for(&peer);
    let mut lease=websocket::open(&client,request(&peer),vec![],Quotas::default(),
        SendContext::trusted(tokio::time::Instant::now()+WAIT,CancellationToken::new())).await.unwrap();
    assert!(tokio::time::timeout(Duration::from_millis(20),lease.next_message()).await.is_err());
    peer.seen.release.store(true,std::sync::atomic::Ordering::SeqCst);
    let text=lease.next_message().await.unwrap().unwrap();assert_eq!(text.payload,TEXT.as_bytes());
    lease.cancel();
    assert!(matches!(lease.next_message().await,Err(Error::Transport(morrow_network_node_stream::Error::Cancelled))));
    assert!(matches!(lease.send_message(message(MessageKind::Binary,BINARY)).await,
        Err(Error::Transport(morrow_network_node_stream::Error::Cancelled))));
    let done=lease.cancel_and_wait().await;assert!(done.worker_joined);assert!(done.outcome.is_err());
    assert_eq!(peer.finish().count(),0);
}

#[tokio::test(flavor="multi_thread", worker_threads=2)]
async fn local_cancel_joins_actual_backpressured_socket_sink_before_original_deadline() {
    let peer=Peer::new(Scenario::StalledSink);let client=client_for(&peer);
    // Every limit remains finite and inside the existing transport profile.
    // The peer never drains application frames, so the real TCP sink must stall.
    let quotas=Quotas {max_message_bytes:64*1024,max_frame_bytes:64*1024,
        max_outgoing_messages:512,max_outgoing_frames:512,max_outgoing_wire_bytes:32*1024*1024,
        ..Quotas::default()};
    let lease=websocket::open(&client,request(&peer),vec![],quotas,
        SendContext::trusted(tokio::time::Instant::now()+WAIT,CancellationToken::new())).await.unwrap();
    let payload=vec![0x5a;64*1024];let mut completed=0u64;let mut pending=false;
    for _ in 0..quotas.max_outgoing_messages {
        match tokio::time::timeout(Duration::from_millis(80),lease.send_message(message(MessageKind::Binary,&payload))).await {
            Ok(Ok(()))=>completed+=1,
            Ok(Err(error))=>panic!("quota or transport failure cannot stand in for backpressure: {error:?}"),
            Err(_)=>{pending=true;break;},
        }
    }
    assert!(pending,"a genuinely pending send is required; unsaturated runs do not pass");
    assert!(completed>=8,"establish substantial actual flushes before pending send");
    let began=tokio::time::Instant::now();
    let done=tokio::time::timeout(Duration::from_secs(1),lease.cancel_and_wait()).await
        .expect("cancel interrupts the real sink before the original deadline");
    assert!(done.worker_joined);assert_eq!(done.outcome,Err(Error::Transport(morrow_network_node_stream::Error::Cancelled)));
    assert!(done.outgoing_wire_bytes>=completed*(payload.len()as u64+14),"masked large-frame bytes really reached the transport");
    eprintln!("bounded saturated sink: {completed} flushed messages, {} wire bytes, {} ms cancel/join",done.outgoing_wire_bytes,began.elapsed().as_millis());
    assert_eq!(peer.finish().count(),0,"the synthetic peer deliberately read no application frames");
}
