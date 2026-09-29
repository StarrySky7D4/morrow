use morrow_native_session_wire::{Frame,Kind,native_session_capnp::native_session as ns};
use capnp::{message,serialize};
use std::path::PathBuf;

fn framed(payload:Vec<u8>)->Vec<u8>{let mut f=(payload.len() as u32).to_le_bytes().to_vec();f.extend(payload);f}
fn mutated(base:&[u8],change:impl FnOnce(ns::Builder<'_>))->Vec<u8>{
    let msg=serialize::read_message(&mut &base[4..],message::ReaderOptions::new()).unwrap();
    let mut dst=message::Builder::new_default();
    dst.set_root(msg.get_root::<ns::Reader>().unwrap()).unwrap();
    change(dst.get_root::<ns::Builder>().unwrap());
    framed(serialize::write_message_to_words(&dst))
}
fn main(){
    let vectors=PathBuf::from(std::env::args_os().nth(1).expect("fixed vectors path"));
    let base=std::fs::read(vectors.join("challenge.frame")).unwrap();
    let initial=Frame::decode(&base).unwrap();
    assert_eq!(initial.pid,42);assert_eq!(initial.nonce,[7;32]);
    assert_eq!(initial.schema,morrow_native_session_wire::schema_digest());
    let expected=[("challenge",Kind::Challenge,0,0),("hello",Kind::Hello,1,0),
      ("welcome",Kind::Welcome,1,2),("query",Kind::Query,2,0),("state",Kind::State,2,2),
      ("denied-revoked",Kind::Denied,3,19),("stop",Kind::Stop,0,25),("close",Kind::Close,3,0)];
    for (name,kind,sequence,code) in expected {
        let f=Frame::decode(&std::fs::read(vectors.join(format!("{name}.frame"))).unwrap()).unwrap();
        assert_eq!((f.kind,f.sequence,f.code),(kind,sequence,code));
        assert_eq!(f.generation,if name=="denied-revoked"{2}else{1});
        assert_eq!(Frame::decode(&f.encode()).unwrap(),f);
    }
    println!("positive_vectors_and_roundtrips=8");
    for name in ["oversize-prefix","truncated","bad-root"] {
        assert!(Frame::decode(&std::fs::read(vectors.join(format!("{name}.invalid"))).unwrap()).is_err());
    }
    println!("provided_negative_vectors=3");
    for n in 0..base.len(){assert!(Frame::decode(&base[..n]).is_err(),"truncation {n}");}
    for n in [0u32,1,7,9,2049,2056,u32::MAX] {assert!(morrow_native_session_wire::payload_length(&n.to_le_bytes()).is_err());}
    println!("all_truncations={} boundary_prefixes=7",base.len());
    let mut failures=0;
    for bad in [
      mutated(&base,|mut b|b.set_major(3)),mutated(&base,|mut b|b.set_revision(2)),
      mutated(&base,|mut b|b.set_reserved(1)),mutated(&base,|mut b|b.set_nonce(&[0;31])),
      mutated(&base,|mut b|b.set_schema_sha256(&[0;33])),
      mutated(&base,|mut b|b.set_artifact_sha256(&[])),
      mutated(&base,|mut b|b.set_execution_config_sha256(&[0;31])),
    ] {assert!(Frame::decode(&bad).is_err());failures+=1;}
    println!("version_reserved_and_digest_length_rejections={failures}");
    let mut extra=base.clone();extra.extend_from_slice(&base[4..]);
    let n=(extra.len()-4) as u32;extra[..4].copy_from_slice(&n.to_le_bytes());
    assert!(Frame::decode(&extra).is_err());
    println!("second_message_rejected=1");
    // Read the generated field position from the generated accessor contract:
    // Kind occupies data u16 index2. This mutation is not an alternate schema.
    let payload=&base[4..];assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()),0);
    let root=u64::from_le_bytes(payload[8..16].try_into().unwrap());assert_eq!(root&3,0);
    let signed_offset=((root as i64 as i32)>>2) as isize;
    let data_start=(16isize+signed_offset*8) as usize;
    let mut unknown=base.clone();unknown[4+data_start+4..4+data_start+6].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(Frame::decode(&unknown).is_err());println!("unknown_enum_rejected=1");
    let wide=mutated(&base,|mut b|{b.set_session(u64::MAX);b.set_instance_epoch(u64::MAX-1);b.set_sequence(u64::MAX-2);});
    let wide=Frame::decode(&wide).unwrap();assert_eq!(wide.session,u64::MAX);assert_eq!(wide.epoch,u64::MAX-1);assert_eq!(wide.sequence,u64::MAX-2);
    println!("u64_preserved=3");
    // Oversized allocation declaration, rejected before reading its missing words.
    let mut declared=vec![0u8;8];declared[4..8].copy_from_slice(&2048u32.to_le_bytes());
    assert!(Frame::decode(&framed(declared)).is_err());println!("oversized_segment_declaration_rejected=1");
    // The schema has four flat Data fields, no recursive structs. No claim that
    // these tests dynamically reach the configured nesting8/traversal1024 edge.
    println!("limited_wire_consumer_passed=true real_ipc=false depth_threshold_exercised=false");
}
