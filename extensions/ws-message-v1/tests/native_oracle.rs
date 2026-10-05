// Actual existing native codec oracle; no second Capnp decoder.
use morrow_network_node_stream::websocket::{Message,MessageKind};
use std::{env,fs};
fn u32at(b:&[u8],at:&mut usize)->u32 { let v=u32::from_le_bytes(b[*at..*at+4].try_into().unwrap());*at+=4;v }
fn kind(v:u32)->MessageKind {match v{0=>MessageKind::Text,1=>MessageKind::Binary,2=>MessageKind::Ping,3=>MessageKind::Pong,4=>MessageKind::Close,_=>panic!("invalid corpus kind")}}
fn value(k:MessageKind)->u32 {match k{MessageKind::Text=>0,MessageKind::Binary=>1,MessageKind::Ping=>2,MessageKind::Pong=>3,MessageKind::Close=>4}}
fn main(){
 let args:Vec<_>=env::args().collect();assert!(args.len()==2||args.len()==3);let canonical=args.get(2).is_some_and(|s|s=="--canonical");
 let b=fs::read(&args[1]).unwrap();assert_eq!(&b[..8],b"WSVC0002");let mut at=8;let count=u32at(&b,&mut at);let(mut yes,mut no,mut encode_limits)=(0,0,0);
 for _ in 0..count {
  let n=u32at(&b,&mut at)as usize;let ok=u32at(&b,&mut at)!=0;let enc=u32at(&b,&mut at)!=0;let k=u32at(&b,&mut at);let has=u32at(&b,&mut at)!=0;let code=u32at(&b,&mut at)as u16;let pn=u32at(&b,&mut at)as usize;let wn=u32at(&b,&mut at)as usize;
  let name=std::str::from_utf8(&b[at..at+n]).unwrap();at+=n;let payload=&b[at..at+pn];at+=pn;let wire=&b[at..at+wn];at+=wn;
  let got=Message::decode(wire);assert_eq!(got.is_ok(),ok,"native verdict {name}");
  if let Ok(msg)=got{assert_eq!(value(msg.kind),k,"kind {name}");assert_eq!(&msg.payload,payload,"payload {name}");assert_eq!(msg.close_code,has.then_some(code),"code {name}");let encoded=msg.encode();assert_eq!(encoded.is_ok(),enc,"native encode bound {name}");if let Ok(encoded)=encoded{if canonical{assert_eq!(&encoded,wire,"actual native default allocator bytes {name}");}let again=Message::decode(&encoded).unwrap();assert_eq!(again.kind,msg.kind);assert_eq!(again.payload,msg.payload);assert_eq!(again.close_code,msg.close_code);}else{encode_limits+=1;}yes+=1;}else{no+=1;}
  println!("VECTOR {} {}",name,if ok{"ACCEPT"}else{"REJECT"});
 }
 assert_eq!(at,b.len());println!("METHOD actual_native_corpus_and_roundtrips PASS vectors={count} accept={yes} reject={no} encode_limits={encode_limits}");
 if canonical{println!("METHOD actual_native_sdk_encoded_bytes_equal_default_allocator PASS vectors={count}");}
 let mut allowed=0;
 for code in 0..=u16::MAX {let msg=Message{kind:kind(4),payload:vec![],close_code:Some(code)};let expected=(1000..=1003).contains(&code)||(1007..=1013).contains(&code)||(3000..=4999).contains(&code);assert_eq!(msg.validate(65536).is_ok(),expected,"native closecode {code}");if expected{allowed+=1;}}
 println!("METHOD actual_native_close_code_exhaustive PASS values=65536 allowed={allowed}");
}
