//! Strict, bounded self-relative security descriptor decoding for A1.
//! No allocator/WinAPI calls, permissive ACL normalization, owner repair or
//! treatment of an absent/null ACL as a proof. Readback must be from originals.
use std::io;
use super::asuser_a1_policy::{Sid,PROCESS_MANAGEMENT,THREAD_MANAGEMENT,
    PROCESS_TARGET,THREAD_TARGET,OWNER_READ_CONTROL};

fn bad()->io::Error {io::Error::new(io::ErrorKind::PermissionDenied,"A1 descriptor policy rejected")}
fn u16_at(b:&[u8],o:usize)->io::Result<u16> {
    Ok(u16::from_le_bytes(b.get(o..o+2).ok_or_else(bad)?.try_into().map_err(|_|bad())?))
}
fn u32_at(b:&[u8],o:usize)->io::Result<u32> {
    Ok(u32::from_le_bytes(b.get(o..o+4).ok_or_else(bad)?.try_into().map_err(|_|bad())?))
}
fn sid_at(b:&[u8],offset:usize)->io::Result<(Sid,usize)> {
    if offset<20||offset%4!=0{return Err(bad());}
    let head=b.get(offset..offset+8).ok_or_else(bad)?;
    let length=8+usize::from(head[1])*4;
    let bytes=b.get(offset..offset.checked_add(length).ok_or_else(bad)?).ok_or_else(bad)?;
    Ok((Sid::from_bytes(bytes)?,length))
}
struct Header {owner:Sid,group:Option<Sid>,dacl:usize,sacl:usize}
fn header(bytes:&[u8])->io::Result<Header> {
    if bytes.len()<20||bytes.len()>65536||bytes[0]!=1||bytes[1]!=0{return Err(bad());}
    let control=u16_at(bytes,2)?;
    // self-relative + DACL present + protected. A system-selected default owner
    // is legal; the exact OWNER_RIGHTS rule below does not depend on its SID.
    // A defaulted DACL is not the supplied protected creation-time policy.
    if control&0x9004!=0x9004||control&0x0008!=0{return Err(bad());}
    let owner_offset=u32_at(bytes,4)? as usize;
    let group_offset=u32_at(bytes,8)? as usize;
    let sacl=u32_at(bytes,12)? as usize;let dacl=u32_at(bytes,16)? as usize;
    let (owner,owner_len)=sid_at(bytes,owner_offset)?;
    let group=if group_offset!=0 {Some(sid_at(bytes,group_offset)?.0)}else{None};
    if dacl<20||dacl%4!=0||dacl<owner_offset+owner_len&&dacl>=owner_offset{return Err(bad());}
    if control&0x10==0&&sacl!=0{return Err(bad());}
    Ok(Header{owner,group,dacl,sacl})
}
fn acl<'a>(b:&'a [u8],offset:usize)->io::Result<(&'a [u8],u16)> {
    if offset<20||offset%4!=0{return Err(bad());}
    let head=b.get(offset..offset+8).ok_or_else(bad)?;
    if head[0]!=2||head[1]!=0||head[6]!=0||head[7]!=0{return Err(bad());}
    let size=usize::from(u16_at(head,2)?);
    if size<8||size%4!=0{return Err(bad());}
    Ok((b.get(offset..offset.checked_add(size).ok_or_else(bad)?).ok_or_else(bad)?,u16_at(head,4)?))
}
pub(crate) struct CreationReadback {pub owner:Sid,pub integrity_rid:u32}
pub(crate) struct TokenLayout {pub owner:Sid,pub integrity_rid:u32,pub explicit_label:bool}
pub(crate) fn default_layout(bytes:&[u8])->io::Result<()> {
    if bytes.len()<8||bytes.len()>4096||bytes[0]!=2||bytes[1]!=0||bytes[6]!=0||bytes[7]!=0
        ||u16_at(bytes,2)? as usize!=bytes.len(){return Err(bad());}
    let count=u16_at(bytes,4)?;if count>64{return Err(bad());}let mut offset=8;
    for _ in 0..count {
        let size=u16_at(bytes,offset+2)? as usize;if size<16||size%4!=0{return Err(bad());}
        let ace=bytes.get(offset..offset.checked_add(size).ok_or_else(bad)?).ok_or_else(bad)?;
        if !matches!(ace[0],0|1){return Err(bad());}Sid::from_bytes(&ace[8..])?;offset+=size;
    }
    if offset!=bytes.len(){return Err(bad());}Ok(())
}
/// TokenDefaultDacl is an object-creation TEMPLATE, not the token object's SD.
/// Addendum1625 permits one private-T initialization to this exact policy.
/// Both its zero-mutation branch and actual post-call readback accept only a
/// real bounded ACL whose EVERY allow is RC for C, this LogonSID or OWNER_RIGHTS.
/// OWNER_RIGHTS must occur: otherwise the system-selected owner could receive
/// implicit WRITE_DAC on later NULL-SA R threads. No guessed object-specific
/// masks (e.g. token QUERY == thread GET_CONTEXT numerically) are safe here.
pub(crate) fn default_objects(acl_bytes:&[u8],caller:Sid,logon:Sid)->io::Result<()> {
    if acl_bytes.len()<8||acl_bytes.len()>4096||acl_bytes[0]!=2||acl_bytes[1]!=0
        ||acl_bytes[6]!=0||acl_bytes[7]!=0||u16_at(acl_bytes,2)? as usize!=acl_bytes.len()
        ||!logon.is_logon()||caller.is_logon()||caller==logon{return Err(bad());}
    let count=u16_at(acl_bytes,4)?;if count!=3{return Err(bad());}
    let owner=Sid::from_bytes(&[1,1,0,0,0,0,0,3,4,0,0,0])?;
    let allowed=[caller,logon,owner];let mut seen=[false;3];let mut offset=8;
    for _ in 0..count {
        let size=u16_at(acl_bytes,offset+2)? as usize;
        if size<16||size%4!=0{return Err(bad());}
        let ace=acl_bytes.get(offset..offset.checked_add(size).ok_or_else(bad)?).ok_or_else(bad)?;
        if ace[0]!=0||ace[1]!=0||u32_at(ace,4)?!=OWNER_READ_CONTROL{return Err(bad());}
        let sid=Sid::from_bytes(&ace[8..])?;
        let index=allowed.iter().position(|v|*v==sid).ok_or_else(bad)?;
        if seen[index]{return Err(bad());}seen[index]=true;offset+=size;
    }
    if offset!=acl_bytes.len()||seen!=[true;3]{return Err(bad());}Ok(())
}
pub(crate) fn default_template(caller:Sid,logon:Sid)->io::Result<Vec<u8>> {
    if caller.is_logon()||!logon.is_logon()||caller==logon{return Err(bad());}
    let owner=Sid::from_bytes(&[1,1,0,0,0,0,0,3,4,0,0,0])?;
    let mut bytes=vec![2,0,0,0,3,0,0,0];
    for sid in [caller,logon,owner] {
        bytes.extend([0,0]);bytes.extend(((8+sid.bytes().len()) as u16).to_le_bytes());
        bytes.extend(OWNER_READ_CONTROL.to_le_bytes());bytes.extend(sid.bytes());
    }
    let length=bytes.len() as u16;bytes[2..4].copy_from_slice(&length.to_le_bytes());
    default_objects(&bytes,caller,logon)?;Ok(bytes)
}
/// Token-object layout capture is not process/token reachability authority.
/// The original token is unnamed and private; only the original pre-login C/S
/// reachability admission can establish that no hostile alias can reach it.
/// Reject an absent/null/unbounded DACL and malformed owner/ACE layout. The
/// token was created by LogonUser, so a defaulted DACL is captured as such; this
/// is not the explicit process/thread creation DACL rule below.
pub(crate) fn token_layout(bytes:&[u8])->io::Result<TokenLayout> {
    if bytes.len()<20||bytes.len()>65536||bytes[0]!=1||bytes[1]!=0{return Err(bad());}
    let control=u16_at(bytes,2)?;
    if control&0x8004!=0x8004{return Err(bad());}
    let (owner,_)=sid_at(bytes,u32_at(bytes,4)? as usize)?;
    let (acl,count)=acl(bytes,u32_at(bytes,16)? as usize)?;
    if count==0||count>64{return Err(bad());}
    let mut offset=8;
    for _ in 0..count {
        let size=usize::from(u16_at(acl,offset+2)?);
        let ace=acl.get(offset..offset.checked_add(size).ok_or_else(bad)?).ok_or_else(bad)?;
        if size<16||size%4!=0||!matches!(ace[0],0|1){return Err(bad());}
        Sid::from_bytes(&ace[8..])?;offset+=size;
    }
    if offset!=acl.len(){return Err(bad());}
    let sacl_offset=u32_at(bytes,12)? as usize;
    if sacl_offset==0 {
        // This is usable only after the caller's successful actual LABEL read,
        // never after access-denied, omitted query or fabricated empty buffer.
        // Win32 MIC documents unlabeled objects as medium. Keep the distinction.
        return Ok(TokenLayout{owner,integrity_rid:0x2000,explicit_label:false});
    }
    if control&0x10==0{return Err(bad());}
    let (sacl,count)=self::acl(bytes,sacl_offset)?;
    // A successful LABEL read can represent absence either by offset zero or
    // by an actual, well-formed empty ACL. Neither representation is an
    // explicit mandatory label; malformed/count-mismatched ACLs still fail.
    if count==0&&sacl.len()==8 {
        return Ok(TokenLayout{owner,integrity_rid:0x2000,explicit_label:false});
    }
    if count!=1||sacl.len()!=28||sacl[8]!=0x11||sacl[9]!=0||u16_at(sacl,10)?!=20
        ||u32_at(sacl,12)?&1==0||u32_at(sacl,12)?&!7!=0{return Err(bad());}
    let label=Sid::from_bytes(&sacl[16..])?;let label=label.bytes();
    if label.len()!=12||label[..8]!=[1,1,0,0,0,0,0,16]{return Err(bad());}
    Ok(TokenLayout{owner,integrity_rid:u32_at(label,8)?,explicit_label:true})
}
/// Descriptor supplied only at creation of the final private launch token.
/// No owner or SACL is supplied: the kernel selects them, and original-handle
/// readback checks the result before launch. In particular, supplying a SACL to
/// DuplicateTokenEx must not silently add ACCESS_SYSTEM_SECURITY to the handle.
pub(crate) fn launch_token_template(caller:Sid,logon:Sid)->io::Result<Vec<u8>> {
    use crate::asuser_a1_rights::{TOKEN_OBJECT_CREATOR,TOKEN_RUNNER_READ};
    if caller.is_logon()||!logon.is_logon()||caller==logon{return Err(bad());}
    let owner=Sid::from_bytes(&[1,1,0,0,0,0,0,3,4,0,0,0])?;
    let mut bytes=vec![1,0,4,0x90,0,0,0,0,0,0,0,0,0,0,0,0,20,0,0,0];
    let mut acl=vec![2,0,0,0,3,0,0,0];
    for (sid,mask) in [(caller,TOKEN_OBJECT_CREATOR),(logon,TOKEN_RUNNER_READ),(owner,OWNER_READ_CONTROL)] {
        acl.extend([0,0]);acl.extend(((8+sid.bytes().len()) as u16).to_le_bytes());
        acl.extend(mask.to_le_bytes());acl.extend(sid.bytes());
    }
    let length=acl.len() as u16;acl[2..4].copy_from_slice(&length.to_le_bytes());bytes.extend(acl);Ok(bytes)
}
/// Actual final token-object readback, independent of TokenDefaultDacl. The
/// protected three-ACE policy grants no token adjustment/assignment to R and
/// suppresses implicit owner WRITE_DAC. Reachability admission remains required.
pub(crate) fn launch_token(bytes:&[u8],caller:Sid,logon:Sid,minimum:u32,maximum:u32)->io::Result<TokenLayout> {
    use crate::asuser_a1_rights::{TOKEN_OBJECT_CREATOR,TOKEN_RUNNER_READ};
    let (h,_)=exact_dacl(bytes,caller,logon,TOKEN_OBJECT_CREATOR,TOKEN_RUNNER_READ)?;
    let layout=token_layout(bytes)?;
    if minimum>maximum||layout.integrity_rid<minimum||layout.integrity_rid>maximum{return Err(bad());}
    // A higher NO_READ_UP object would deny R's intended read-only open.
    // Default kernel object labels are captured, never lowered or repaired.
    if !matches!(effective_label(bytes,h.sacl)?,EffectiveLabel::AbsentMediumNoWriteUp|
        EffectiveLabel::Explicit{mask:1,..}){return Err(bad());}
    Ok(layout)
}
/// Every allow ACE must be one of the exact three creation-time ACEs. This
/// rules out generic rights, inherited/broad/stable-account write management.
/// No explicit owner is demanded; OWNER_RIGHTS suppression is owner-agnostic.
/// MIC is independent: an actual original LABEL_SECURITY_INFORMATION readback
/// must carry a single NO_WRITE_UP | NO_READ_UP mandatory label at the expected
/// integrity. Process/thread MIC is separate from the GUI object's NW policy.
pub(crate) fn creation(bytes:&[u8],caller:Sid,logon:Sid,thread:bool,integrity:u32)->io::Result<CreationReadback> {
    let management=if thread{THREAD_MANAGEMENT}else{PROCESS_MANAGEMENT};
    let target=if thread{THREAD_TARGET}else{PROCESS_TARGET};
    let (h,_)=exact_dacl(bytes,caller,logon,management,target)?;
    let label=effective_label(bytes,h.sacl)?;
    // Explicit creation label supplied for R process/thread; no process/thread
    // relabel is performed or permitted by the GUI exception.
    if label!=(EffectiveLabel::Explicit{rid:integrity,mask:3}){return Err(bad());}
    Ok(CreationReadback{owner:h.owner,integrity_rid:integrity})
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum EffectiveLabel {AbsentMediumNoWriteUp,Explicit{rid:u32,mask:u32}}
impl EffectiveLabel {
    pub(crate) fn medium(self)->bool {matches!(self,Self::AbsentMediumNoWriteUp|Self::Explicit{rid:0x2000,mask:1})}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct GuiReadback {
    pub owner:Sid,pub group:Option<Sid>,pub label:EffectiveLabel,
    dacl:[u8;32],
}
impl GuiReadback {
    pub(crate) fn digest(self)->[u8;32] {
        use sha2::{Digest,Sha256};
        let mut hash=Sha256::new();hash.update(b"C28A1-GUI-SECURITY-V1");
        hash.update((self.owner.bytes().len() as u32).to_le_bytes());hash.update(self.owner.bytes());
        match self.group {Some(g)=>{hash.update([1]);hash.update((g.bytes().len() as u32).to_le_bytes());hash.update(g.bytes());},
            None=>hash.update([0])}
        hash.update(self.dacl);
        match self.label {EffectiveLabel::AbsentMediumNoWriteUp=>hash.update([0]),
            EffectiveLabel::Explicit{rid,mask}=>{hash.update([1]);hash.update(rid.to_le_bytes());hash.update(mask.to_le_bytes());}}
        hash.finalize().into()
    }
    fn same_owner_dacl(self,other:Self)->bool {self.owner==other.owner&&self.group==other.group&&self.dacl==other.dacl}
}
fn exact_dacl(bytes:&[u8],caller:Sid,logon:Sid,management:u32,target:u32)->io::Result<(Header,[u8;32])> {
    use sha2::{Digest,Sha256};
    let h=header(bytes)?;
    if !logon.is_logon()||caller.is_logon()||caller==logon{return Err(bad());}
    let (dacl,count)=acl(bytes,h.dacl)?;if count!=3{return Err(bad());}
    let owner_rights=Sid::from_bytes(&[1,1,0,0,0,0,0,3,4,0,0,0])?;
    let expected=[(caller,management),(logon,target),(owner_rights,OWNER_READ_CONTROL)];
    let mut seen=[false;3];let mut offset=8;
    for _ in 0..count {
        let size=usize::from(u16_at(dacl,offset+2)?);
        if size<16||size%4!=0{return Err(bad());}
        let ace=dacl.get(offset..offset.checked_add(size).ok_or_else(bad)?).ok_or_else(bad)?;
        if ace[0]!=0||ace[1]!=0{return Err(bad());}
        let sid=Sid::from_bytes(&ace[8..])?;let mask=u32_at(ace,4)?;
        let index=expected.iter().position(|v|*v==(sid,mask)).ok_or_else(bad)?;
        if seen[index]{return Err(bad());}seen[index]=true;offset+=size;
    }
    if offset!=dacl.len()||seen!=[true;3]{return Err(bad());}
    Ok((h,Sha256::digest(dacl).into()))
}
fn effective_label(bytes:&[u8],offset:usize)->io::Result<EffectiveLabel> {
    // Successful native LABEL read is mandatory at call sites. Absence is an
    // explicit observed branch, with documented effective Medium/NW semantics.
    if offset==0{return Ok(EffectiveLabel::AbsentMediumNoWriteUp);}
    if u16_at(bytes,2)?&0x10==0{return Err(bad());}
    let (sacl,count)=acl(bytes,offset)?;
    if count==0&&sacl.len()==8{return Ok(EffectiveLabel::AbsentMediumNoWriteUp);}
    if count!=1||sacl.len()!=28||sacl[8]!=0x11||sacl[9]!=0||u16_at(sacl,10)?!=20{return Err(bad());}
    // Decode only the two explicit policies used here. The object-specific
    // caller still demands its exact mask; GUI label lowering remains NW-only.
    let mask=u32_at(sacl,12)?;
    if !matches!(mask,1|3){return Err(bad());}
    let sid=Sid::from_bytes(&sacl[16..])?;let sid=sid.bytes();
    if sid.len()!=12||sid[..8]!=[1,1,0,0,0,0,0,16]{return Err(bad());}
    Ok(EffectiveLabel::Explicit{rid:u32_at(sid,8)?,mask})
}
pub(crate) fn medium_sacl_offset(bytes:&[u8])->io::Result<usize> {
    if bytes.len()<20||bytes.len()>65536||bytes[0]!=1||u16_at(bytes,2)?&0x8010!=0x8010{return Err(bad());}
    let offset=u32_at(bytes,12)? as usize;
    if offset==0||effective_label(bytes,offset)?!=(EffectiveLabel::Explicit{rid:0x2000,mask:1}){return Err(bad());}
    Ok(offset)
}
pub(crate) fn gui_observed(bytes:&[u8],caller:Sid,logon:Sid,desktop:bool)->io::Result<GuiReadback> {
    use crate::asuser_a1_rights::*;
    let (h,dacl)=exact_dacl(bytes,caller,logon,if desktop{DESKTOP_MANAGEMENT}else{STATION_MANAGEMENT},
        if desktop{DESKTOP_TARGET}else{STATION_TARGET})?;
    if h.owner!=caller{return Err(bad());} // explicit actual S TokenUser, not TokenOwner group
    let label=effective_label(bytes,h.sacl)?;
    if !matches!(label,EffectiveLabel::AbsentMediumNoWriteUp|
        EffectiveLabel::Explicit{rid:0x2000|0x3000,mask:1}){return Err(bad());}
    Ok(GuiReadback{owner:h.owner,group:h.group,label,dacl})
}
pub(crate) fn gui(bytes:&[u8],caller:Sid,logon:Sid,desktop:bool,integrity:u32)->io::Result<GuiReadback> {
    let observed=gui_observed(bytes,caller,logon,desktop)?;
    if integrity!=0x2000||!observed.label.medium(){return Err(bad());}Ok(observed)
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum LabelPhase {Unobserved,NeedsLowering,InFlight,SetSucceeded,Verified,Unknown}
pub(crate) struct LabelAttempt {pub phase:LabelPhase,pub status:Option<u32>,before:Option<GuiReadback>}
impl LabelAttempt {
    pub(crate) const fn new()->Self {Self{phase:LabelPhase::Unobserved,status:None,before:None}}
    pub(crate) fn observe(&mut self,before:GuiReadback)->io::Result<bool> {
        if self.phase!=LabelPhase::Unobserved{return Err(bad());}
        self.before=Some(before);
        if before.label.medium(){self.phase=LabelPhase::Verified;return Ok(false);}
        if before.label==(EffectiveLabel::Explicit{rid:0x3000,mask:1}) {
            self.phase=LabelPhase::NeedsLowering;return Ok(true);
        }
        self.phase=LabelPhase::Unknown;Err(bad())
    }
    pub(crate) fn begin(&mut self)->io::Result<()> {
        if self.phase!=LabelPhase::NeedsLowering||self.status.is_some(){return Err(bad());}
        self.phase=LabelPhase::InFlight;Ok(())
    }
    pub(crate) fn returned(&mut self,status:u32)->io::Result<()> {
        if self.phase!=LabelPhase::InFlight||self.status.is_some(){return Err(bad());}
        self.status=Some(status);
        if status==0{self.phase=LabelPhase::SetSucceeded;Ok(())}
        else{self.phase=LabelPhase::Unknown;Err(io::Error::from_raw_os_error(status as i32))}
    }
    pub(crate) fn verify(&mut self,after:GuiReadback)->io::Result<()> {
        if self.phase!=LabelPhase::SetSucceeded||self.status!=Some(0)
            ||!self.before.is_some_and(|before|before.same_owner_dacl(after))
            ||after.label!=(EffectiveLabel::Explicit{rid:0x2000,mask:1}) {
            self.phase=LabelPhase::Unknown;return Err(bad());
        }
        self.phase=LabelPhase::Verified;Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sid(parts:&[u32])->Sid {let mut b=vec![1,parts.len() as u8,0,0,0,0,0,5];
        for p in parts{b.extend(p.to_le_bytes());}Sid::from_bytes(&b).unwrap()}
    fn descriptor()->(Vec<u8>,Sid,Sid) {
        let c=sid(&[21,1,2,3,1001]);let l=sid(&[5,7,8]);
        let owner=Sid::from_bytes(&[1,1,0,0,0,0,0,3,4,0,0,0]).unwrap();
        let mut b=vec![1,0,0x14,0x90];b.extend(20u32.to_le_bytes());b.extend(0u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());b.extend(0u32.to_le_bytes());b.extend(c.bytes());
        let d=b.len();b[16..20].copy_from_slice(&(d as u32).to_le_bytes());
        let mut a=vec![2,0,0,0,3,0,0,0];
        for (s,m) in [(c,PROCESS_MANAGEMENT),(l,PROCESS_TARGET),(owner,OWNER_READ_CONTROL)] {
            a.extend([0,0]);a.extend(((8+s.bytes().len()) as u16).to_le_bytes());
            a.extend(m.to_le_bytes());a.extend(s.bytes());
        }
        let len=a.len() as u16;a[2..4].copy_from_slice(&len.to_le_bytes());b.extend(a);
        let s=b.len();b[12..16].copy_from_slice(&(s as u32).to_le_bytes());
        b.extend([2,0,28,0,1,0,0,0,0x11,0,20,0,3,0,0,0,1,1,0,0,0,0,0,16,0,32,0,0]);
        (b,c,l)
    }
    #[test] fn process_and_thread_mic_require_exact_nwnr_without_changing_gui_policy() {
        let (b,c,l)=descriptor();let s=u32_at(&b,12).unwrap() as usize;
        for mask in [0u32,1,2,4,5,6,7,8,u32::MAX] {
            let mut changed=b.clone();changed[s+12..s+16].copy_from_slice(&mask.to_le_bytes());
            assert!(creation(&changed,c,l,false,0x2000).is_err(),"mask {mask}");
        }
        let mut thread=b.clone();let d=u32_at(&thread,16).unwrap() as usize;
        thread[d+12..d+16].copy_from_slice(&THREAD_MANAGEMENT.to_le_bytes());
        assert!(creation(&thread,c,l,true,0x2000).is_ok());
        assert!(creation(&thread,c,l,false,0x2000).is_err());
        assert!(medium_sacl_offset(&b).is_err()); // GUI repair input stays NW-only.
        for (offset,value) in [(s+4,2),(s+9,0x10),(s+10,19),(s+25,0)] {
            let mut changed=b.clone();changed[offset]=value;
            assert!(creation(&changed,c,l,false,0x2000).is_err());
        }
    }
    #[test] fn windows_kernel_creation_shape_keeps_system_owner_and_exact_ace_policy() {
        let (mut b,c,l)=descriptor();
        // Successful Windows original-handle reads can set SACL_AUTO_INHERITED
        // and select Administrators as owner. Neither grants implicit WRITE_DAC
        // because the exact OWNER_RIGHTS RC ACE remains required.
        b[2..4].copy_from_slice(&0x9814u16.to_le_bytes());
        let owner=Sid::from_bytes(&[1,2,0,0,0,0,0,5,32,0,0,0,32,2,0,0]).unwrap();
        b[20..20+owner.bytes().len()].copy_from_slice(owner.bytes());
        assert_eq!(creation(&b,c,l,false,0x2000).unwrap().owner,owner);
        let d=u32_at(&b,16).unwrap() as usize;
        for mask in [PROCESS_MANAGEMENT|0x20,PROCESS_MANAGEMENT|0x40000,0x10000000] {
            let mut changed=b.clone();changed[d+12..d+16].copy_from_slice(&mask.to_le_bytes());
            assert!(creation(&changed,c,l,false,0x2000).is_err());
        }
    }
    #[test] fn owner_readback_is_not_token_user_and_never_gifts_implicit_write_dac() {
        let (mut b,c,l)=descriptor();let original=creation(&b,c,l,false,0x2000).unwrap();
        assert_eq!(original.owner,c);assert_eq!(original.integrity_rid,0x2000);
        // A different legal system-default owner does not create implicit
        // WRITE_DAC because the exact OWNER_RIGHTS ACE remains mandatory.
        b[20+8..20+12].copy_from_slice(&32u32.to_le_bytes());
        assert_ne!(creation(&b,c,l,false,0x2000).unwrap().owner,c);
        let d=u32_at(&b,16).unwrap() as usize;
        b[d+8+4..d+8+8].copy_from_slice(&0x001f_ffffu32.to_le_bytes());
        assert!(creation(&b,c,l,false,0x2000).is_err());
    }
    #[test] fn null_unprotected_implicit_owner_and_missing_mic_never_pass_readback() {
        let (b,c,l)=descriptor();for index in [2,3,12,16] {
            let mut bad=b.clone();bad[index]=0;assert!(creation(&bad,c,l,false,0x2000).is_err());
        }
        assert!(creation(&b,c,l,false,0x3000).is_err());
        assert!(creation(&b,c,l,true,0x2000).is_err());
        assert!(creation(&b[..b.len()-1],c,l,false,0x2000).is_err());
    }
    #[test] fn actual_token_label_absence_has_two_bounded_representations_not_creation_proof() {
        let (original,c,l)=descriptor();
        let explicit=token_layout(&original).unwrap();
        assert!(explicit.explicit_label);assert_eq!(explicit.integrity_rid,0x2000);
        let offset=u32_at(&original,12).unwrap() as usize;
        let mut absent=original[..offset].to_vec();
        absent[12..16].copy_from_slice(&0u32.to_le_bytes());
        let observed=token_layout(&absent).unwrap();
        assert!(!observed.explicit_label);assert_eq!(observed.integrity_rid,0x2000);
        assert!(creation(&absent,c,l,false,0x2000).is_err());
        let mut empty=original[..offset].to_vec();empty.extend([2,0,8,0,0,0,0,0]);
        let observed=token_layout(&empty).unwrap();
        assert!(!observed.explicit_label);assert_eq!(observed.owner,c);
        assert_eq!(observed.integrity_rid,0x2000);
        assert!(creation(&empty,c,l,false,0x2000).is_err());
        for (index,value) in [(offset,4),(offset+2,12),(offset+4,1),(offset+6,1)] {
            let mut malformed=empty.clone();malformed[index]=value;
            assert!(token_layout(&malformed).is_err());
        }
        let mut no_present=empty.clone();no_present[2]&=!0x10;
        assert!(token_layout(&no_present).is_err());
        let mut null_dacl=empty.clone();null_dacl[16..20].fill(0);
        assert!(token_layout(&null_dacl).is_err());
    }
    #[test] fn default_template_never_confuses_token_query_with_thread_context_or_owner_authority() {
        let (b,c,l)=descriptor();let offset=u32_at(&b,16).unwrap() as usize;
        let (source,_)=acl(&b,offset).unwrap();let mut template=source.to_vec();
        let mut at=8;
        for _ in 0..3 {template[at+4..at+8].copy_from_slice(&OWNER_READ_CONTROL.to_le_bytes());
            at+=u16_at(&template,at+2).unwrap() as usize;}
        assert!(default_objects(&template,c,l).is_ok());
        for mask in [8u32,OWNER_READ_CONTROL|8,0x100000,0x40000,0x10000000] {
            let mut unsafe_acl=template.clone();unsafe_acl[12..16].copy_from_slice(&mask.to_le_bytes());
            assert!(default_objects(&unsafe_acl,c,l).is_err());
        }
        let mut inherited=template.clone();inherited[9]=0x10;
        assert!(default_objects(&inherited,c,l).is_err());
        assert!(default_objects(&[],c,l).is_err());
        let owner_offset=8+(8+c.bytes().len())+(8+l.bytes().len());
        let mut implicit_owner=template[..owner_offset].to_vec();
        implicit_owner[2..4].copy_from_slice(&(owner_offset as u16).to_le_bytes());
        implicit_owner[4..6].copy_from_slice(&2u16.to_le_bytes());
        assert!(default_objects(&implicit_owner,c,l).is_err());
    }
}



