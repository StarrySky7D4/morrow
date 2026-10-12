//! Pure policy/codec/flush tests only. No worker, Store, supervisor or success fixture.
use super::*;
fn request(op:Operation,id:u8)->Vec<u8> {
    let mut raw=vec![0;REQUEST_BYTES];raw[..8].copy_from_slice(MAGIC);raw[8]=1;raw[9]=op as u8;
    raw[12..44].fill(id);raw[44..76].fill(2);raw[76..92].fill(3);
    if op!=Operation::Start {raw[92..124].fill(4);}
    if matches!(op,Operation::Start|Operation::Ack) {raw[124..156].fill(5);raw[156..188].fill(6);}
    raw
}
#[test]
fn codec_rejects_ambiguous_shapes_and_zero_correlations() {
    let original=request(Operation::Start,1);assert!(Request::decode(&original).is_ok());
    for (at,value) in [(8,0),(9,5),(10,1),(92,1)] {
        let mut raw=original.clone();raw[at]=value;assert!(Request::decode(&raw).is_err());
    }
    for range in [12..44,44..76,76..92,124..156,156..188] {
        let mut raw=original.clone();raw[range].fill(0);assert!(Request::decode(&raw).is_err());
    }
    let mut extra=original.clone();extra.push(0);assert!(Request::decode(&extra).is_err());
    assert!(Request::decode(&original[..187]).is_err());
}
#[test]
fn stop_poll_have_no_hidden_barrier_fields() {
    for op in [Operation::Stop,Operation::Poll] {
        let mut raw=request(op,1);assert!(Request::decode(&raw).is_ok());
        raw[124]=1;assert!(Request::decode(&raw).is_err());
    }
}
#[test]
fn start_requires_both_actual_delivered_pair_and_idle_observation() {
    let q=Request::decode(&request(Operation::Start,1)).unwrap();
    let mut p=Policy::default();assert!(p.reserve(q).is_err());assert!(!p.start);
    let mut p=Policy{startup:Some(q.barrier),..Default::default()};
    assert!(p.reserve(q).is_err());assert!(!p.start);
    let mut p=Policy{startup:Some(q.barrier),idle:true,..Default::default()};
    p.reserve(q).unwrap();assert!(p.start);
    assert!(p.reserve(Request::decode(&request(Operation::Start,7)).unwrap()).is_err());
    assert!(p.start); // Failure never resets a consumed effect latch.
}
#[test]
fn ack_requires_retained_pair_and_is_permanent_even_without_reply() {
    let q=Request::decode(&request(Operation::Ack,1)).unwrap();
    let mut p=Policy{start:true,stop:true,..Default::default()};assert!(p.reserve(q).is_err());
    let mut p=Policy{start:true,stop:true,retained:Some(q.barrier),..Default::default()};
    p.reserve(q).unwrap();p.fenced=true;
    assert!(p.reserve(Request::decode(&request(Operation::Ack,7)).unwrap()).is_err());assert!(p.ack);
}
#[test]
fn fence_never_restores_start_or_ack_but_exact_cleanup_can_be_observed() {
    let mut p=Policy{start:true,fenced:true,..Default::default()};
    p.reserve(Request::decode(&request(Operation::Stop,1)).unwrap()).unwrap();
    p.reserve(Request::decode(&request(Operation::Poll,2)).unwrap()).unwrap();
    assert!(p.fenced);assert!(p.reserve(Request::decode(&request(Operation::Ack,3)).unwrap()).is_err());
}
#[test]
fn duplicate_poll_ids_and_poll_bound_fail_closed() {
    let mut p=Policy{start:true,stop:true,..Default::default()};let q=Request::decode(&request(Operation::Poll,1)).unwrap();
    p.reserve(q).unwrap();assert!(p.reserve(q).is_err());
    let mut p=Policy{start:true,stop:true,polls:1000,..Default::default()};assert!(p.reserve(q).is_err());
}
struct FlushFailure { raw:Vec<u8>, flushes:u32 }
impl Write for FlushFailure {
    fn write(&mut self,raw:&[u8])->std::io::Result<usize>{self.raw.extend(raw);Ok(raw.len())}
    fn flush(&mut self)->std::io::Result<()>{self.flushes+=1;Err(std::io::Error::other("injected flush failure"))}
}
#[test]
fn full_write_without_flush_is_delivery_failure() {
    let mut output=FlushFailure{raw:vec![],flushes:0};let gate=Default::default();
    assert!(deliver_reply(&mut output,&[0;REPLY_BYTES],&gate).is_err());
    assert_eq!(output.raw.len(),4+REPLY_BYTES);assert_eq!(output.flushes,1);
    // Production caller fences before returning this error; no delivered marker runs.
}
#[test]
fn revoked_gate_prevents_first_byte() {
    let gate=morrow_workbench_host::product_gate::ProductGate::default();gate.revoke();
    let mut output=Vec::new();assert!(deliver_reply(&mut output,&[0;REPLY_BYTES],&gate).is_err());assert!(output.is_empty());
}
#[test]
fn malformed_or_error_public_response_is_never_startup_barrier() {
    assert!(!startup_response(&[],&[]));assert!(!startup_response(b"pageVersioned",b"ready"));
    let mut q=capnp::message::Builder::new_default();
    {let mut r=q.init_root::<morrow_workbench_host::host_capnp::request::Builder>();r.set_version(1);
        r.set_digest(&morrow_workbench_host::host_protocol_digest());r.set_action(morrow_workbench_host::host_capnp::Action::PageVersioned);r.set_limit(1);}
    let mut r=capnp::message::Builder::new_default();
    {let mut v=r.init_root::<morrow_workbench_host::host_capnp::response::Builder>();v.set_version(1);
        v.set_digest(&morrow_workbench_host::host_protocol_digest());v.set_error("Busy");}
    assert!(!startup_response(&serialize::write_message_to_words(&q),&serialize::write_message_to_words(&r)));
}
#[test]
fn fixed_plan_rejects_whitespace_unknown_fields_and_nonfixed_capability_policy() {
    // Pure decoder only: these dummy paths/identities never touch the filesystem.
    let v=serde_json::json!(["c28-product-session-qualification-v1",
        "C:/test/c28-product-session-01010101010101010101010101010101","run",
        "01010101010101010101010101010101","C:/test/host.exe",1,"01".repeat(32),
        "C:/test/supervisor.exe",1,"02".repeat(32),"C:/test/package",1,"03".repeat(32),
        "C:/test/fixture",425912,b::FIXTURE_SHA,40000,20000,60000]);
    let raw=serde_json::to_vec(&v).unwrap();assert!(b::Plan::decode(&raw,b::sha(&raw)).is_ok());
    let mut extra=raw.clone();extra.push(b'\n');assert!(b::Plan::decode(&extra,b::sha(&extra)).is_err());
    let mut v=v.clone();v.as_array_mut().unwrap().push(serde_json::json!(false));
    let raw=serde_json::to_vec(&v).unwrap();assert!(b::Plan::decode(&raw,b::sha(&raw)).is_err());
}
#[test]
fn win32_device_components_rejected_before_any_open() {
    for leaf in ["NUL","NUL .txt","con.txt","aUx","PRN.data","COM1","com9.ext","LPT1","lpt9.log","CONIN$","conout$.x"] {
        assert!(b::path(&format!("C:/parent/{leaf}")).is_err());
    }
    for leaf in ["COM0","COM10","LPT0","LPT10","normal.txt"] {assert!(b::path(&format!("C:/parent/{leaf}")).is_ok());}
}
