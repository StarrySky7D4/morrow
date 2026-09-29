"""Single-authority schema and owned codec generation for M03, only new paths."""
from pathlib import Path
R=Path(__file__).resolve().parents[1];D=R/'contracts/experimental/agent_host_v3_http_stream';assert not D.exists();D.mkdir();(D/'src').mkdir();(D/'examples').mkdir()
# One metadata table drives schema declarations and checked owned Rust adapters.
enums={
'Kind':['challenge','hello','welcome','query','state','denied','stop','close','dataOffer','dataBind','dataBound','httpPrepare','requestChunk','httpProposed','httpApproved','httpCommit','responseHead','bodyChunk','httpCredit','creditState','httpCancel','cancelAccepted','httpTerminal','released'],
'IntentPhase':['absent','prepared','unknown','observed','cancelledBeforeDispatch'],
'NetworkPhase':['idle','awaitingHead','streaming','eof','failed','cancelled'],
'OwnerPhase':['preparing','active','revoked','closing','closingUnconfirmed','released'],
}
structs={
'Channel':[('locator','Text'),('nonce','Data'),('maxChunkBytes','UInt32'),('creditLimit','UInt32')],
'Header':[('name','Text'),('value','Data')],
'Prepare':[('method','Text'),('absoluteTarget','Text'),('headers','List(Header)'),('bodyBytes','UInt32'),('bodySha256','Data')],
'Chunk':[('offset','UInt64'),('bytes','Data')],
'Decision':[('proposalRef','Data'),('bodySha256','Data'),('requestSha256','Data'),('httpGrantRef','Data'),('endpointRef','Data'),('bodyBytes','UInt32'),('sendBudget','UInt32')],
'Head':[('status','UInt16'),('headers','List(Header)'),('remoteAddress','Text')],
'Credit':[('consumedOffset','UInt64'),('parserYieldedBytes','UInt64'),('drainDiscardedBytes','UInt64'),('cancelDiscardedBytes','UInt64'),('windowBytes','UInt32'),('maxChunkBytes','UInt32')],
'Progress':[('intent','IntentPhase'),('network','NetworkPhase'),('owner','OwnerPhase'),('httpStatus','UInt16'),('errorCode','UInt32'),('receivedOffset','UInt64'),('reservedOffset','UInt64'),('issuedOffset','UInt64'),('osCompletedOffset','UInt64'),('peerConsumedOffset','UInt64'),('parserYieldedBytes','UInt64'),('drainDiscardedBytes','UInt64'),('cancelDiscardedBytes','UInt64'),('lastWriteOrdinal','UInt64'),('revokePersisted','Bool'),('revokeApplied','Bool'),('httpEof','Bool'),('responseMaterialStored','Bool'),('workerJoined','Bool'),('connectReaped','Bool'),('readReaped','Bool'),('writeReaped','Bool'),('dataClosed','Bool'),('childExited','Bool'),('stdoutEof','Bool'),('stderrEof','Bool'),('ownerReleased','Bool')],
}
variants=[('none','Void'),('channel','Channel'),('prepare','Prepare'),('chunk','Chunk'),('decision','Decision'),('head','Head'),('credit','Credit'),('progress','Progress')]
base=[('major','UInt16'),('revision','UInt16'),('kind','Kind'),('sequence','UInt64'),('session','UInt64'),('instanceEpoch','UInt64'),('revocationGeneration','UInt64'),('childPid','UInt32'),('code','UInt32'),('remainingMs','UInt64'),('nonce','Data'),('schemaSha256','Data'),('artifactSha256','Data'),('executionConfigSha256','Data'),('requestBudget','UInt64'),('capabilities','UInt64'),('reserved','UInt64'),('operationId','Data'),('attempt','UInt64')]
schema='''@0xba19abb12fecd3fd;
# Sole M03 native HTTP stream authority, major3 revision1.
# uint32 LE payload byte length + one unpacked Capnp message; 8..32768 aligned8.
# See README.md for direction, identity, approval, credit and orthogonal completion rules.
'''
for name,values in enums.items():schema+='enum '+name+' {\n'+''.join(f'  {v} @{i};\n' for i,v in enumerate(values))+'}\n'
for name,fields in structs.items():schema+='struct '+name+' {\n'+''.join(f'  {n} @{i} :{t};\n' for i,(n,t) in enumerate(fields))+'}\n'
schema+='struct Frame {\n'+''.join(f'  {n} @{i} :{t};\n' for i,(n,t) in enumerate(base))+'  payload :union {\n'+''.join(f'    {n} @{i+len(base)} :{t};\n' for i,(n,t) in enumerate(variants))+'  }\n}\n'
(D/'native_http.capnp').write_text(schema)
(D/'Cargo.toml').write_text('''[workspace]
resolver = "2"
[package]
name = "morrow-native-http-stream-wire"
version = "0.3.0-experimental.1"
edition = "2024"
publish = false
license = "AGPL-3.0-only"
[dependencies]
sha2 = "=0.10.9"
capnp = "=0.24.1"
[build-dependencies]
capnpc = "=0.24.0"
''')
(D/'build.rs').write_text('''fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=native_http.capnp");
    capnpc::CompilerCommand::new().file("native_http.capnp").run()?;
    Ok(())
}
''')
def snake(s):return ''.join('_'+c.lower() if c.isupper() else c for c in s)
def title(s):return s[0].upper()+s[1:]
types={'Data':'Vec<u8>','Text':'String','UInt16':'u16','UInt32':'u32','UInt64':'u64','Bool':'bool','List(Header)':'Vec<Header>'}
def rt(t):return types.get(t,t)
def getter(n,t,obj='r'):
    x=f'{obj}.get_{snake(n)}()'
    if t=='Text':return x+'.map_err(|_| "text pointer")?.to_str().map_err(|_| "UTF8")?.to_owned()'
    if t=='Data':return x+'.map_err(|_| "data pointer")?.to_vec()'
    if t in enums:return x+'.map_err(|_| "enum")?'
    if t=='List(Header)':return '{ let list='+x+'.map_err(|_| "headers")?; if list.len()>32 { return Err("header count"); } list.iter().map(Header::read).collect::<Result<Vec<_>>>()? }'
    return x