#[cfg(test)]
mod gui_policy_tests {
    use super::*;
    fn sid(parts:&[u32])->Sid {
        let mut b=vec![1,parts.len() as u8,0,0,0,0,0,5];
        for p in parts{b.extend(p.to_le_bytes());}Sid::from_bytes(&b).unwrap()
    }
    fn gui(label:Option<(u32,u32)>)->(Vec<u8>,Sid,Sid) { gui_for(label,false) }
    fn gui_for(label:Option<(u32,u32)>,desktop:bool)->(Vec<u8>,Sid,Sid) {
        let c=sid(&[21,1,2,3,1001]);let l=sid(&[5,7,8]);
        let ow=Sid::from_bytes(&[1,1,0,0,0,0,0,3,4,0,0,0]).unwrap();
        let mut b=vec![1,0,4,0x90];b.extend(20u32.to_le_bytes());b.extend([0;12]);b.extend(c.bytes());
        let d=b.len();b[16..20].copy_from_slice(&(d as u32).to_le_bytes());
        let mut acl=vec![2,0,0,0,3,0,0,0];
        let management=if desktop{crate::asuser_a1_rights::DESKTOP_MANAGEMENT}else{crate::asuser_a1_rights::STATION_MANAGEMENT};
        let target=if desktop{crate::asuser_a1_rights::DESKTOP_TARGET}else{crate::asuser_a1_rights::STATION_TARGET};
        for (s,m) in [(c,management),(l,target),(ow,OWNER_READ_CONTROL)] {
            acl.extend([0,0]);acl.extend(((8+s.bytes().len()) as u16).to_le_bytes());
            acl.extend(m.to_le_bytes());acl.extend(s.bytes());
        }
        let size=acl.len() as u16;acl[2..4].copy_from_slice(&size.to_le_bytes());b.extend(acl);
        if let Some((rid,mask))=label {
            let offset=b.len();b[12..16].copy_from_slice(&(offset as u32).to_le_bytes());b[2]|=0x10;
            b.extend([2,0,28,0,1,0,0,0,0x11,0,20,0]);b.extend(mask.to_le_bytes());
            b.extend([1,1,0,0,0,0,0,16]);b.extend(rid.to_le_bytes());
        }
        (b,c,l)
    }
    #[test] fn user32_station_read_control_is_required_but_desktop_contract_stays_exact() {
        for (desktop,expected,rejected) in [(false,0x0002037fu32,0x0000037fu32),(true,0x000001ff,0x000201ff)] {
            let (bytes,caller,logon)=gui_for(Some((0x2000,1)),desktop);
            let acl=u32_at(&bytes,16).unwrap() as usize;
            let target_mask=acl+8+8+caller.bytes().len()+4;
            assert_eq!(u32_at(&bytes,target_mask).unwrap(),expected);
            assert!(gui_observed(&bytes,caller,logon,desktop).is_ok());
            let mut wrong=bytes.clone();wrong[target_mask..target_mask+4].copy_from_slice(&rejected.to_le_bytes());
            assert!(gui_observed(&wrong,caller,logon,desktop).is_err());
        }
    }
    #[test] fn gui_runtime_read_access_never_authorizes_security_mutation_or_generic_masks() {
        for desktop in [false,true] {
            let (bytes,caller,logon)=gui_for(Some((0x2000,1)),desktop);
            let acl=u32_at(&bytes,16).unwrap() as usize;
            let target_mask=acl+8+8+caller.bytes().len()+4;
            let exact=u32_at(&bytes,target_mask).unwrap();
            for forbidden in [0x00040000u32,0x00080000,0x01000000,0x10000000,0x80000000] {
                let mut wrong=bytes.clone();wrong[target_mask..target_mask+4].copy_from_slice(&(exact|forbidden).to_le_bytes());
                assert!(gui_observed(&wrong,caller,logon,desktop).is_err());
            }
        }
    }
    #[test] fn exact_medium_and_observed_absence_need_zero_mutations_but_not_an_omitted_query() {
        for label in [None,Some((0x2000,1))] {
            let(b,c,l)=gui(label);let read=gui_observed(&b,c,l,false).unwrap();
            let mut attempt=LabelAttempt::new();assert!(!attempt.observe(read).unwrap());
            assert_eq!(attempt.phase,LabelPhase::Verified);assert_eq!(attempt.status,None);
            assert!(attempt.begin().is_err());assert!(read.label.medium());
        }
        let(b,c,l)=gui(None);assert!(gui_observed(&b[..20],c,l,false).is_err());
    }
    #[test] fn owned_high_has_one_label_only_transition_and_independent_post_readback() {
        let(before,c,l)=gui(Some((0x3000,1)));let(after,_,_)=gui(Some((0x2000,1)));
        let mut attempt=LabelAttempt::new();assert!(attempt.observe(gui_observed(&before,c,l,false).unwrap()).unwrap());
        attempt.begin().unwrap();attempt.returned(0).unwrap();
        assert!(attempt.begin().is_err()); // no replay, even before final readback
        attempt.verify(gui_observed(&after,c,l,false).unwrap()).unwrap();
        assert_eq!(attempt.phase,LabelPhase::Verified);assert_eq!(attempt.status,Some(0));
        assert!(attempt.begin().is_err());
    }
    #[test] fn label_set_error_and_owner_or_dacl_drift_never_authorize_target() {
        let(before,c,l)=gui(Some((0x3000,1)));let(after,_,_)=gui(Some((0x2000,1)));
        let mut attempt=LabelAttempt::new();attempt.observe(gui_observed(&before,c,l,false).unwrap()).unwrap();
        attempt.begin().unwrap();assert_eq!(attempt.returned(5).unwrap_err().raw_os_error(),Some(5));
        assert_eq!(attempt.phase,LabelPhase::Unknown);assert!(attempt.begin().is_err());
        let mut attempt=LabelAttempt::new();attempt.observe(gui_observed(&before,c,l,false).unwrap()).unwrap();
        attempt.begin().unwrap();attempt.returned(0).unwrap();
        let mut read=gui_observed(&after,c,l,false).unwrap();read.group=Some(c);
        assert!(attempt.verify(read).is_err());assert_eq!(attempt.phase,LabelPhase::Unknown);
        let mut foreign=after;foreign[28..32].copy_from_slice(&32u32.to_le_bytes());
        assert!(gui_observed(&foreign,c,l,false).is_err()); // group-owner/different-owner not S User
    }
    #[test] fn low_unknown_duplicate_inherited_and_malformed_label_aces_fail_closed() {
        for label in [(0x1000,1),(0x4000,1),(0x2000,0),(0x2000,3)] {
            let(b,c,l)=gui(Some(label));assert!(gui_observed(&b,c,l,false).is_err());
        }
        let(b,c,l)=gui(Some((0x2000,1)));let s=u32_at(&b,12).unwrap() as usize;
        for (offset,value) in [(s+4,2),(s+8,0),(s+9,0x10),(s+10,19),(s+17,2)] {
            let mut changed=b.clone();changed[offset]=value;assert!(gui_observed(&changed,c,l,false).is_err());
        }
    }
}
