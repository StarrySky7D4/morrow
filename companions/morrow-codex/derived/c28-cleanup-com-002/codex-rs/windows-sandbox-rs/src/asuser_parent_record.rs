//! Source047 P->C record DATA, never an origin capability or an EOF proof.
//! Shared source with the original existing native parent. Independent of A0,
//! A1 C/S and runner wires. No commands, paths, tokens, credentials or nonces.
use std::io;
pub(crate) const BYTES:usize=2048;
pub(crate) const MAX_FILES:usize=24;
const MAGIC:[u8;8]=*b"C28P1V1\0";
fn bad()->io::Error {io::Error::new(io::ErrorKind::InvalidData,"A1 parent fixed record rejected")}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct FileIdentity {
    pub alias:u64,pub volume:u64,pub id:[u8;16],pub attributes:u32,
    pub kind:u8,pub length:u64,pub creation:u64,pub modified:u64,
}
impl FileIdentity {
    pub(crate) const EMPTY:Self=Self{alias:0,volume:0,id:[0;16],attributes:0,kind:0,length:0,creation:0,modified:0};
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct Record {
    pub generation:u64,pub parent_pid:u32,pub parent_time:u64,
    pub child_pid:u32,pub child_time:u64,pub deadline:u64,
    pub parent_alias:u64,pub self_alias:u64,pub reader_alias:u64,
    pub parent_image:[u8;32],pub child_image:[u8;32],
    pub files:[FileIdentity;MAX_FILES],pub count:u8,
}
fn put(b:&mut[u8],o:usize,v:u64){b[o..o+8].copy_from_slice(&v.to_le_bytes());}
fn get(b:&[u8],o:usize)->u64{u64::from_le_bytes(b[o..o+8].try_into().unwrap())}
fn handle(v:u64)->bool{v!=0&&v<=isize::MAX as u64}
impl Record {
    pub(crate) fn encode(self)->io::Result<[u8;BYTES]> {
        let count=usize::from(self.count);
        if count<2||count>MAX_FILES||self.generation==0||self.parent_pid==0||self.child_pid==0
            ||self.parent_pid==self.child_pid||self.parent_time==0||self.child_time==0||self.deadline==0
            ||self.parent_image==[0;32]||self.child_image==[0;32]||self.parent_image==self.child_image
            ||![self.parent_alias,self.self_alias,self.reader_alias].into_iter().all(handle)
            ||self.parent_alias==self.self_alias||self.parent_alias==self.reader_alias||self.self_alias==self.reader_alias {
            return Err(bad());
        }
        let mut out=[0;BYTES];out[..8].copy_from_slice(&MAGIC);out[8]=1;out[9]=self.count;
        put(&mut out,16,self.generation);put(&mut out,24,u64::from(self.parent_pid));
        put(&mut out,32,self.parent_time);put(&mut out,40,u64::from(self.child_pid));put(&mut out,48,self.child_time);
        put(&mut out,56,self.deadline);put(&mut out,64,self.parent_alias);put(&mut out,72,self.self_alias);
        put(&mut out,80,self.reader_alias);out[96..128].copy_from_slice(&self.parent_image);
        out[128..160].copy_from_slice(&self.child_image);
        let mut child=0;let mut parent=0;
        for (i,file) in self.files.iter().enumerate() {
            if i>=count {if *file!=FileIdentity::EMPTY{return Err(bad());}continue;}
            if !handle(file.alias)||[self.parent_alias,self.self_alias,self.reader_alias].contains(&file.alias)
                ||self.files[..i].iter().any(|v|v.alias==file.alias||v.volume==file.volume&&v.id==file.id)
                ||file.id==[0;16]||file.attributes&0x400!=0||file.creation==0||file.modified==0
                ||!matches!(file.kind,1..=5)||(file.attributes&0x10!=0)!=matches!(file.kind,2|4|5) {
                return Err(bad());
            }
            if file.kind==1{child+=1;}if file.kind==3{parent+=1;}
            if matches!(file.kind,1|3)&&(file.length==0||file.length>512*1024*1024){return Err(bad());}
            let offset=192+i*64;
            put(&mut out,offset,file.alias);put(&mut out,offset+8,file.volume);
            out[offset+16..offset+32].copy_from_slice(&file.id);
            out[offset+32..offset+36].copy_from_slice(&file.attributes.to_le_bytes());out[offset+36]=file.kind;
            put(&mut out,offset+40,file.length);put(&mut out,offset+48,file.creation);put(&mut out,offset+56,file.modified);
        }
        // The exact path ancestry is validated against these ORIGINAL aliases by
        // the native consumer. Count/IDs alone do not establish that relation.
        if child!=1||parent!=1{return Err(bad());}
        Ok(out)
    }
    pub(crate) fn decode(bytes:&[u8])->io::Result<Self> {
        if bytes.len()!=BYTES||bytes[..8]!=MAGIC||bytes[8]!=1||bytes[10..16].iter().any(|v|*v!=0)
            ||bytes[88..96].iter().chain(&bytes[160..192]).any(|v|*v!=0){return Err(bad());}
        let count=bytes[9];if count as usize>MAX_FILES{return Err(bad());}
        let mut r=Self{generation:get(bytes,16),parent_pid:get(bytes,24).try_into().map_err(|_|bad())?,
            parent_time:get(bytes,32),child_pid:get(bytes,40).try_into().map_err(|_|bad())?,child_time:get(bytes,48),
            deadline:get(bytes,56),parent_alias:get(bytes,64),self_alias:get(bytes,72),reader_alias:get(bytes,80),
            parent_image:bytes[96..128].try_into().unwrap(),child_image:bytes[128..160].try_into().unwrap(),
            files:[FileIdentity::EMPTY;MAX_FILES],count};
        for i in 0..usize::from(count) {
            let o=192+i*64;
            r.files[i]=FileIdentity{alias:get(bytes,o),volume:get(bytes,o+8),id:bytes[o+16..o+32].try_into().unwrap(),
                attributes:u32::from_le_bytes(bytes[o+32..o+36].try_into().unwrap()),kind:bytes[o+36],
                length:get(bytes,o+40),creation:get(bytes,o+48),modified:get(bytes,o+56)};
        }
        if r.encode()?.as_slice()!=bytes{return Err(bad());}Ok(r)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample()->Record {
        let f=|alias,kind,id,attributes|FileIdentity{alias,kind,id:[id;16],volume:1,attributes,length:64,creation:1,modified:2};
        let mut files=[FileIdentity::EMPTY;MAX_FILES];files[0]=f(4,1,1,0);files[1]=f(8,3,2,0);
        Record{generation:1,parent_pid:4,parent_time:10,child_pid:8,child_time:11,deadline:12,
            parent_alias:16,self_alias:20,reader_alias:24,parent_image:[1;32],child_image:[2;32],files,count:2}
    }
    #[test] fn parent_data_rejects_cross_wire_padding_alias_reuse_and_extra_privileged_handles() {
        let r=sample();let b=r.encode().unwrap();assert_eq!(Record::decode(&b).unwrap(),r);
        for i in [0,8,10,88,160,192+37,192+2*64,2047] {
            let mut bad=b;bad[i]^=1;assert!(Record::decode(&bad).is_err());
        }
        let mut bad=r;bad.files[1].alias=bad.self_alias;assert!(bad.encode().is_err());
        let mut bad=r;bad.files[1].id=bad.files[0].id;assert!(bad.encode().is_err());
        let mut bad=r;bad.files[2]=FileIdentity{alias:100,kind:6,..bad.files[0]};bad.count=3;
        assert!(bad.encode().is_err());
    }
    #[test] fn valid_parent_record_is_still_only_data_not_a_native_creation_proof() {
        let b=sample().encode().unwrap();assert!(Record::decode(&b).is_ok());
        let mut r=sample();r.child_time+=1;assert!(r.encode().is_ok());
        // There is intentionally no Record -> PrecreationOrigin conversion.
        assert!(Record::decode(&b[..512]).is_err());
    }
}

