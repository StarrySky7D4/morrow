//! Synthetic metadata codec tests; no transport, guest grants or business completion.
#![cfg(all(feature = "managed-channel", not(target_arch = "wasm32")))]
use morrow_network_node_stream::managed_sse::{EventEnvelope, Error, MAX_ENVELOPE_BYTES, VERSION, schema_digest};
fn event(retry: Option<u64>) -> EventEnvelope {
    EventEnvelope {
        data:"synthetic-wire-utf8-marker: 你🙂\nsecond line\0literal NUL data".into(),
        event:"合成-delta".into(), id:"i".repeat(300), retry,
    }
}
// Locate an actual encoded allocation; no guessed byte offset or private Core API.
fn unique_payload_offset(wire: &[u8], payload: &[u8]) -> usize {
    let mut matches=wire.windows(payload.len()).enumerate().filter(|(_,bytes)|*bytes==payload);
    let position=matches.next().expect("real synthetic allocation exists").0;
    assert!(matches.next().is_none(),"mutation must identify one exact allocation");
    position
}
fn root_data_offset(wire: &[u8]) -> usize {
    // These small messages use the standard one-segment Capnp framing. Decode the
    // actual root struct pointer and its signed relative offset rather than
    // assuming where the version field's data section was allocated.
    assert_eq!(u32::from_le_bytes(wire[..4].try_into().unwrap()),0,"one segment");
    let words=u32::from_le_bytes(wire[4..8].try_into().unwrap()) as usize;
    assert_eq!(wire.len(),8+words*8);
    let pointer=u64::from_le_bytes(wire[8..16].try_into().unwrap());
    assert_eq!(pointer&3,0,"actual root struct pointer");
    let relative=((pointer as u32) as i32)>>2;
    let offset=16i64+i64::from(relative)*8;
    let data_words=((pointer>>32)&0xffff) as usize;
    let pointer_words=(pointer>>48) as usize;
    assert!(data_words>=1 && pointer_words>=4);
    let offset=usize::try_from(offset).unwrap();
    assert!(offset+(data_words+pointer_words)*8<=wire.len());
    offset
}

#[test]
fn complete_unicode_long_id_and_retry_presence_roundtrip_without_truncation() {
    let mut encodings=Vec::new();
    for retry in [None,Some(0),Some(u64::MAX)] {
        let original=event(retry); let wire=original.encode().unwrap();
        assert!(wire.len()<=MAX_ENVELOPE_BYTES);
        let decoded=EventEnvelope::decode(&wire).unwrap();
        assert!(decoded==original,"owned complete metadata roundtrip");
        assert_eq!(decoded.id.len(),300); assert_eq!(decoded.retry,retry);
        encodings.push(wire);
    }
    assert!(encodings[0]!=encodings[1],"None must be distinct from explicitly supplied retry zero");
    assert!(encodings[1]!=encodings[2]);
}

#[test]
fn wrong_version_digest_trailing_bytes_and_actual_invalid_utf8_fail_closed() {
    let original=event(Some(0)); let wire=original.encode().unwrap();
    let data_offset=root_data_offset(&wire);
    assert_eq!(u16::from_le_bytes(wire[data_offset..data_offset+2].try_into().unwrap()),VERSION);
    let mut bad_version=wire.clone();
    bad_version[data_offset..data_offset+2].copy_from_slice(&(VERSION+1).to_le_bytes());
    assert_eq!(EventEnvelope::decode(&bad_version).err(),Some(Error::Invalid));
    let digest=schema_digest(); let digest_offset=unique_payload_offset(&wire,&digest);
    let mut bad_digest=wire.clone(); bad_digest[digest_offset]^=0x80;
    assert_eq!(EventEnvelope::decode(&bad_digest).err(),Some(Error::Invalid));
    let mut trailing=wire.clone(); trailing.push(0);
    assert_eq!(EventEnvelope::decode(&trailing).err(),Some(Error::Invalid));
    // The mutation targets real Text payload data, leaving its pointer/length,
    // NUL terminator, version and schema digest intact.
    for payload in [&original.data,&original.event,&original.id] {
        let offset=unique_payload_offset(&wire,payload.as_bytes());
        let mut malformed=wire.clone(); malformed[offset]=0xff;
        assert_eq!(EventEnvelope::decode(&malformed).err(),Some(Error::Invalid),"strict UTF-8 for every metadata field");
    }
    assert!(EventEnvelope::decode(&wire).unwrap()==original,"untouched control remains valid");
}

#[test]
fn aggregate_source_fields_and_serialized_overhead_have_separate_limits() {
    // No single field exceeds the ceiling: the owned aggregate does.
    let aggregate=EventEnvelope { data:"d".repeat(MAX_ENVELOPE_BYTES/2),event:"event".into(),
        id:"i".repeat(MAX_ENVELOPE_BYTES/2),retry:None };
    assert!(aggregate.data.len()<=MAX_ENVELOPE_BYTES && aggregate.id.len()<=MAX_ENVELOPE_BYTES);
    assert_eq!(aggregate.encode().err(),Some(Error::Limit));
    // Exactly the source-byte ceiling still cannot hide schema/pointer/framing
    // overhead. It must reject rather than truncate metadata or exceed the wire cap.
    let overhead=EventEnvelope { data:"d".repeat(MAX_ENVELOPE_BYTES),event:String::new(),id:String::new(),retry:None };
    assert_eq!(overhead.data.len()+overhead.event.len()+overhead.id.len(),MAX_ENVELOPE_BYTES);
    assert_eq!(overhead.encode().err(),Some(Error::Limit));
    let bounded=EventEnvelope { data:"d".repeat(MAX_ENVELOPE_BYTES-512),event:"message".into(),id:"i".repeat(64),retry:Some(u64::MAX) };
    let wire=bounded.encode().unwrap(); assert!(wire.len()<=MAX_ENVELOPE_BYTES);
    assert!(EventEnvelope::decode(&wire).unwrap()==bounded,"large valid owned control is preserved");
    assert_eq!(EventEnvelope::decode(&vec![0;MAX_ENVELOPE_BYTES+1]).err(),Some(Error::Limit));
}
