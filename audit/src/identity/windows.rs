use std::{ffi::OsString, io, os::windows::ffi::OsStringExt, path::PathBuf};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Security::TOKEN_QUERY,
    System::Threading::{GetCurrentProcess, OpenProcessToken},
    UI::Shell::GetUserProfileDirectoryW,
};
struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: OpenProcessToken returned this owned token handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub(super) fn profile_dir() -> io::Result<PathBuf> {
    let mut raw = std::ptr::null_mut();
    // SAFETY: current process pseudo-handle and valid output pointer; query access only.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Token(raw);
    let mut length = 0;
    // SAFETY: null first buffer requests the required UTF-16 capacity.
    unsafe {
        GetUserProfileDirectoryW(token.0, std::ptr::null_mut(), &mut length);
    }
    if length == 0 || length > 32768 {
        return Err(io::Error::other("profile path size unavailable"));
    }
    let mut value = vec![0u16; length as usize];
    // SAFETY: buffer capacity equals the count passed to the API.
    if unsafe { GetUserProfileDirectoryW(token.0, value.as_mut_ptr(), &mut length) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let end = value
        .iter()
        .position(|c| *c == 0)
        .ok_or_else(|| io::Error::other("profile path terminator missing"))?;
    Ok(PathBuf::from(OsString::from_wide(&value[..end])))
}
