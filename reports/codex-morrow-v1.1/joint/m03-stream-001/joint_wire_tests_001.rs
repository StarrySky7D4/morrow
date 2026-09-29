use morrow_native_http_stream_wire::*;
#[path = "common/mod.rs"] mod fixture;
use std::{fs, path::PathBuf};
fn out() -> PathBuf { PathBuf::from(std::env::var_os("JOINT_CASE_DIR").unwrap()) }
fn emit(name: &str, f: &Frame) -> Vec<u8> {
    let bytes = f.encode().unwrap();
    assert_eq!(Frame::decode(&bytes).unwrap(), *f);
    fs::write(out().join(format!("{name}.frame")), &bytes).unwrap();
    println!("frame {name} bytes={} sha256={}", bytes.len(), hex(&digest(&bytes)));
    bytes
}
fn headers_max() -> Vec<Header> {
    let mut hs: Vec<_> = (0..32).map(|n| Header {name: format!("x{n}"), value: vec![]}).collect();
    let names: usize = hs.iter().map(|h| h.name.len() + 4).sum();
    hs[0].value = vec![255; 8192 - names]; hs
}
fn data_start(bytes: &[u8], ptr: usize) -> usize {
    let v = u64::from_le_bytes(bytes[ptr..ptr+8].try_into().unwrap());
    assert_eq!(v & 3, 0);
    let offset = ((v as u32 as i32) >> 2) as isize;
    (ptr as isize + 8 + offset * 8) as usize
}
#[test]
fn independent_consumption_of_all_sealed_vectors() {
    let dir = PathBuf::from(std::env::var_os("JOINT_VECTORS").unwrap());
    let mut good = 0; let mut bad = 0;
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|x| x == "frame") {
            let bytes = fs::read(&path).unwrap(); let f = Frame::decode(&bytes).unwrap();
            let expected = path.file_stem().unwrap().to_str().unwrap().replace('-', "");
            assert_eq!(format!("{:?}", f.kind).to_lowercase(), expected);
            assert_eq!(f.schema_sha256, schema_digest());
            assert_eq!(f.encode().unwrap(), bytes); good += 1;
        } else if path.extension().is_some_and(|x| x == "invalid") {
            assert!(Frame::decode(&fs::read(path).unwrap()).is_err()); bad += 1;
        }
    }
    assert_eq!((good,bad),(24,2)); println!("sealed vectors valid={good} invalid={bad}");
}
#[test]
fn maximum_reachable_metadata_headers_chunks_and_one_over_limits() {
    let mut f = fixture::initial(); f.operation_id = vec![b'o';256]; f.kind=Kind::HttpPrepare;
    let mut p = fixture::prepare(); p.method="A".repeat(16); p.absolute_target="x".repeat(2048);
    p.headers=headers_max(); p.body_bytes=MAX_BODY as u32; p.body_sha256=digest(&vec![0;MAX_BODY]).to_vec();
    f.payload=Payload::Prepare(p.clone()); emit("max-prepare-structural-not-http-policy", &f);
    for what in 0..6 { let mut q=p.clone(); match what {
        0 => q.headers.push(Header{name:"x".into(), value:vec![]}),
        1 => q.headers[0].value.push(0), 2=>q.method.push('A'), 3=>q.absolute_target.push('x'),
        4=>q.body_bytes+=1, _=>q.response_limit_bytes+=1
    }; f.payload=Payload::Prepare(q); assert!(f.encode().is_err(), "one-over {what}"); }
    f.kind=Kind::ResponseHead; f.payload=Payload::Head(Head{status:599,headers:headers_max(),remote_address:"x".repeat(128)});
    emit("max-head",&f);
    for (kind,limit,label) in [(Kind::RequestChunk,MAX_BODY,"max-request-chunk"),(Kind::BodyChunk,MAX_RESPONSE,"max-response-chunk")] {
        f.kind=kind; f.payload=Payload::Chunk(Chunk{offset:(limit-MAX_CHUNK) as u64,bytes:vec![255;MAX_CHUNK]}); emit(label,&f);
        if let Payload::Chunk(ref mut c)=f.payload {c.offset+=1;}
        assert!(f.encode().is_err());
        f.payload=Payload::Chunk(Chunk{offset:0,bytes:vec![0;MAX_CHUNK+1]}); assert!(f.encode().is_err());
    }
    f.payload=Payload::Chunk(Chunk{offset:u64::MAX,bytes:vec![0]}); assert!(f.encode().is_err());
}
#[test]
fn actual_maximum_payload_and_raw_unknown_version_enums_union_reserved() {
    let frame=fixture::initial(); let bytes=frame.encode().unwrap();
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()),0);
    let mut max=bytes.clone(); max.resize(MAX_FRAME,0);
    max[..4].copy_from_slice(&(MAX_PAYLOAD as u32).to_le_bytes());
    max[8..12].copy_from_slice(&(((MAX_PAYLOAD-8)/8) as u32).to_le_bytes());
    assert_eq!(Frame::decode(&max).unwrap(),frame);
    fs::write(out().join("maximum-payload-with-unused-segment-words.frame"),&max).unwrap();
    println!("maximum payload={} full_frame={}",MAX_PAYLOAD,max.len());
    max.extend([0;8]); max[..4].copy_from_slice(&((MAX_PAYLOAD+8) as u32).to_le_bytes());
    assert!(Frame::decode(&max).is_err());
    let root=data_start(&bytes,12);
    for (label,offset,val) in [("major",0,4u16),("revision",2,2),("kind",4,u16::MAX),("union",6,u16::MAX)] {
        let mut b=bytes.clone(); b[root+offset..root+offset+2].copy_from_slice(&val.to_le_bytes());
        assert!(Frame::decode(&b).is_err(),"{label}"); fs::write(out().join(format!("bad-{label}.frame")),b).unwrap();
    }
    let mut b=bytes.clone(); b[root+72]=1; assert!(Frame::decode(&b).is_err());
    let mut f=fixture::initial(); f.kind=Kind::State; f.payload=Payload::Progress(fixture::progress());
    let b=f.encode().unwrap(); let root=data_start(&b,12);
    let words=((u64::from_le_bytes(b[12..20].try_into().unwrap())>>32)&65535) as usize;
    let progress=data_start(&b,root+words*8+5*8);
    for offset in [0,2,4] {let mut bad=b.clone();bad[progress+offset..progress+offset+2].copy_from_slice(&65535u16.to_le_bytes());assert!(Frame::decode(&bad).is_err());}
}
#[test]
fn independent_python_hash_preimage_and_approved_limit_are_distinct() {
    let mut p=fixture::prepare(); p.headers.push(Header{name:"x-raw".into(),value:vec![255]});
    let oracle=fs::read_to_string(out().join("hash-oracle.txt")).unwrap();
    for (limit, expected) in [65536,1024].into_iter().zip(oracle.lines()) {
        p.response_limit_bytes=limit;
        assert_eq!(hex(&request_digest(1,2,b"op",1,&p,b"{}").unwrap()),expected);
    }
    p.response_limit_bytes=65536; let first=request_digest(1,2,b"op",1,&p,b"{}").unwrap();
    p.headers.reverse(); assert_ne!(request_digest(1,2,b"op",1,&p,b"{}").unwrap(),first);
    assert!(request_digest(1,2,b"op",1,&p,b"[]").is_err());
    assert_ne!(request_digest(1,3,b"op",1,&p,b"{}").unwrap(),first);
    let (_,mut approved)=fixture::cases().into_iter().find(|(n,_)|*n=="http-approved").unwrap();
    if let Payload::Decision(ref mut d)=approved.payload {d.response_limit_bytes=1024;}
    emit("tightened-approved-limit",&approved);
}
#[test]
fn credit_four_classes_overflow_and_offset_geometry() {
    let mut f=fixture::initial(); f.kind=Kind::HttpCredit;
    let c=Credit{consumed_offset:MAX_RESPONSE as u64,parser_yielded_bytes:100,drain_discarded_bytes:200,cancel_discarded_bytes:300,error_consumed_bytes:MAX_RESPONSE as u64-600,window_bytes:MAX_CREDIT,max_chunk_bytes:MAX_CHUNK as u32};
    f.payload=Payload::Credit(c.clone());emit("maximum-credit",&f);
    for mode in 0..5 {let mut bad=c.clone();match mode {0=>bad.parser_yielded_bytes+=1,1=>bad.parser_yielded_bytes=u64::MAX,2=>bad.window_bytes+=1,3=>bad.max_chunk_bytes+=1,_=>bad.max_chunk_bytes=0};f.payload=Payload::Credit(bad);assert!(f.encode().is_err());}
    let mut p=fixture::progress();p.received_offset=10;p.reserved_offset=8;p.issued_offset=6;p.os_completed_offset=0;p.peer_consumed_offset=6;p.parser_yielded_bytes=6;
    f.kind=Kind::State;f.payload=Payload::Progress(p.clone());emit("peer-ack-before-os-observation",&f);
    for mode in 0..5 {let mut bad=p.clone();match mode {0=>bad.os_completed_offset=7,1=>{bad.peer_consumed_offset=7;bad.parser_yielded_bytes=7},2=>bad.issued_offset=9,3=>bad.reserved_offset=11,_=>bad.received_offset=65537};f.payload=Payload::Progress(bad);assert!(f.encode().is_err());}
}
#[test]
fn orthogonal_observed_error_and_request_cleanup_without_own_exit() {
    let mut f=fixture::initial();f.kind=Kind::RequestClosed;
    let mut p=fixture::progress();p.intent=IntentPhase::Observed;p.http_eof=true;p.response_material_stored=true;
    p.network=NetworkPhase::Eof;p.worker_joined=true;p.request_closed=true;p.data_closed=true;p.error_code=24;
    p.revoke_persisted=true;p.revoke_applied=true;p.peer_consumed_offset=3;p.parser_yielded_bytes=3;
    assert!(!p.child_exited&&!p.stdout_eof&&!p.stderr_eof&&!p.owner_released);
    f.payload=Payload::Progress(p.clone());emit("request-closed-guest-still-alive",&f);
    for mode in 0..6 {let mut bad=p.clone();match mode {0=>bad.worker_joined=false,1=>bad.read_reaped=false,2=>bad.write_reaped=false,3=>bad.connect_reaped=false,4=>bad.http_eof=false,_=>bad.response_material_stored=false};f.payload=Payload::Progress(bad);assert!(f.encode().is_err());}
    p.intent=IntentPhase::Unknown;p.http_eof=false;p.response_material_stored=false;p.network=NetworkPhase::Cancelled;
    f.payload=Payload::Progress(p);emit("request-closed-cancelled-unknown",&f);
}
#[test]
fn structural_codec_never_promises_direction_or_live_authority() {
    let f=fixture::initial();let mut stale=f.clone();stale.sequence=999;stale.revocation_generation=2;stale.remaining_ms=1;stale.request_budget=1;stale.code=999;
    assert!(stale.same_admission(&f));assert!(stale.encode().is_ok());
    stale.kind=Kind::BodyChunk;stale.payload=Payload::Chunk(Chunk{offset:0,bytes:vec![1]});
    assert!(Frame::decode(&stale.encode().unwrap()).is_ok());
    println!("direction/state/generation/echo checks intentionally belong to driver");
}