def setter(n,t,obj='b',value=None):
    v=value or 'self.'+snake(n)
    if t=='List(Header)':return '{ let mut list='+obj+'.reborrow().init_'+snake(n)+'('+v+'.len() as u32); for (i,value) in '+v+'.iter().enumerate() { value.write(list.reborrow().get(i as u32)); } }'
    ref='&' if t in ['Data','Text'] else ''
    return obj+'.set_'+snake(n)+'('+ref+v+');'
lib='''#![deny(unsafe_code)]
//! Owned bounded codec. Decoding never grants authority or advances an attempt.
use capnp::{message, serialize};
use sha2::{Digest,Sha256};
#[allow(unsafe_code,clippy::all)]
pub mod native_http_capnp { include!(concat!(env!("OUT_DIR"),"/native_http_capnp.rs")); }
pub use native_http_capnp::{Kind,IntentPhase,NetworkPhase,OwnerPhase};
pub type Result<T> = std::result::Result<T,&'static str>;
pub const MAX_PAYLOAD:usize=32768;
pub const MAX_FRAME:usize=MAX_PAYLOAD+4;
pub const MAX_BODY:usize=32768;
pub const MAX_RESPONSE:usize=65536;
pub const MAX_CHUNK:usize=8192;
pub const MAX_CREDIT:u32=16384;
pub const SCHEMA:&[u8]=include_bytes!("../native_http.capnp");
pub fn digest(bytes:&[u8])->[u8;32] { Sha256::digest(bytes).into() }
pub fn schema_digest()->[u8;32] { digest(SCHEMA) }
pub fn hex(bytes:&[u8])->String { bytes.iter().map(|b|format!("{b:02x}")).collect() }
pub fn payload_length(prefix:&[u8])->Result<usize> { let p:[u8;4]=prefix.try_into().map_err(|_|"prefix")?; let n=u32::from_le_bytes(p) as usize; if !(8..=MAX_PAYLOAD).contains(&n)||n%8!=0{return Err("payload limit/alignment");} Ok(n) }
'''
for name,fields in structs.items():
    lib+='#[derive(Clone,Debug,PartialEq)]\npub struct '+name+' {\n'+''.join(f'    pub {snake(n)}: {rt(t)},\n' for n,t in fields)+'}\n'
    lib+='impl '+name+' {\n fn write(&self, mut b:native_http_capnp::'+snake(name).lstrip('_')+'::Builder<\'_>) {\n'+''.join(setter(n,t)+'\n' for n,t in fields)+'}\n'
    lib+=' fn read(r:native_http_capnp::'+snake(name).lstrip('_')+'::Reader<\'_>)->Result<Self>{ Ok(Self {\n'+''.join(f'{snake(n)}:{getter(n,t)},\n' for n,t in fields)+'})}\n}\n'
