//! Explicit disposable-guest qualification input. This only removes one
//! existing privilege from this original C process token. It cannot add a
//! privilege, edit LSA policy, select another process or mint a refusal proof.
use super::*;
use windows_sys::Win32::Foundation::{LUID,SetLastError};
use windows_sys::Win32::Security::{AdjustTokenPrivileges,GetTokenInformation,LookupPrivilegeValueW,
    TokenPrivileges,TokenStatistics,TOKEN_PRIVILEGES,TOKEN_STATISTICS,LUID_AND_ATTRIBUTES,
    TOKEN_ADJUST_PRIVILEGES,TOKEN_QUERY,SE_PRIVILEGE_REMOVED};
use windows_sys::Win32::System::Threading::OpenProcessToken;

#[derive(serde::Serialize)]
pub struct CallerRefusalQualification {
    pub process_id:u32,
    pub privilege:&'static str,
    pub present_before:bool,
    pub present_after:bool,
    pub other_privileges_unchanged:bool,
    pub original_query_adjust_handle_closed:bool,
    pub lsa_policy_changed:bool,
    pub execution_or_cleanup_authority:bool,
}
pub(super) struct Fixture {
    token:Slot,attempted:bool,completed:bool,buffer:Box<[u64;512]>,
}
impl Fixture {
    pub(super) fn new()->Self {Self{token:Slot::new(CloseKind::Kernel),attempted:false,completed:false,buffer:Box::new([0;512])}}
    pub(super) fn ready_for_origin_close(&self)->bool {
        (!self.attempted || self.completed) && self.token.retired()
    }
    fn privileges(&mut self)->io::Result<Vec<(u64,u32)>> {
        self.buffer.fill(0);let mut length=0;
        check(unsafe{GetTokenInformation(self.token.raw,TokenPrivileges,self.buffer.as_mut_ptr().cast(),4096,&mut length)})?;
        if length<4 || length>4096 {return Err(bad());}
        let bytes=unsafe{std::slice::from_raw_parts(self.buffer.as_ptr().cast::<u8>(),length as usize)};
        let count=u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        if count>128 || 4+count*12!=bytes.len(){return Err(bad());}
        let mut result=Vec::with_capacity(count);
        for item in bytes[4..].chunks_exact(12) {
            result.push((u64::from_le_bytes(item[..8].try_into().unwrap()),u32::from_le_bytes(item[8..].try_into().unwrap())));
        }
        result.sort_unstable();
        if result.windows(2).any(|p|p[0].0==p[1].0){return Err(bad());}
        Ok(result)
    }
    pub(super) fn remove_assign_once(&mut self,expected_token:u64,expected_auth:u64)->io::Result<CallerRefusalQualification> {
        if self.attempted || !self.ready_for_origin_close(){return Err(bad());}
        self.attempted=true;
        let deadline=Instant::now()+Duration::from_secs(2);
        let mut luid:LUID=unsafe{mem::zeroed()};
        let name=wide("SeAssignPrimaryTokenPrivilege");
        check(unsafe{LookupPrivilegeValueW(ptr::null(),name.as_ptr(),&mut luid)})?;
        remaining(deadline)?;
        self.token.begin()?;
        let ok=unsafe{OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY|TOKEN_ADJUST_PRIVILEGES,&mut self.token.raw)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        self.token.captured(ok!=0,self.token.raw,error)?;
        let mut statistics:TOKEN_STATISTICS=unsafe{mem::zeroed()};let mut length=0;
        check(unsafe{GetTokenInformation(self.token.raw,TokenStatistics,(&mut statistics as *mut TOKEN_STATISTICS).cast(),
            mem::size_of::<TOKEN_STATISTICS>() as u32,&mut length)})?;
        let luid_value=|value:LUID|u64::from(value.LowPart)|((value.HighPart as u32 as u64)<<32);
        if length as usize!=mem::size_of::<TOKEN_STATISTICS>() || statistics.TokenType!=1
            || luid_value(statistics.TokenId)!=expected_token || luid_value(statistics.AuthenticationId)!=expected_auth {return Err(bad());}
        let before=self.privileges()?;let selected=luid_value(luid);
        if !before.iter().any(|(id,attributes)|*id==selected && attributes&SE_PRIVILEGE_REMOVED==0){return Err(bad());}
        remaining(deadline)?;
        let change=TOKEN_PRIVILEGES{PrivilegeCount:1,Privileges:[LUID_AND_ATTRIBUTES{Luid:luid,Attributes:SE_PRIVILEGE_REMOVED}]};
        unsafe{SetLastError(0);}
        let ok=unsafe{AdjustTokenPrivileges(self.token.raw,0,&change,0,ptr::null_mut(),ptr::null_mut())};
        let error=unsafe{GetLastError()};
        if ok==0 || error!=0 {return Err(io::Error::from_raw_os_error(error as i32));}
        remaining(deadline)?;
        let after=self.privileges()?;
        let expected=before.into_iter().filter(|(id,_)|*id!=selected).collect::<Vec<_>>();
        if after!=expected || after.iter().any(|(id,_)|*id==selected){return Err(bad());}
        remaining(deadline)?;self.token.close()?;remaining(deadline)?;self.completed=true;
        Ok(CallerRefusalQualification{process_id:std::process::id(),privilege:"SeAssignPrimaryTokenPrivilege",
            present_before:true,present_after:false,other_privileges_unchanged:true,
            original_query_adjust_handle_closed:true,lsa_policy_changed:false,execution_or_cleanup_authority:false})
    }
}
