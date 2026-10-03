//! W15 independent payload codec tests; these do not prove any network or guest route.
#![cfg(all(feature="managed-websocket",not(target_arch="wasm32")))]
use morrow_network_node_stream::{
    managed_ws::{MessageEnvelope,MessageKind,MAX_ENVELOPE_BYTES,schema_digest},
};
fn word(raw:&[u8],at:usize)->u64{u64::from_le_bytes(raw[at..at+8].try_into().unwrap())}
fn canonical(message:&MessageEnvelope)->Vec<u8>{
    let total=72+((message.payload.len()+7)&!7);let mut raw=vec![0;total];
    for(at,value)in[(0,((total/8-1)as u64)<<32),(8,(1u64<<32)|(2u64<<48)),
        (16,1|((message.kind as u64)<<16)|((message.close_code.is_some()as u64)<<32)|((message.close_code.unwrap_or(0)as u64)<<48)),
        (24,5|(2u64<<32)|(32u64<<35)),(32,17|(2u64<<32)|((message.payload.len()as u64)<<35))]{
        raw[at..at+8].copy_from_slice(&value.to_le_bytes());
    }
    raw[40..72].copy_from_slice(&schema_digest());raw[72..72+message.payload.len()].copy_from_slice(&message.payload);raw
}
#[test]
fn actual_native_encoding_matches_new_fixture_layout_and_complete_typed_roundtrips(){
    for (kind,payload,close_code) in [
        (MessageKind::Text,"W15 雪 🙂".as_bytes().to_vec(),None),
        (MessageKind::Binary,vec![0,255,128,10,13,0,37],None),
        (MessageKind::Ping,vec![0x61;125],None),
        (MessageKind::Pong,vec![],None),
        (MessageKind::Close,vec![],None),
        (MessageKind::Close,vec![],Some(1000)),
    ]{
        let message=MessageEnvelope{kind,payload,close_code};let actual=message.encode().unwrap();
        assert_eq!(word(&actual,0),((actual.len()/8-1)as u64)<<32);
        assert_eq!(word(&actual,8),(1u64<<32)|(2u64<<48),"actual generated root data1 ptr2");
        assert_eq!(actual,canonical(&message),"NEW C/C++/Rust fixture layout must match real native Capnp encode");
        let decoded=MessageEnvelope::decode(&actual).unwrap();assert!(decoded==message);
    }
}
#[test]
fn wrong_version_digest_trailing_bytes_utf8_and_close_metadata_are_fail_closed(){
    let text=MessageEnvelope{kind:MessageKind::Text,payload:b"UNIQUE-W15-PAYLOAD".to_vec(),close_code:None};
    let raw=text.encode().unwrap();assert_eq!(raw,canonical(&text));
    let mut wrong=raw.clone();wrong[16]=2;assert!(MessageEnvelope::decode(&wrong).is_err());
    let mut wrong=raw.clone();wrong[40]^=1;assert!(MessageEnvelope::decode(&wrong).is_err());
    let mut wrong=raw.clone();wrong.extend_from_slice(&[0;8]);assert!(MessageEnvelope::decode(&wrong).is_err());
    let positions:Vec<_>=raw.windows(text.payload.len()).enumerate().filter_map(|(i,v)|(v==text.payload).then_some(i)).collect();
    assert_eq!(positions,vec![72],"actual payload allocation validated before UTF-8 corruption");
    let mut wrong=raw.clone();wrong[positions[0]]=0xff;assert!(MessageEnvelope::decode(&wrong).is_err());
    let mut wrong=raw.clone();wrong[18]=0xff;wrong[19]=0xff;assert!(MessageEnvelope::decode(&wrong).is_err());
    let mut wrong=raw.clone();wrong[20]|=1;wrong[22..24].copy_from_slice(&1000u16.to_le_bytes());
    assert!(MessageEnvelope::decode(&wrong).is_err(),"non-Close carrying code rejected");
    assert!(MessageEnvelope{kind:MessageKind::Close,payload:vec![],close_code:Some(1005)}.encode().is_err());
    assert!(MessageEnvelope{kind:MessageKind::Close,payload:b"reason".to_vec(),close_code:None}.encode().is_err());
}
#[test]
fn encoded_overhead_data_ceiling_and_control_ceiling_remain_separate(){
    let near=MessageEnvelope{kind:MessageKind::Binary,payload:vec![0x7a;MAX_ENVELOPE_BYTES-256],close_code:None};
    let encoded=near.encode().unwrap();assert!(encoded.len()<=MAX_ENVELOPE_BYTES);
    // Native allocator may use multiple segments: do not assume one-segment overhead for large encode.
    let exact=MessageEnvelope{kind:MessageKind::Binary,payload:vec![0x7a;MAX_ENVELOPE_BYTES-72],close_code:None};
    let exact_wire=canonical(&exact);assert_eq!(exact_wire.len(),MAX_ENVELOPE_BYTES);
    assert!(MessageEnvelope::decode(&exact_wire).unwrap()==exact);
    assert!(MessageEnvelope::decode(&encoded).unwrap()==near);
    let excess=MessageEnvelope{kind:MessageKind::Binary,payload:vec![0;MAX_ENVELOPE_BYTES],close_code:None};
    assert!(excess.encode().is_err(),"encoded overhead may exceed cap even when payload alone fits");
    assert!(MessageEnvelope::decode(&vec![0;MAX_ENVELOPE_BYTES+1]).is_err());
    for kind in [MessageKind::Ping,MessageKind::Pong] {
        assert!(MessageEnvelope{kind,payload:vec![0;125],close_code:None}.encode().is_ok());
        assert!(MessageEnvelope{kind,payload:vec![0;126],close_code:None}.encode().is_err());
    }
    assert!(MessageEnvelope{kind:MessageKind::Close,payload:vec![b'x';124],close_code:Some(1000)}.encode().is_err());
}
