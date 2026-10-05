// Actual original native EventEnvelope oracle; no hand-written decoder.
use morrow_network_node_stream::managed_sse::{EventEnvelope,Error};
use std::{env,fs};
#[cfg(sse_sdk_candidate)]
use morrow_sse_event_v1 as sdk;
fn num(b:&[u8],at:&mut usize)->u32{let v=u32::from_le_bytes(b[*at..*at+4].try_into().unwrap());*at+=4;v}
fn take<'a>(b:&'a[u8],at:&mut usize,n:usize)->&'a[u8]{let v=&b[*at..*at+n];*at+=n;v}
fn main(){
 let args:Vec<_>=env::args().collect();assert!(args.len()==2||args.len()==3);let canonical=args.get(2).is_some_and(|s|s=="--canonical");let b=fs::read(&args[1]).unwrap();assert_eq!(&b[..8],b"SEVC0001");let mut at=8;let count=num(&b,&mut at);let(mut accept,mut reject,mut limits)=(0,0,0);
 for _ in 0..count{
  let n=num(&b,&mut at)as usize;let ok=num(&b,&mut at)!=0;let enc=num(&b,&mut at)!=0;let has=num(&b,&mut at)!=0;let retry=num(&b,&mut at)as u64|((num(&b,&mut at)as u64)<<32);let dn=num(&b,&mut at)as usize;let en=num(&b,&mut at)as usize;let idn=num(&b,&mut at)as usize;let wn=num(&b,&mut at)as usize;
  let name=std::str::from_utf8(take(&b,&mut at,n)).unwrap();let d=take(&b,&mut at,dn);let e=take(&b,&mut at,en);let id=take(&b,&mut at,idn);let wire=take(&b,&mut at,wn);let got=EventEnvelope::decode(wire);assert_eq!(got.is_ok(),ok,"actual native verdict {name}");
  if let Ok(value)=got{assert_eq!(value.data.as_bytes(),d,"data {name}");assert_eq!(value.event.as_bytes(),e,"event {name}");assert_eq!(value.id.as_bytes(),id,"id {name}");assert_eq!(value.retry,has.then_some(retry));let encoded=value.encode();assert_eq!(encoded.is_ok(),enc,"actual native encode {name}");if let Ok(encoded)=encoded{if canonical{assert_eq!(encoded,wire,"default allocator bytes {name}");}let again=EventEnvelope::decode(&encoded).unwrap();assert!(again==value,"actual native semantic roundtrip {name}");}else{limits+=1;}accept+=1;}else{println!("REJECTION_REASON {name} {:?} wire_bytes={wn}",got.err().unwrap());reject+=1;}
  #[cfg(sse_sdk_candidate)]{
   let mut sdk_input=wire.to_vec();let result=sdk::Event::decode(&sdk_input);assert_eq!(result.is_ok(),ok,"Rust SDK verdict {name}");sdk_input.fill(0xcc);if let Ok(event)=result{assert_eq!(event.data.as_bytes(),d);assert_eq!(event.event.as_bytes(),e);assert_eq!(event.id.as_bytes(),id);assert_eq!(event.retry,has.then_some(retry));let encoded=event.encode();assert_eq!(encoded.is_ok(),enc);if let Ok(encoded)=encoded{let native=EventEnvelope{data:event.data.clone(),event:event.event.clone(),id:event.id.clone(),retry:event.retry};assert_eq!(encoded,native.encode().unwrap(),"Rust/native actual encoder bytes {name}");assert!(EventEnvelope::decode(&encoded).unwrap()==native);}}
  }
  println!("VECTOR {name} {}",if ok{"ACCEPT"}else{"REJECT"});
 }
 assert_eq!(at,b.len());println!("METHOD actual_native_sse_corpus_and_encode_bounds PASS vectors={count} accept={accept} reject={reject} encode_limits={limits}");if canonical{println!("METHOD actual_native_sse_sdk_wire_equal_default_allocator PASS vectors={count}");}
 let full=EventEnvelope{data:"x".repeat(65536),event:String::new(),id:String::new(),retry:Some(u64::MAX)};assert_eq!(full.encode().err(),Some(Error::Limit));let aggregate=EventEnvelope{data:"x".repeat(32768),event:"x".into(),id:"x".repeat(32768),retry:None};assert_eq!(aggregate.encode().err(),Some(Error::Limit));println!("METHOD actual_native_sse_aggregate_and_serialized_limits PASS values=2");
 #[cfg(sse_sdk_candidate)]{
  let owned=sdk::Event{data:"x".repeat(32768),event:"x".into(),id:"x".repeat(32768),retry:None};let view=owned.as_ref();assert_eq!(view.validate(),Err(sdk::Error::Limit));assert_eq!(view.encode(),Err(sdk::Error::Limit));assert_eq!(owned.validate(),Err(sdk::Error::Limit));assert_eq!(owned.encode(),Err(sdk::Error::Limit));let full="x".repeat(65536);let view=sdk::EventRef{data:&full,event:"",id:"",retry:Some(u64::MAX)};assert_eq!(view.validate(),Ok(()));assert_eq!(view.encode(),Err(sdk::Error::Limit));println!("METHOD rust_sse_corpus_matches_actual_native_semantics_and_encoder PASS vectors={count}");println!("METHOD rust_sse_owned_borrowed_aggregate_before_encode_and_wire_limit PASS values=2");
 }
}