lib+='#[derive(Clone,Debug,PartialEq)]\npub enum Payload { None, '+', '.join(title(n)+'('+t+')' for n,t in variants[1:])+' }\n'
owned=[(n,t) for n,t in base if n not in ['major','revision','reserved']]
lib+='#[derive(Clone,Debug,PartialEq)]\npub struct Frame {\n'+''.join(f'pub {snake(n)}:{rt(t)},\n' for n,t in owned)+'pub payload:Payload,\n}\n'
lib+='''impl Frame {
pub fn encode(&self)->Result<Vec<u8>> {
 self.validate()?;
 let mut msg=message::Builder::new_default();
 {let mut b=msg.init_root::<native_http_capnp::frame::Builder>();
 b.set_major(3);b.set_revision(1);b.set_reserved(0);
'''+''.join(setter(n,t)+'\n' for n,t in owned)+'''
 let mut p=b.init_payload();
 match &self.payload { Payload::None=>p.set_none(()),
'''+''.join(f'Payload::{title(n)}(value)=>value.write(p.init_{n}()),\n' for n,t in variants[1:])+''' }
 }
 let bytes=serialize::write_message_to_words(&msg); if bytes.len()>MAX_PAYLOAD{return Err("encoded limit");}
 let mut frame=(bytes.len() as u32).to_le_bytes().to_vec();frame.extend(bytes);Ok(frame)
}
pub fn decode(bytes:&[u8])->Result<Self> {
 if bytes.len()<4{return Err("short");}let n=payload_length(&bytes[..4])?;if bytes.len()!=n+4{return Err("frame length");}
 let mut cursor=std::io::Cursor::new(&bytes[4..]);let mut opts=message::ReaderOptions::new();opts.traversal_limit_in_words(Some(16384));opts.nesting_limit(8);
 let msg=serialize::read_message(&mut cursor,opts).map_err(|_|"Capnp decode")?;if cursor.position() as usize!=n{return Err("trailing message");}
 let r=msg.get_root::<native_http_capnp::frame::Reader>().map_err(|_|"root")?;
 if r.get_major()!=3||r.get_revision()!=1||r.get_reserved()!=0{return Err("version/reserved");}
 use native_http_capnp::frame::payload::Which;
 let payload=match r.get_payload().which().map_err(|_|"union")? {
 Which::None(())=>Payload::None,
'''+''.join(f'Which::{title(n)}(value)=>Payload::{title(n)}({t}::read(value.map_err(|_|"payload pointer")?)?),\n' for n,t in variants[1:])+''' };
 let value=Self {
'''+''.join(f'{snake(n)}:{getter(n,t)},\n' for n,t in owned)+'''payload}; value.validate()?;Ok(value)
}
}
'''
(D/'src/lib.rs').write_text(lib)
print(D)
