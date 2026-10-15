//! Copies token user identities with bounded Windows queries and owned SID storage.

use anyhow::Result;
use anyhow::anyhow;
use anyhow::ensure;
use std::ffi::c_void;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Security::GetTokenInformation;
use windows_sys::Win32::Security::IsValidSid;
use windows_sys::Win32::Security::TOKEN_USER;
use windows_sys::Win32::Security::TokenUser;

/// Copies a token's user SID into owned storage after a bounded TokenUser query.
///
/// # Safety
/// The caller must keep a token handle with TOKEN_QUERY access valid during this call.
pub unsafe fn get_user_sid_bytes(h_token: HANDLE) -> Result<Vec<u8>> {
    let mut needed: u32 = 0;
    GetTokenInformation(h_token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
    ensure!(
        needed as usize >= std::mem::size_of::<TOKEN_USER>() && needed <= 4096,
        "invalid TokenUser query size"
    );
    let mut user_buf: Vec<u8> = vec![0u8; needed as usize];
    let ok = GetTokenInformation(
        h_token,
        TokenUser,
        user_buf.as_mut_ptr() as *mut c_void,
        needed,
        &mut needed,
    );
    if ok == 0 {
        return Err(anyhow!(
            "GetTokenInformation(TokenUser) failed: {}",
            GetLastError()
        ));
    }
    ensure!(
        needed as usize <= user_buf.len(),
        "invalid TokenUser result size"
    );
    decode_token_user(&user_buf[..needed as usize])
}

fn decode_token_user(buffer: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        buffer.len() >= std::mem::size_of::<TOKEN_USER>(),
        "truncated TokenUser"
    );
    let user = unsafe { std::ptr::read_unaligned(buffer.as_ptr().cast::<TOKEN_USER>()) };
    copy_token_sid(buffer, user.User.Sid)
}

fn copy_token_sid(buffer: &[u8], pointer: *mut c_void) -> Result<Vec<u8>> {
    // Bound the header and every subauthority before calling a SID API.
    let offset = (pointer as usize).wrapping_sub(buffer.as_ptr() as usize);
    ensure!(
        buffer.len() >= 8 && offset <= buffer.len() - 8,
        "invalid token SID pointer"
    );
    ensure!(buffer[offset] == 1, "invalid token SID revision");
    let length = 8 + usize::from(buffer[offset + 1]) * 4;
    ensure!(
        length <= 68 && length <= buffer.len() - offset,
        "invalid token SID size"
    );
    ensure!(unsafe { IsValidSid(pointer) } != 0, "invalid token SID");
    Ok(buffer[offset..offset + length].to_vec())
}

/// Bounded public data about the calling PROCESS's primary token. No HANDLE,
/// expected identity, caller-supplied token, process selector or authority.
#[derive(Clone,Debug)]
pub struct CurrentPrimaryTokenIdentity {
    user_sid:String,
    token_type:u32,
    has_restrictions:bool,
    elevated:bool,
}
impl CurrentPrimaryTokenIdentity {
    pub fn user_sid(&self)->&str {&self.user_sid}
    pub fn is_primary(&self)->bool {self.token_type==windows_sys::Win32::Security::TokenPrimary as u32}
    pub fn has_restrictions(&self)->bool {self.has_restrictions}
    pub fn is_elevated(&self)->bool {self.elevated}
}
fn sid_text(sid:&[u8])->Result<String> {
    ensure!(sid.len()>=8 && sid[0]==1 && sid[1]<=15,"invalid owned SID header");
    ensure!(sid.len()==8+usize::from(sid[1])*4,"invalid owned SID length");
    let authority=u64::from_be_bytes([0,0,sid[2],sid[3],sid[4],sid[5],sid[6],sid[7]]);
    let mut text=format!("S-1-{authority}");
    for part in sid[8..].chunks_exact(4) {
        text.push('-');text.push_str(&u32::from_le_bytes(part.try_into()?).to_string());
    }
    Ok(text)
}
// Windows SDK GetCurrentProcessToken() borrowed pseudo-handle, not a numeric
// owned handle supplied by an external caller or an external PID lookup.
const CURRENT_PROCESS_PRIMARY_TOKEN:HANDLE=-4;
fn current_primary_dword(class:windows_sys::Win32::Security::TOKEN_INFORMATION_CLASS)->Result<u32> {
    let mut value=0u32;let mut needed=0u32;
    // Windows SDK GetCurrentProcessToken() pseudo-handle (-4):
    // always THIS process primary token, even on an impersonating thread.
    // Borrowed, query-only; never wrap/duplicate/close it. Supported by the
    // fixed Windows 11 guest (the SDK contract is Windows 8.1 and later).
    if unsafe{GetTokenInformation(CURRENT_PROCESS_PRIMARY_TOKEN,class,(&mut value as *mut u32).cast(),4,&mut needed)}==0 {
        return Err(std::io::Error::last_os_error().into());
    }
    ensure!(needed==4,"invalid fixed primary-token scalar size");Ok(value)
}
fn current_primary_has_restrictions()->Result<bool> {
    // TokenHasRestrictions returns a native BOOLEAN (one byte), unlike the
    // DWORD-sized TokenType and TOKEN_ELEVATION results. The output pointer
    // nevertheless needs DWORD alignment. Capacity4 below is bounded aligned
    // storage, not a claim that the effective result/minimum capacity is4.
    let mut storage=0xA5A5A5A5u32;let mut needed=u32::MAX;
    if unsafe{GetTokenInformation(CURRENT_PROCESS_PRIMARY_TOKEN,
        windows_sys::Win32::Security::TokenHasRestrictions,
        (&mut storage as *mut u32).cast(),4,&mut needed)}==0 {
        return Err(std::io::Error::last_os_error().into());
    }
    ensure!(needed==1,"invalid primary-token restriction BOOLEAN size");
    // Read only the first returned byte. The initialized high three sentinel
    // bytes are storage, never a DWORD token result. This pointer is valid for
    // one byte for the lifetime of this same initialized aligned local.
    let value=unsafe{(&storage as *const u32).cast::<u8>().read()};
    ensure!(value<=1,"invalid primary-token restriction BOOLEAN value");
    Ok(value==1)
}
/// Reads ONLY the current process primary token. No child process, thread-token
/// fallback, external PID, token mutation, privilege adjustment or handle lease.
/// The result is observation data, never a native/SDK/cleanup capability.
pub fn current_process_primary_token_identity()->Result<CurrentPrimaryTokenIdentity> {
    use windows_sys::Win32::Security::{TokenType,TokenPrimary,TokenElevation};
    let token_type=current_primary_dword(TokenType)?;
    ensure!(token_type==TokenPrimary as u32,"current process token is not primary");
    let sid=unsafe{get_user_sid_bytes(CURRENT_PROCESS_PRIMARY_TOKEN)}?;
    let restrictions=current_primary_has_restrictions()?;
    let elevated=current_primary_dword(TokenElevation)?;
    ensure!(elevated<=1,"invalid primary-token elevation scalar");
    Ok(CurrentPrimaryTokenIdentity {user_sid:sid_text(&sid)?,token_type,has_restrictions:restrictions,elevated:elevated==1})
}

#[cfg(test)]
#[path = "token_user_tests.rs"]
mod tests;
