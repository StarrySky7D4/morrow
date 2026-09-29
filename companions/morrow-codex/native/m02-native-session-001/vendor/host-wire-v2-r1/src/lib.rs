#![forbid(unsafe_code)]
use sha2::{Digest, Sha256};
pub const SIZE: usize = 256;
pub const SCHEMA: &[u8] = include_bytes!("../native_session_wire.json");
pub fn digest(bytes: &[u8]) -> [u8; 32] { Sha256::digest(bytes).into() }
pub fn schema_digest() -> [u8; 32] { digest(SCHEMA) }
pub fn hex(bytes: &[u8]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Challenge=1, Hello=2, Welcome=3, Query=4, State=5, Denied=6, Stop=7, Close=8 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: Kind, pub sequence: u64, pub session: u64, pub epoch: u64,
    pub generation: u64, pub pid: u32, pub code: u32, pub remaining_ms: u64,
    pub nonce: [u8;32], pub schema: [u8;32], pub artifact: [u8;32],
    pub config: [u8;32], pub budget: u64, pub capabilities: u64,
}
impl Frame {
    pub fn request(&self, kind: Kind, sequence: u64) -> Self {
        let mut f=self.clone(); f.kind=kind; f.sequence=sequence; f.code=0; f
    }
    pub fn encode(&self) -> [u8;SIZE] {
        let mut b=[0;SIZE]; b[..8].copy_from_slice(b"MRWNSV02");
        b[8..10].copy_from_slice(&2u16.to_le_bytes()); b[10..12].copy_from_slice(&(self.kind as u16).to_le_bytes());
        b[12..16].copy_from_slice(&(SIZE as u32).to_le_bytes());
        for (o,v) in [(16,self.sequence),(24,self.session),(32,self.epoch),(40,self.generation),(56,self.remaining_ms),(192,self.budget),(200,self.capabilities)] { b[o..o+8].copy_from_slice(&v.to_le_bytes()); }
        b[48..52].copy_from_slice(&self.pid.to_le_bytes()); b[52..56].copy_from_slice(&self.code.to_le_bytes());
        for (o,v) in [(64,&self.nonce),(96,&self.schema),(128,&self.artifact),(160,&self.config)] { b[o..o+32].copy_from_slice(v); }
        b
    }
    pub fn decode(b: &[u8]) -> Result<Self, &'static str> {
        if b.len()!=SIZE || &b[..8]!=b"MRWNSV02" || b[8..10]!=2u16.to_le_bytes() || b[12..16]!=(SIZE as u32).to_le_bytes() || b[208..].iter().any(|v|*v!=0) { return Err("invalid frame header/size/reserved"); }
        let kind=match u16::from_le_bytes(b[10..12].try_into().unwrap()) {1=>Kind::Challenge,2=>Kind::Hello,3=>Kind::Welcome,4=>Kind::Query,5=>Kind::State,6=>Kind::Denied,7=>Kind::Stop,8=>Kind::Close,_=>return Err("unknown kind")};
        let u=|o|u64::from_le_bytes(b[o..o+8].try_into().unwrap());
        Ok(Self {kind,sequence:u(16),session:u(24),epoch:u(32),generation:u(40),pid:u32::from_le_bytes(b[48..52].try_into().unwrap()),code:u32::from_le_bytes(b[52..56].try_into().unwrap()),remaining_ms:u(56),nonce:b[64..96].try_into().unwrap(),schema:b[96..128].try_into().unwrap(),artifact:b[128..160].try_into().unwrap(),config:b[160..192].try_into().unwrap(),budget:u(192),capabilities:u(200)})
    }
    pub fn matches_admission(&self, initial: &Self) -> bool {
        self.request(initial.kind,initial.sequence)==initial.clone().request(initial.kind,initial.sequence)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn rejects_bad_headers_and_all_truncations() {
        let f=Frame {kind:Kind::Challenge,sequence:0,session:1,epoch:2,generation:1,pid:3,code:0,remaining_ms:1000,nonce:[4;32],schema:schema_digest(),artifact:[5;32],config:[6;32],budget:16,capabilities:1};
        let b=f.encode(); assert_eq!(Frame::decode(&b).unwrap(),f);
        for n in 0..SIZE {assert!(Frame::decode(&b[..n]).is_err());}
        for offset in [0,8,10,12,208,255] {let mut bad=b; bad[offset]=255;assert!(Frame::decode(&bad).is_err());}
        let mut r=f.request(Kind::Hello,1); assert!(r.matches_admission(&f)); r.epoch+=1; assert!(!r.matches_admission(&f));
    }
}
