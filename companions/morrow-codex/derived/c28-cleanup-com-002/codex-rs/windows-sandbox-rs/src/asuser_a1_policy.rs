//! Private A1 policy. Public facts are not admission capabilities.
//! No wire, getter or serialized record can construct PrecreationOrigin.
//! The native creator/immutable bootstrap inlet is deliberately still required.
use std::{io,
    path::{Component,Path,PathBuf,Prefix}, sync::Arc};
use crate::MatchedRunnerArtifact;
use crate::asuser_a1_protocol::{RoleTable};

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
pub(crate) use crate::asuser_a1_target::{Statistics,Sid,TargetPolicy,TargetFacts};
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
    pub(super) environment:Environment,cwd_paths:Vec<PathBuf>,
    pub(super) cwd_pins:crate::asuser_cwd_pins::CwdPins,
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
        let parents=cwd.ancestors().map(Path::to_path_buf).collect::<Vec<_>>();
        if parents.len()>32{return Err(reject());}
        let app=bundle.r.path().to_str().ok_or_else(reject)?;
        if app.contains('"')||app.contains('\0'){return Err(reject());}
        let text=format!("\"{app}\" --pipe-in={input} --pipe-out={output} --native-close-v8");
        let wide=|v:&str|v.encode_utf16().chain(Some(0)).collect();
        Ok(Self{application:wide(app),command:wide(&text),cwd:wide(cwd.to_str().ok_or_else(reject)?),
            desktop:Vec::new(),environment,cwd_paths:parents.into_iter().rev().collect(),
            cwd_pins:crate::asuser_cwd_pins::CwdPins::new()})
    }
    pub(super) fn pin_cwd_once(&mut self,deadline:std::time::Instant)->io::Result<()> {
        self.cwd_pins.open_once(&self.cwd_paths,deadline)
    }
}

/// Exact creation-time ACLs. OWNER_RIGHTS RC suppresses the owner-implicit
/// WRITE_DAC rule for every possible legal default owner; no arbitrary owner is
/// set. This DACL claim does NOT imply MIC/label or original-T reachability.
pub(crate) fn process_thread_sddl(caller:Sid,logon:Sid,thread:bool)->io::Result<String> {
    if caller==logon || caller.is_logon() || !logon.is_logon(){return Err(reject());}
    let management=if thread{THREAD_MANAGEMENT}else{PROCESS_MANAGEMENT};
    let target=if thread{THREAD_TARGET}else{PROCESS_TARGET};
    // Process/thread objects receive NO_READ_UP as well as NO_WRITE_UP from
    // Windows. Request both explicitly so creation and original readback use
    // the same stricter MIC contract; this does not alter the three DACL ACEs.
    Ok(format!("D:P(A;;0x{management:x};;;{})(A;;0x{target:x};;;{})(A;;0x{OWNER_READ_CONTROL:x};;;OW)S:(ML;;NWNR;;;ME)",
        caller.text(),logon.text()))
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
        TargetFacts{initial:stats,final_stats:stats,granted:TOKEN_REQUIRED,user,owner:user,context:[1;32],security_context:[2;32],change_notify:Some(3),logon,
            logon_attributes:0xc000_0004,logon_count:1,session:1,integrity_rid:0x2000,
            elevated:false,elevation_type:1,restricted:false,app_container:false,ui_access:false,
            virtualization:false,mandatory_policy:1,privilege_count:2,unsafe_privileges:0,admin_membership:false})
    }
    #[test] fn creation_dacl_has_owner_rights_but_no_stable_target_write_or_arbitrary_owner() {
        let (policy,f)=facts();let d=process_thread_sddl(policy.caller,f.logon,false).unwrap();
        assert!(d.starts_with("D:P"));assert!(!d.starts_with("O:"));
        assert!(d.contains("(A;;0x20000;;;OW)"));assert!(d.ends_with("S:(ML;;NWNR;;;ME)"));assert!(!d.contains(&f.user.text()));
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

