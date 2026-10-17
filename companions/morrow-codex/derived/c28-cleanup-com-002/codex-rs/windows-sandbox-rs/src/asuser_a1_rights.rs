//! A1 rights derived from SDK names, never GENERIC_ALL/ALL_ACCESS.
//! These are source policy; native compatibility remains independently NOT_RUN.
use windows_sys::Win32::System::Threading::{PROCESS_READ_CONTROL,PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE,THREAD_READ_CONTROL,THREAD_QUERY_LIMITED_INFORMATION,THREAD_SYNCHRONIZE};
use windows_sys::Win32::Security::{TOKEN_QUERY,TOKEN_DUPLICATE,TOKEN_ASSIGN_PRIMARY,TOKEN_ADJUST_DEFAULT};
use windows_sys::Win32::Storage::FileSystem::{READ_CONTROL,WRITE_OWNER};
use windows_sys::Win32::System::StationsAndDesktops::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
pub(crate) const TOKEN_REQUIRED:u32=READ_CONTROL|TOKEN_QUERY|TOKEN_DUPLICATE|TOKEN_ASSIGN_PRIMARY;
pub(crate) const TOKEN_INITIALIZATION:u32=TOKEN_REQUIRED|TOKEN_ADJUST_DEFAULT;
pub(crate) const PROCESS_MANAGEMENT:u32=PROCESS_READ_CONTROL|PROCESS_QUERY_LIMITED_INFORMATION|PROCESS_SYNCHRONIZE;
pub(crate) const THREAD_MANAGEMENT:u32=THREAD_READ_CONTROL|THREAD_QUERY_LIMITED_INFORMATION|THREAD_SYNCHRONIZE;
// No QUERY on either trusted-R object: that could make its primary T reachable
// by hostile business descendants through OpenProcessToken/OpenThreadToken.
pub(crate) const PROCESS_TARGET:u32=PROCESS_SYNCHRONIZE;
pub(crate) const THREAD_TARGET:u32=THREAD_SYNCHRONIZE;
pub(crate) const OWNER_READ_CONTROL:u32=READ_CONTROL;
// S selects the new private station and creates one desktop; it reads security
// and may do the one authorized label lowering. No WRITE_DAC or SYNCHRONIZE.
pub(crate) const STATION_MANAGEMENT:u32=READ_CONTROL|WRITE_OWNER
    |WINSTA_ENUMDESKTOPS as u32|WINSTA_READATTRIBUTES as u32|WINSTA_CREATEDESKTOP as u32;
// CreateDesktop requires CREATEWINDOW. When standard RC/WO are requested the
// SDK contract also requires READOBJECTS and WRITEOBJECTS. ENUMERATE is not
// SWITCHDESKTOP, and neither is requested for S's management handle.
pub(crate) const DESKTOP_MANAGEMENT:u32=DESKTOP_READ_CONTROL|DESKTOP_WRITE_OWNER
    |DESKTOP_READOBJECTS|DESKTOP_WRITEOBJECTS|DESKTOP_CREATEWINDOW;
// R initialization gets only private-object operational rights, never DACL,
// owner, full-SACL management or standard generic access. Each station flag is
// kept explicit to avoid confusing GLOBALATOMS(0x20) with WRITEATTRIBUTES(0x10).
pub(crate) const STATION_TARGET:u32=WINSTA_ENUMDESKTOPS as u32|WINSTA_READATTRIBUTES as u32
    |WINSTA_ACCESSCLIPBOARD as u32|WINSTA_CREATEDESKTOP as u32|WINSTA_WRITEATTRIBUTES as u32
    |WINSTA_ACCESSGLOBALATOMS as u32|WINSTA_EXITWINDOWS as u32|WINSTA_ENUMERATE as u32|WINSTA_READSCREEN as u32;
pub(crate) const DESKTOP_TARGET:u32=DESKTOP_READOBJECTS|DESKTOP_CREATEWINDOW|DESKTOP_CREATEMENU
    |DESKTOP_HOOKCONTROL|DESKTOP_JOURNALRECORD|DESKTOP_JOURNALPLAYBACK|DESKTOP_ENUMERATE
    |DESKTOP_WRITEOBJECTS|DESKTOP_SWITCHDESKTOP;
// C pulls read-only aliases; it never lowers labels or changes GUI association.
pub(crate) const STATION_OBSERVE:u32=READ_CONTROL|WINSTA_READATTRIBUTES as u32;
pub(crate) const DESKTOP_OBSERVE:u32=DESKTOP_READ_CONTROL|DESKTOP_READOBJECTS;
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn target_never_receives_original_token_or_security_management_rights() {
        assert_eq!(PROCESS_TARGET&PROCESS_QUERY_LIMITED_INFORMATION,0);
        assert_eq!(THREAD_TARGET&THREAD_QUERY_LIMITED_INFORMATION,0);
        for mask in [STATION_TARGET,DESKTOP_TARGET,OWNER_READ_CONTROL] {
            assert_eq!(mask&(0x00040000|0x00080000|0x01000000|0xf0000000),0);
        }
        assert_eq!(STATION_MANAGEMENT&0x00040000,0);assert_eq!(DESKTOP_MANAGEMENT&0x00040000,0);
        assert_eq!(STATION_OBSERVE&WRITE_OWNER,0);assert_eq!(DESKTOP_OBSERVE&DESKTOP_WRITE_OWNER,0);
    }
}

