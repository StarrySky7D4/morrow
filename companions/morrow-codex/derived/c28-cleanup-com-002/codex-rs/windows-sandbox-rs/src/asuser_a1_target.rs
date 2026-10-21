//! Pure login/launch contract shared by native admission and diagnostics.
//! No token handles, OS calls, serialization or authority constructors.
#![allow(dead_code)]
use std::io;
// READ_CONTROL | TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY.
// The native rights module checks this against the SDK constants.
pub(crate) const TOKEN_REQUIRED:u32=0x0002000b;
fn reject()->io::Error {io::Error::new(io::ErrorKind::PermissionDenied,"A1 original security admission rejected")}
fn valid_sid(bytes:&[u8])->bool {
    bytes.len()>=8&&bytes.len()<=68&&bytes[0]==1&&bytes[1]<=15&&bytes.len()==8+usize::from(bytes[1])*4
}
/// Bounded diagnostic reasons for the FIRST matching original target-policy
/// predicate, never a token fact, OS error, SID/path or admission capability.
/// None in WindowsAsUserNativeFailure means not observed; no clean inference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WindowsTargetPolicyRejection {
    /// Guard denied but the diagnostic mirror could not classify it. Stay denied.
    UnclassifiedGuardRejection,
    StatisticsChanged,
    NotPrimary,
    TokenIdentityZero,
    AuthenticationIdentityZero,
    AuthenticationEqualsCaller,
    TokenIdentityEqualsCaller,
    ModifiedIdentityZero,
    RequiredGrantMissing,
    UserNotManaged,
    UserEqualsCaller,
    UserIsLogonSid,
    LogonGroupCountNotOne,
    LogonSidInvalid,
    LogonEqualsCaller,
    LogonAttributesMissing,
    LogonDenyOnly,
    SessionMismatch,
    IntegrityMismatch,
    IntegrityAboveMedium,
    Elevated,
    FullElevationType,
    UnsupportedElevationType,
    Restricted,
    AppContainer,
    UiAccess,
    Virtualization,
    MandatoryNoWriteUpMissing,
    MandatoryPolicyUnsupportedBits,
    PrivilegeCountExceeded,
    UnsafePrivilegesPresent,
    AdministratorMembership,
    DerivedIdentityUnchanged,
    DerivedContextChanged,
    DerivedGrantChanged,
    DerivedPrivilegeMismatch,
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
pub(crate) struct TargetPolicy {
    pub(crate) managed_user:Sid,
    pub(crate) caller:Sid,
    pub(crate) session:u32,
    pub(crate) caller_auth:u64,
    pub(crate) caller_logon:Sid,
    pub(crate) caller_token:u64,
    pub(crate) integrity_rid:u32,
}
/// Actual original-token facts, interpreted only inside the retained native
/// attempt. Never Serialize, and never a replacement for the token itself.
#[derive(Clone,Debug,PartialEq,Eq)]
pub(crate) struct TargetFacts {
    pub initial:Statistics,pub final_stats:Statistics,pub granted:u32,
    pub user:Sid,pub owner:Sid,pub context:[u8;32],pub security_context:[u8;32],pub change_notify:Option<u32>,pub logon:Sid,pub logon_attributes:u32,pub logon_count:u16,
    pub session:u32,pub integrity_rid:u32,pub elevated:bool,pub elevation_type:u32,
    pub restricted:bool,pub app_container:bool,pub ui_access:bool,
    pub virtualization:bool,pub mandatory_policy:u32,pub privilege_count:u16,pub unsafe_privileges:u16,pub admin_membership:bool,
}
impl TargetFacts {
    pub(crate) fn same_after_default_initialization(&self,before:&Self,actual_successful_mutation:bool)->bool {
        // The one authorized default-DACL mutation may or may not change
        // ModifiedId. EVERYTHING else actually observed must still match.
        if !actual_successful_mutation{return self==before;}
        let mut normalized=self.clone();normalized.initial.modified=before.initial.modified;
        normalized.final_stats.modified=before.final_stats.modified;normalized==*before
    }
}
impl TargetPolicy {
    pub(crate) fn validate(&self,f:&TargetFacts)->io::Result<()> {
        self.validate_with_mask(f,TOKEN_REQUIRED)
    }
    pub(crate) fn validate_runner(&self,f:&TargetFacts,logon:Sid,auth:u64)->io::Result<()> {
        self.validate_with_mask(f,0x00020008)?;
        if f.logon!=logon||f.initial.auth!=auth{return Err(reject());}Ok(())
    }
    /// The same pure evaluator supplies admission and its first rejection.
    /// Facts never authorize an OS call or construct a retained native owner.
    pub(crate) fn first_rejection(&self,f:&TargetFacts)->Option<WindowsTargetPolicyRejection> {
        self.rejection(f,TOKEN_REQUIRED,false,false)
    }
    fn rejection(&self,f:&TargetFacts,required:u32,source:bool,filtered:bool)->Option<WindowsTargetPolicyRejection> {
        use WindowsTargetPolicyRejection as R;
        if f.initial!=f.final_stats {return Some(R::StatisticsChanged);}
        if !f.initial.primary {return Some(R::NotPrimary);}
        if f.initial.token==0 {return Some(R::TokenIdentityZero);}
        if f.initial.auth==0 {return Some(R::AuthenticationIdentityZero);}
        if f.initial.auth==self.caller_auth {return Some(R::AuthenticationEqualsCaller);}
        if f.initial.token==self.caller_token {return Some(R::TokenIdentityEqualsCaller);}
        if f.initial.modified==0 {return Some(R::ModifiedIdentityZero);}
        if f.granted&required!=required {return Some(R::RequiredGrantMissing);}
        if f.user!=self.managed_user {return Some(R::UserNotManaged);}
        if f.user==self.caller {return Some(R::UserEqualsCaller);}
        if f.user.is_logon() {return Some(R::UserIsLogonSid);}
        if f.logon_count!=1 {return Some(R::LogonGroupCountNotOne);}
        if !f.logon.is_logon() {return Some(R::LogonSidInvalid);}
        if f.logon==self.caller_logon {return Some(R::LogonEqualsCaller);}
        if f.logon_attributes&0xc000_0004!=0xc000_0004 {return Some(R::LogonAttributesMissing);}
        if f.logon_attributes&0x10!=0 {return Some(R::LogonDenyOnly);}
        if f.session!=self.session {return Some(R::SessionMismatch);}
        if f.integrity_rid!=self.integrity_rid {return Some(R::IntegrityMismatch);}
        if f.integrity_rid>0x2000 {return Some(R::IntegrityAboveMedium);}
        if f.elevated {return Some(R::Elevated);}
        if f.elevation_type==2 {return Some(R::FullElevationType);}
        if !matches!(f.elevation_type,1|3) {return Some(R::UnsupportedElevationType);}
        if f.restricted&&!filtered {return Some(R::Restricted);}
        if f.app_container {return Some(R::AppContainer);}
        if f.ui_access {return Some(R::UiAccess);}
        if f.virtualization {return Some(R::Virtualization);}
        if f.mandatory_policy&1==0 {return Some(R::MandatoryNoWriteUpMissing);}
        if f.mandatory_policy&!3!=0 {return Some(R::MandatoryPolicyUnsupportedBits);}
        if f.privilege_count>64 {return Some(R::PrivilegeCountExceeded);}
        if f.unsafe_privileges!=0&&!source {return Some(R::UnsafePrivilegesPresent);}
        if f.admin_membership {return Some(R::AdministratorMembership);}
        None
    }
    fn validate_with_mask(&self,f:&TargetFacts,required:u32)->io::Result<()> {
        self.rejection(f,required,false,false).map_or(Ok(()),|_|Err(reject()))
    }
    /// Login source may contain account privileges, but must otherwise be the
    /// fresh, nonadministrator, unfiltered managed account in the caller session.
    pub(crate) fn validate_source(&self,f:&TargetFacts)->io::Result<()> {
        self.source_rejection(f).map_or(Ok(()),|_|Err(reject()))
    }
    pub(crate) fn source_rejection(&self,f:&TargetFacts)->Option<WindowsTargetPolicyRejection> {
        self.rejection(f,TOKEN_REQUIRED,true,false)
    }
    /// Facts alone grant no authority. The native Attempt owns both handles and
    /// is the sole caller after its one CreateRestrictedToken invocation.
    pub(crate) fn validate_launch(&self,source:&TargetFacts,launch:&TargetFacts)->io::Result<()> {
        self.launch_rejection(source,launch).map_or(Ok(()),|_|Err(reject()))
    }
    pub(crate) fn launch_rejection(&self,source:&TargetFacts,launch:&TargetFacts)->Option<WindowsTargetPolicyRejection> {
        use WindowsTargetPolicyRejection as R;
        if let Some(reason)=self.source_rejection(source){return Some(reason);}
        if let Some(reason)=self.rejection(launch,TOKEN_REQUIRED,false,true){return Some(reason);}
        if source.initial.token==launch.initial.token{return Some(R::DerivedIdentityUnchanged);}
        if source.security_context!=launch.security_context
            || source.initial.auth!=launch.initial.auth
            || source.user!=launch.user || source.owner!=launch.owner
            || source.logon!=launch.logon || source.logon_attributes!=launch.logon_attributes
            || source.session!=launch.session || source.integrity_rid!=launch.integrity_rid
            || source.mandatory_policy!=launch.mandatory_policy
            || source.elevation_type!=launch.elevation_type {return Some(R::DerivedContextChanged);}
        if source.granted!=launch.granted{return Some(R::DerivedGrantChanged);}
        if source.change_notify!=launch.change_notify
            || Some(launch.privilege_count)!=source.privilege_count.checked_sub(source.unsafe_privileges)
            || launch.privilege_count>1 {return Some(R::DerivedPrivilegeMismatch);}
        None
    }
    pub(crate) fn validate_launched_runner(&self,f:&TargetFacts,baseline:&TargetFacts)->io::Result<()> {
        if self.rejection(f,0x00020008,false,true).is_some()
            || f.logon!=baseline.logon || f.initial.auth!=baseline.initial.auth
            || f.owner!=baseline.owner || f.context!=baseline.context
            || f.restricted!=baseline.restricted {return Err(reject());}
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
        TargetFacts{initial:stats,final_stats:stats,granted:TOKEN_REQUIRED,user,owner:user,context:[1;32],security_context:[2;32],change_notify:Some(3),logon,
            logon_attributes:0xc000_0004,logon_count:1,session:1,integrity_rid:0x2000,
            elevated:false,elevation_type:1,restricted:false,app_container:false,ui_access:false,
            virtualization:false,mandatory_policy:1,privilege_count:1,unsafe_privileges:0,admin_membership:false})
    }
    #[test] fn account_privileges_are_source_only_and_must_be_deleted_before_launch() {
        let (policy,mut source)=facts();
        // The default Users account can have a disabled SeTimeZonePrivilege.
        source.privilege_count=2;source.unsafe_privileges=1;
        assert!(policy.validate_source(&source).is_ok());
        assert!(policy.validate(&source).is_err());
        let mut launch=source.clone();
        launch.initial.token=91;launch.initial.modified=92;launch.final_stats=launch.initial;
        launch.restricted=true;launch.privilege_count=1;launch.unsafe_privileges=0;
        launch.context=[3;32];
        assert!(policy.validate_launch(&source,&launch).is_ok());
        let mut wrong=launch.clone();wrong.unsafe_privileges=1;
        assert!(policy.validate_launch(&source,&wrong).is_err());
        wrong=launch.clone();wrong.change_notify=Some(7);
        assert!(policy.validate_launch(&source,&wrong).is_err());
        wrong=launch.clone();wrong.privilege_count=0;
        assert!(policy.validate_launch(&source,&wrong).is_err());
        wrong=launch.clone();wrong.security_context[0]^=1;
        assert!(policy.validate_launch(&source,&wrong).is_err());
        wrong=launch.clone();wrong.initial.token=source.initial.token;wrong.final_stats=wrong.initial;
        assert!(policy.validate_launch(&source,&wrong).is_err());
        source.admin_membership=true;
        assert!(policy.validate_launch(&source,&launch).is_err());
    }
    #[test] fn filtered_launch_identity_is_retained_through_runner_readback() {
        let (policy,source)=facts();let mut launch=source.clone();
        launch.initial.token=91;launch.final_stats=launch.initial;launch.restricted=true;
        assert!(policy.validate_launch(&source,&launch).is_ok());
        let mut runner=launch.clone();runner.granted=0x20008;
        runner.initial.token=93;runner.final_stats=runner.initial;
        assert!(policy.validate_launched_runner(&runner,&launch).is_ok());
        runner.restricted=false;
        assert!(policy.validate_launched_runner(&runner,&launch).is_err());
        runner=launch.clone();runner.context[0]^=1;
        assert!(policy.validate_launched_runner(&runner,&launch).is_err());
    }
    #[test] fn exact_original_grant_still_rejects_security_right_loss_or_extra_rights() {
        let (policy,mut source)=facts();source.granted=0x000f01ff;
        let mut launch=source.clone();launch.initial.token=91;launch.final_stats=launch.initial;
        launch.restricted=true;
        assert!(policy.validate_launch(&source,&launch).is_ok());
        launch.granted=0x000301ff;
        assert_eq!(policy.launch_rejection(&source,&launch),Some(WindowsTargetPolicyRejection::DerivedGrantChanged));
        assert!(policy.validate_launch(&source,&launch).is_err());
        source.granted=TOKEN_REQUIRED;launch.granted=0x000f01ff;
        assert_eq!(policy.launch_rejection(&source,&launch),Some(WindowsTargetPolicyRejection::DerivedGrantChanged));
        launch.security_context[0]^=1;
        assert_eq!(policy.launch_rejection(&source,&launch),Some(WindowsTargetPolicyRejection::DerivedContextChanged));
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
    #[test] fn diagnostic_mirror_names_every_original_reject_without_changing_error_or_policy() {
        use WindowsTargetPolicyRejection as R;
        let cases:[(R,fn(&mut TargetPolicy,&mut TargetFacts));31]=[
            (R::StatisticsChanged,|_p,f|{f.final_stats.modified+=1;}),
            (R::NotPrimary,|_p,f|{f.initial.primary=false;f.final_stats=f.initial;}),
            (R::TokenIdentityZero,|_p,f|{f.initial.token=0;f.final_stats=f.initial;}),
            (R::AuthenticationIdentityZero,|_p,f|{f.initial.auth=0;f.final_stats=f.initial;}),
            (R::AuthenticationEqualsCaller,|p,f|{f.initial.auth=p.caller_auth;f.final_stats=f.initial;}),
            (R::TokenIdentityEqualsCaller,|p,f|{f.initial.token=p.caller_token;f.final_stats=f.initial;}),
            (R::ModifiedIdentityZero,|_p,f|{f.initial.modified=0;f.final_stats=f.initial;}),
            (R::RequiredGrantMissing,|_p,f|{f.granted&=!2;}),
            (R::UserNotManaged,|_p,f|{f.user=sid(&[21,7,8,9,1003]);}),
            (R::UserEqualsCaller,|p,f|{p.managed_user=p.caller;f.user=p.caller;}),
            (R::UserIsLogonSid,|p,f|{p.managed_user=f.logon;f.user=f.logon;}),
            (R::LogonGroupCountNotOne,|_p,f|{f.logon_count=2;}),
            (R::LogonSidInvalid,|_p,f|{f.logon=f.user;}),
            (R::LogonEqualsCaller,|p,f|{f.logon=p.caller_logon;}),
            (R::LogonAttributesMissing,|_p,f|{f.logon_attributes&=!4;}),
            (R::LogonDenyOnly,|_p,f|{f.logon_attributes|=0x10;}),
            (R::SessionMismatch,|p,f|{f.session=p.session+1;}),
            (R::IntegrityMismatch,|_p,f|{f.integrity_rid=0x1000;}),
            (R::IntegrityAboveMedium,|p,f|{p.integrity_rid=0x3000;f.integrity_rid=0x3000;}),
            (R::Elevated,|_p,f|{f.elevated=true;}),
            (R::FullElevationType,|_p,f|{f.elevation_type=2;}),
            (R::UnsupportedElevationType,|_p,f|{f.elevation_type=0;}),
            (R::Restricted,|_p,f|{f.restricted=true;}),
            (R::AppContainer,|_p,f|{f.app_container=true;}),
            (R::UiAccess,|_p,f|{f.ui_access=true;}),
            (R::Virtualization,|_p,f|{f.virtualization=true;}),
            (R::MandatoryNoWriteUpMissing,|_p,f|{f.mandatory_policy=0;}),
            (R::MandatoryPolicyUnsupportedBits,|_p,f|{f.mandatory_policy=5;}),
            (R::PrivilegeCountExceeded,|_p,f|{f.privilege_count=65;}),
            (R::UnsafePrivilegesPresent,|_p,f|{f.unsafe_privileges=1;}),
            (R::AdministratorMembership,|_p,f|{f.admin_membership=true;}),
        ];
        for (reason,mutate) in cases {
            let (mut policy,mut f)=facts();mutate(&mut policy,&mut f);
            let error=policy.validate(&f).unwrap_err();
            assert_eq!(error.kind(),io::ErrorKind::PermissionDenied);
            assert_eq!(error.raw_os_error(),None);
            assert_eq!(error.to_string(),"A1 original security admission rejected");
            assert_eq!(policy.first_rejection(&f),Some(reason));
        }
    }
    #[test] fn diagnostic_mirror_is_ordered_and_not_an_admission_or_runner_query_alias() {
        use WindowsTargetPolicyRejection as R;
        let (policy,mut f)=facts();assert_eq!(policy.first_rejection(&f),None);
        assert!(policy.validate(&f).is_ok());f.mandatory_policy=3;
        assert!(policy.validate(&f).is_ok());assert_eq!(policy.first_rejection(&f),None);
        f.app_container=true;f.unsafe_privileges=1;
        assert_eq!(policy.first_rejection(&f),Some(R::AppContainer));
        f.final_stats.modified+=1;
        assert_eq!(policy.first_rejection(&f),Some(R::StatisticsChanged));
        let (policy,mut f)=facts();let logon=f.logon;let auth=f.initial.auth;
        f.granted=0x20008;assert!(policy.validate_runner(&f,logon,auth).is_ok());
        assert!(policy.validate(&f).is_err());
        assert_eq!(policy.first_rejection(&f),Some(R::RequiredGrantMissing));
    }
}
