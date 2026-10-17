//! Private A1 policy. Public facts are not admission capabilities.
//! No wire, getter or serialized record can construct PrecreationOrigin.
//! The native creator/immutable bootstrap inlet is deliberately still required.
use std::{io, fs::{File,OpenOptions}, os::windows::fs::{OpenOptionsExt,MetadataExt},
    path::{Component,Path,PathBuf,Prefix}, sync::Arc};
use crate::MatchedRunnerArtifact;
use crate::asuser_a1_protocol::{RoleTable,valid_sid};

pub(crate) use crate::asuser_a1_rights::{TOKEN_REQUIRED,PROCESS_MANAGEMENT,THREAD_MANAGEMENT,
    PROCESS_TARGET,THREAD_TARGET,OWNER_READ_CONTROL};
pub(crate) const MAX_ENV_UNITS:usize=32767;

fn reject()->io::Error {io::Error::new(io::ErrorKind::PermissionDenied,"A1 original security admission rejected")}

/// No public constructor/Clone/Deserialize. A late acquired file/ancestor pin
/// cannot mint this type. The parent-side precreation bootstrap producer and
/// authenticated receiving inlet were absent in frozen046; only the new actual
/// native inlet may supply this origin, never a late path/hash acquisition.
pub(crate) struct PrecreationOrigin {
    pub(super) c:Arc<MatchedRunnerArtifact>,
    pub(super) native_pid:u32,
    pub(super) native_creation:u64,
    // Original native launcher's retained precreation identity, not a timestamp
    // asserted by C. Kept private until the native producer/inlet is reviewed.
    _launcher:Arc<crate::asuser_a1_helper::parent_native::AsUserParentOrigin>,
}
/// Original object-instance ties. These are not serialized role booleans.
/// No source selector can substitute equal path/hash pins for the original Arc.
pub(crate) struct Bundle {
    pub(super) c:Arc<MatchedRunnerArtifact>,
    pub(super) s:Arc<MatchedRunnerArtifact>,
    pub(super) r:Arc<MatchedRunnerArtifact>,
    origin:PrecreationOrigin,
    _materializer:Arc<()>,
    _factory:std::sync::Weak<dyn std::any::Any+Send+Sync>,
    setup:Arc<crate::OwnedWindowsSandboxSetup>,
    table:RoleTable,
}
impl Bundle {
    pub(crate) fn from_originals(origin:Arc<crate::asuser_a1_helper::parent_native::AsUserParentOrigin>,
        setup:Arc<crate::OwnedWindowsSandboxSetup>,s:Arc<MatchedRunnerArtifact>,r:Arc<MatchedRunnerArtifact>,
        materializer:Arc<()>,factory:std::sync::Weak<dyn std::any::Any+Send+Sync>)->io::Result<Self> {
        origin.guard.require_admitted(origin.pid,origin.creation)?;let c=origin.c.clone();let table=s.asuser_s_table()?;
        let precreation=PrecreationOrigin{c:c.clone(),native_pid:origin.pid,native_creation:origin.creation,_launcher:origin};
        let result=Self{c,s,r,origin:precreation,_materializer:materializer,_factory:factory,setup,table};
        result.verify_originals(result.origin.native_pid,result.origin.native_creation)?;Ok(result)
    }
    pub(super) fn same_original_runner(&self,runner:&MatchedRunnerArtifact)->bool {
        self.r.shares_original_pin(runner)
    }
    pub(super) fn verify_originals(&self,pid:u32,creation:u64)->io::Result<()> {
        self.origin._launcher.guard.require_admitted(pid,creation)?;
        if self._factory.upgrade().is_none(){return Err(reject());}
        if !self.origin.c.shares_original_pin(&self.c)
            || self.origin.native_pid!=pid || self.origin.native_creation!=creation
            || pid==0 || creation==0 || self.table.c!=self.c.sha256() || self.table.r!=self.r.sha256()
            || self.c.shares_original_pin(&self.s) || self.c.shares_original_pin(&self.r)
            || self.s.shares_original_pin(&self.r) || self.c.sha256()==self.s.sha256()
            || self.c.sha256()==self.r.sha256() || self.s.sha256()==self.r.sha256() {
            return Err(reject());
        }
        if self.s.sha256()!=self.setup.setup_exe_sha256()
            ||self.s.verify_resolved(self.setup.setup_exe_path()).is_err(){return Err(reject());}
        Ok(())
    }
    pub(super) fn target_username(&self)->&str {self.setup.offline_username()}
    pub(super) fn origin_retirement(&self,startup:u64)->crate::AsUserOriginRetirement {
        crate::AsUserOriginRetirement{origin:self.origin._launcher.guard.clone(),factory:self._factory.clone(),startup}
    }
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct Statistics {pub token:u64,pub auth:u64,pub modified:u64,pub primary:bool}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct Sid {bytes:[u8;68],len:u8}
impl Sid {
    pub(crate) fn from_text(text:&str)->io::Result<Self> {
        if text.len()>184||!text.starts_with("S-1-"){return Err(reject());}
        let parts=text[4..].split('-').collect::<Vec<_>>();if parts.len()<2||parts.len()>16{return Err(reject());}
        let integer=|part:&str|->io::Result<u64>{
            if part.is_empty()||part.len()>15||part.len()>1&&part.starts_with('0')||!part.bytes().all(|v|v.is_ascii_digit()){return Err(reject());}
            part.parse().map_err(|_|reject())};
        let authority=integer(parts[0])?;if authority>0xffff_ffff_ffff{return Err(reject());}
        let mut bytes=vec![1,(parts.len()-1) as u8];bytes.extend(&authority.to_be_bytes()[2..]);
        for part in &parts[1..]{let value=u32::try_from(integer(part)?).map_err(|_|reject())?;bytes.extend(value.to_le_bytes());}
        Self::from_bytes(&bytes)
    }
    pub(crate) fn from_bytes(bytes:&[u8])->io::Result<Self> {
        if !valid_sid(bytes){return Err(reject());}
        let mut result=Self{bytes:[0;68],len:bytes.len() as u8};
        result.bytes[..bytes.len()].copy_from_slice(bytes);Ok(result)
    }
    pub(crate) fn bytes(&self)->&[u8] {&self.bytes[..usize::from(self.len)]}
    pub(crate) fn text(&self)->String {
        let b=self.bytes();let auth=b[2..8].iter().fold(0u64,|n,v|(n<<8)|u64::from(*v));
        let mut result=format!("S-1-{auth}");
        for part in b[8..].chunks_exact(4) {result.push_str(&format!("-{}",u32::from_le_bytes(part.try_into().unwrap())));}
        result
    }
    pub(crate) fn is_logon(&self)->bool {
        self.len==20 && self.bytes[..12]==[1,3,0,0,0,0,0,5,5,0,0,0]
            &&self.bytes[12..20].iter().any(|byte|*byte!=0)
    }
}
/// Finite explicit block; never std::env, ambient inheritance or profile loading.
/// Constructor validates and sorts once before any token/native allocation.
pub(crate) struct Environment {units:Vec<u16>,actual_system:bool}
impl Environment {
    pub(crate) fn new(entries:&[(&str,&str)])->io::Result<Self> {
        if entries.len()>64{return Err(reject());}
        let mut sorted=entries.to_vec();
        sorted.sort_by_key(|(key,_)|key.to_ascii_uppercase());
        let mut previous=None;let mut units=Vec::new();
        for (key,value) in sorted {
            if key.is_empty() || key.len()>128 || value.len()>MAX_ENV_UNITS*3
                || !key.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'_')
                || value.contains('\0') || previous.as_ref().is_some_and(|old:&String|old.eq_ignore_ascii_case(key)) {
                return Err(reject());
            }
            previous=Some(key.to_owned());
            let added=key.encode_utf16().count()+value.encode_utf16().count()+2;
            if units.len().checked_add(added).is_none_or(|v|v>=MAX_ENV_UNITS){return Err(reject());}
            units.extend(key.encode_utf16());units.push(b'=' as u16);
            units.extend(value.encode_utf16());units.push(0);
            if units.len()>=MAX_ENV_UNITS{return Err(reject());}
        }
        units.push(0);if units.len()==1{units.push(0);}
        Ok(Self{units,actual_system:false})
    }
    pub(crate) fn from_system(directory:crate::asuser_a1_native::NativeSystemDirectory)->io::Result<Self> {
        let root=directory.into_root();let command=format!("{root}\\System32\\cmd.exe");
        let mut environment=Self::new(&[("SystemRoot",&root),("WINDIR",&root),("COMSPEC",&command)])?;
        environment.actual_system=true;Ok(environment)
    }
    pub(super) fn as_mut_ptr(&mut self)->*mut std::ffi::c_void {self.units.as_mut_ptr().cast()}
}
pub(crate) struct Command {
    pub(super) application:Vec<u16>,pub(super) command:Vec<u16>,
    pub(super) cwd:Vec<u16>,pub(super) desktop:Vec<u16>,
    pub(super) environment:Environment,_cwd_path:PathBuf,_cwd_pins:Vec<File>,
}
impl Command {
    /// Only the pinned R plus the existing fixed runner transport arguments.
    /// The endpoint strings are data for the existing v8 transport, never extra
    /// switches, arbitrary business argv or a shell.
    pub(super) fn new(bundle:&Bundle,input:&str,output:&str,
        environment:Environment)->io::Result<Self> {
        // Trusted R startup must not search an untrusted business workspace for
        // DLLs. The business request keeps its own unchanged cwd on the existing
        // v8 SpawnRequest; only this new A1 runner's startup cwd is its pinned
        // installation directory. No ambient current-directory mutation.
        let cwd=bundle.r.path().parent().ok_or_else(reject)?;
        if !environment.actual_system||!cwd.is_absolute() || cwd.to_str().is_none_or(|v|v.contains('\0')||v.len()>32700)
            || ![input,output].iter().all(|s|s.starts_with(r"\\.\pipe\")
                && s.len()<=160 && s.bytes().all(|b|b.is_ascii_alphanumeric()||b"\\.-_".contains(&b))) {
            return Err(reject());
        }
        let mut components=cwd.components();
        if !matches!(components.next(),Some(Component::Prefix(p)) if matches!(p.kind(),Prefix::Disk(_)))
            ||components.next()!=Some(Component::RootDir)
            ||!components.all(|c|matches!(c,Component::Normal(s) if !s.to_string_lossy().contains(':'))) {
            return Err(reject());
        }
        let parents=cwd.ancestors().collect::<Vec<_>>();
        if parents.len()>32{return Err(reject());}
        let mut cwd_pins=Vec::with_capacity(parents.len());
        for path in parents.iter().rev() {
            let file=OpenOptions::new().read(true).share_mode(1).custom_flags(0x02000000|0x00200000).open(path)?;
            let metadata=file.metadata()?;
            if !metadata.is_dir()||metadata.file_attributes()&0x400!=0{return Err(reject());}
            cwd_pins.push(file);
        }
        let app=bundle.r.path().to_str().ok_or_else(reject)?;
        if app.contains('"')||app.contains('\0'){return Err(reject());}
        let text=format!("\"{app}\" --pipe-in={input} --pipe-out={output} --native-close-v8");
        let wide=|v:&str|v.encode_utf16().chain(Some(0)).collect();
        Ok(Self{application:wide(app),command:wide(&text),cwd:wide(cwd.to_str().ok_or_else(reject)?),
            desktop:Vec::new(),environment,_cwd_path:cwd.to_path_buf(),_cwd_pins:cwd_pins})
    }
}

/// Exact creation-time ACLs. OWNER_RIGHTS RC suppresses the owner-implicit
/// WRITE_DAC rule for every possible legal default owner; no arbitrary owner is
/// set. This DACL claim does NOT imply MIC/label or original-T reachability.
pub(crate) fn process_thread_sddl(caller:Sid,logon:Sid,thread:bool)->io::Result<String> {
    if caller==logon || caller.is_logon() || !logon.is_logon(){return Err(reject());}
    let management=if thread{THREAD_MANAGEMENT}else{PROCESS_MANAGEMENT};
    let target=if thread{THREAD_TARGET}else{PROCESS_TARGET};
    Ok(format!("D:P(A;;0x{management:x};;;{})(A;;0x{target:x};;;{})(A;;0x{OWNER_READ_CONTROL:x};;;OW)S:(ML;;NW;;;ME)",
        caller.text(),logon.text()))
}

pub(crate) struct TargetPolicy {
    pub(super) managed_user:Sid,
    pub(super) caller:Sid,
    pub(super) session:u32,
    pub(super) caller_auth:u64,
    pub(super) caller_logon:Sid,
    pub(super) caller_token:u64,
    pub(super) integrity_rid:u32,
}
/// Actual original-token facts, interpreted only inside the retained native
/// attempt. Never Serialize, and never a replacement for the token itself.
#[derive(Clone,Debug,PartialEq,Eq)]
pub(crate) struct TargetFacts {
    pub initial:Statistics,pub final_stats:Statistics,pub granted:u32,
    pub user:Sid,pub owner:Sid,pub context:[u8;32],pub logon:Sid,pub logon_attributes:u32,pub logon_count:u16,
    pub session:u32,pub integrity_rid:u32,pub elevated:bool,pub elevation_type:u32,
    pub restricted:bool,pub app_container:bool,pub ui_access:bool,
    pub virtualization:bool,pub mandatory_policy:u32,pub privilege_count:u16,pub unsafe_privileges:u16,pub admin_membership:bool,
}
impl TargetFacts {
    pub(super) fn same_after_default_initialization(&self,before:&Self,actual_successful_mutation:bool)->bool {
        // The one authorized default-DACL mutation may or may not change
        // ModifiedId. EVERYTHING else actually observed must still match.
        if !actual_successful_mutation{return self==before;}
        let mut normalized=self.clone();normalized.initial.modified=before.initial.modified;
        normalized.final_stats.modified=before.final_stats.modified;normalized==*before
    }
}
impl TargetPolicy {
    pub(super) fn validate(&self,f:&TargetFacts)->io::Result<()> {
        self.validate_with_mask(f,TOKEN_REQUIRED)
    }
    pub(super) fn validate_runner(&self,f:&TargetFacts,logon:Sid,auth:u64)->io::Result<()> {
        self.validate_with_mask(f,0x00020008)?;
        if f.logon!=logon||f.initial.auth!=auth{return Err(reject());}Ok(())
    }
    fn validate_with_mask(&self,f:&TargetFacts,required:u32)->io::Result<()> {
        if f.initial!=f.final_stats || !f.initial.primary || f.initial.token==0 || f.initial.auth==0
            || f.initial.auth==self.caller_auth || f.initial.token==self.caller_token||f.initial.modified==0
            || f.granted&required!=required || f.user!=self.managed_user
            || f.user==self.caller || f.user.is_logon() || f.logon_count!=1
            // LogonSID and AuthenticationId are independent actual fields.
            // No public API contract requires SID subauthorities X/Y to be a
            // numerical encoding of AuthenticationId. R must match BOTH T
            // observations separately in validate_runner, never a derivation.
            || !f.logon.is_logon()||f.logon==self.caller_logon
            || f.logon_attributes&0xc000_0004!=0xc000_0004
            || f.logon_attributes&0x10!=0 // deny-only
            || f.session!=self.session || f.integrity_rid!=self.integrity_rid
            || f.integrity_rid>0x2000 || f.elevated || f.elevation_type==2
            || !matches!(f.elevation_type,1|3) || f.restricted || f.app_container
            || f.ui_access || f.virtualization || f.mandatory_policy&1==0 || f.mandatory_policy&!3!=0
            || f.privilege_count>64 || f.unsafe_privileges!=0 ||f.admin_membership {
            return Err(reject());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sid(parts:&[u32])->Sid {
        let mut b=vec![1,parts.len() as u8,0,0,0,0,0,5];
        for p in parts{b.extend(p.to_le_bytes());}Sid::from_bytes(&b).unwrap()
    }
    fn facts()->(TargetPolicy,TargetFacts) {
        let user=sid(&[21,1,2,3,1001]);let caller=sid(&[21,1,2,3,1002]);let logon=sid(&[5,7,8]);
        let stats=Statistics{token:1,auth:(7u64<<32)|8,modified:4,primary:true};
        (TargetPolicy{managed_user:user,caller,session:1,caller_auth:3,caller_logon:sid(&[5,97,101]),caller_token:77,integrity_rid:0x2000},
        TargetFacts{initial:stats,final_stats:stats,granted:TOKEN_REQUIRED,user,owner:user,context:[1;32],logon,
            logon_attributes:0xc000_0004,logon_count:1,session:1,integrity_rid:0x2000,
            elevated:false,elevation_type:1,restricted:false,app_container:false,ui_access:false,
            virtualization:false,mandatory_policy:1,privilege_count:2,unsafe_privileges:0,admin_membership:false})
    }
    #[test] fn target_identity_is_fresh_enabled_logon_not_stable_user_or_caller_restriction() {
        let (policy,mut f)=facts();assert!(policy.validate(&f).is_ok());
        f.logon=f.user;assert!(policy.validate(&f).is_err());
        let (_,mut f)=facts();f.restricted=true;assert!(policy.validate(&f).is_err());
        let (_,mut f)=facts();f.logon_attributes|=0x10;assert!(policy.validate(&f).is_err());
        let (_,mut f)=facts();f.initial.auth=policy.caller_auth;f.final_stats=f.initial;
        assert!(policy.validate(&f).is_err());
    }
    #[test] fn target_policy_rejects_partial_grant_aba_privilege_and_wrong_integrity() {
        let (policy,mut f)=facts();f.granted&=!2;assert!(policy.validate(&f).is_err());
        let (_,mut f)=facts();f.final_stats.modified+=1;assert!(policy.validate(&f).is_err());
        let (_,mut f)=facts();f.unsafe_privileges=1;assert!(policy.validate(&f).is_err());
        let (_,mut f)=facts();f.admin_membership=true;f.elevation_type=3;
        assert!(policy.validate(&f).is_err()); // deny-only BA is still an administrator account
        let (_,mut f)=facts();f.integrity_rid=0x3000;assert!(policy.validate(&f).is_err());
        let (_,mut f)=facts();f.mandatory_policy=0;assert!(policy.validate(&f).is_err());
        let (_,mut f)=facts();f.mandatory_policy=5;assert!(policy.validate(&f).is_err());
        let (_,mut f)=facts();f.mandatory_policy=3;assert!(policy.validate(&f).is_ok());
    }
    #[test] fn created_runner_context_requires_its_actual_read_only_query_and_same_fresh_logon() {
        let (policy,mut actual)=facts();let logon=actual.logon;let auth=actual.initial.auth;
        actual.granted=0x20008;actual.initial.token=55;actual.final_stats=actual.initial;
        assert!(policy.validate_runner(&actual,logon,auth).is_ok());
        assert!(policy.validate(&actual).is_err()); // query alias is not original T's creation grant
        actual.integrity_rid=0x3000;assert!(policy.validate_runner(&actual,logon,auth).is_err());
        let (_,mut actual)=facts();actual.initial.auth+=1;actual.final_stats=actual.initial;
        assert!(policy.validate_runner(&actual,logon,auth).is_err());
    }
    #[test] fn independent_observed_logon_sid_and_authentication_id_are_not_arithmetic_aliases() {
        for (parts,auth) in [([5,19,23],0x0000_0011_0000_001du64),
            ([5,41,43],0x0000_002f_0000_0035u64)] {
            let (policy,mut actual)=facts();actual.logon=sid(&parts);
            actual.initial.auth=auth;actual.final_stats=actual.initial;
            assert!(policy.validate(&actual).is_ok());
            let original_logon=actual.logon;actual.granted=0x20008;
            assert!(policy.validate_runner(&actual,original_logon,auth).is_ok());
            // Same AuthId cannot conceal a different actual LogonSID.
            assert!(policy.validate_runner(&actual,sid(&[5,61,67]),auth).is_err());
            // Same LogonSID cannot conceal a different actual AuthId.
            assert!(policy.validate_runner(&actual,original_logon,auth+1).is_err());
        }
    }
    #[test] fn explicit_default_change_has_a_new_stable_modified_window_but_no_other_policy_drift() {
        let (_,before)=facts();let mut after=before.clone();
        assert!(after.same_after_default_initialization(&before,false));
        assert!(after.same_after_default_initialization(&before,true));
        after.initial.modified+=7;after.final_stats=after.initial;
        assert!(!after.same_after_default_initialization(&before,false));
        assert!(after.same_after_default_initialization(&before,true));
        after.context[0]^=1;assert!(!after.same_after_default_initialization(&before,true));
        after=before.clone();after.owner=sid(&[32,544]);assert!(!after.same_after_default_initialization(&before,true));
        after=before.clone();after.initial.token+=1;after.final_stats=after.initial;
        assert!(!after.same_after_default_initialization(&before,true));
    }
    #[test] fn creation_dacl_has_owner_rights_but_no_stable_target_write_or_arbitrary_owner() {
        let (policy,f)=facts();let d=process_thread_sddl(policy.caller,f.logon,false).unwrap();
        assert!(d.starts_with("D:P"));assert!(!d.starts_with("O:"));
        assert!(d.contains("(A;;0x20000;;;OW)"));assert!(d.ends_with("S:(ML;;NW;;;ME)"));assert!(!d.contains(&f.user.text()));
        assert!(d.contains(&format!("(A;;0x100000;;;{})",f.logon.text())));
        assert!(process_thread_sddl(policy.caller,f.user,false).is_err());
    }
    #[test] fn explicit_environment_is_finite_double_nul_and_case_unique() {
        let env=Environment::new(&[("B","2"),("A","1")]).unwrap();
        assert_eq!(env.units,"A=1\0B=2\0\0".encode_utf16().collect::<Vec<_>>());
        assert!(Environment::new(&[("PATH","a"),("path","b")]).is_err());
        assert!(Environment::new(&[("A","nul\0value")]).is_err());
        assert!(Environment::new(&[("A",&"x".repeat(MAX_ENV_UNITS))]).is_err());
        assert_eq!(Environment::new(&[]).unwrap().units,vec![0,0]);
    }
}

