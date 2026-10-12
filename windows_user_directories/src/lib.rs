//! Shared current-primary-token LocalAppData candidate for cooperating Windows consumers.
//! No caller environment, profile fallback, registry query or authority is accepted.
//! Common resolution removes the core/native algorithm split. Registered redirection,
//! physical alias identity and compatibility with older environment-based hosts remain OPEN.
#![cfg(windows)]
#![deny(unsafe_op_in_unsafe_fn)]
use std::{io, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
// Pure decoding only. A decoded string is not proof of token origin or filesystem identity.
pub fn decode_token_local_app_data(buffer: &[u16]) -> io::Result<String> {
    if buffer.is_empty() || buffer.len() > 32768 {
        return Err(invalid("native token folder buffer bound"));
    }
    let end = buffer.iter().position(|&unit| unit == 0)
        .ok_or_else(|| invalid("native token folder missing terminator"))?;
    if end == 0 {
        return Err(invalid("native token folder empty"));
    }
    let value = String::from_utf16(&buffer[..end])
        .map_err(|_| invalid("native token folder UTF-16"))?;
    // The sole input is this fixed placeholder. Literal percent characters in
    // an absolute directory name are valid and must not be rejected wholesale.
    if value.eq_ignore_ascii_case("%LOCALAPPDATA%") {
        return Err(invalid("native token folder unresolved"));
    }
    if !std::path::Path::new(&value).is_absolute() {
        return Err(invalid("native token folder absolute path"));
    }
    Ok(value)
}


fn expand_fixed_local_app_data(
    expand: impl FnOnce(&[u16], &mut [u16]) -> io::Result<()>,
) -> io::Result<String> {
    let input: Vec<u16> = "%LOCALAPPDATA%".encode_utf16().chain(Some(0)).collect();
    let mut output = vec![0u16; 32768];
    expand(&input, &mut output)?;
    decode_token_local_app_data(&output)
}
fn finish_resolution(result: io::Result<String>, closed: io::Result<()>) -> io::Result<String> {
    match (result, closed) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) | (Ok(_), Err(error)) => Err(error),
    }
}

pub fn current_process_local_app_data() -> io::Result<String> {
    use windows_sys::Win32::{
        Security::{TOKEN_QUERY, TOKEN_IMPERSONATE, TOKEN_DUPLICATE},
        System::Environment::ExpandEnvironmentStringsForUserW,
    };
    struct PrimaryToken(HANDLE);
    impl Drop for PrimaryToken {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: guard owns the successful OpenProcessToken handle.
                // This is unwind/early-return fallback, never a success claim.
                unsafe { CloseHandle(self.0); }
            }
        }
    }
    let mut raw_token = ptr::null_mut();
    // SAFETY: current-process pseudo handle and writable HANDLE storage are valid.
    // Query/impersonate/duplicate access is required by this read-only userenv API;
    // no token impersonation, duplication, privilege adjustment or logon occurs.
    if unsafe { OpenProcessToken(GetCurrentProcess(),
        TOKEN_QUERY | TOKEN_IMPERSONATE | TOKEN_DUPLICATE, &mut raw_token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if raw_token.is_null() {
        return Err(invalid("native token handle missing"));
    }
    let mut token = PrimaryToken(raw_token);
    let result = (|| {
        let value = expand_fixed_local_app_data(|input, output| {
            // SAFETY: live owned token, fixed NUL-terminated input and writable
            // output of exactly32768 WCHARs remain valid throughout this synchronous call.
            if unsafe { ExpandEnvironmentStringsForUserW(token.0, input.as_ptr(),
                output.as_mut_ptr(), output.len() as u32) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        })?;
        // Keep the existing-directory contract; this follows ordinary filesystem
        // semantics and does not establish canonical/physical namespace equivalence.
        if !std::path::Path::new(&value).is_dir() {
            return Err(invalid("native token folder directory"));
        }
        Ok(value)
    })();
    // Check normal close explicitly; RAII remains the unwind fallback. Never retry
    // a failed close and never return a successful folder after a close failure.
    let handle = std::mem::replace(&mut token.0, ptr::null_mut());
    // SAFETY: handle is the original exclusively-owned successful token handle.
    let closed = if unsafe { CloseHandle(handle) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    };
    finish_resolution(result, closed)
}

// These tests use in-memory buffers/errors only. They do not query token/userenv/FS.
#[cfg(test)]
mod contract_tests {
    use super::*;
    #[test]
    fn fixed_input_and_capacity_ignore_inherited_override_data() {
        let inherited_override = r"C:\synthetic-inherited\Local";
        let expected = r"C:\synthetic-token-result\Local";
        let value = expand_fixed_local_app_data(|input, output| {
            assert_eq!(input, "%LOCALAPPDATA%".encode_utf16().chain(Some(0)).collect::<Vec<_>>());
            assert_ne!(input, inherited_override.encode_utf16().chain(Some(0)).collect::<Vec<_>>());
            assert_eq!(output.len(), 32768);
            let encoded: Vec<_> = expected.encode_utf16().chain(Some(0)).collect();
            output[..encoded.len()].copy_from_slice(&encoded);
            Ok(())
        }).unwrap();
        assert_eq!(value, expected);
        assert_ne!(value, inherited_override);
    }
    #[test]
    fn expansion_failure_is_preserved_without_decoding_or_fallback() {
        let error = expand_fixed_local_app_data(|_, output| {
            output[0] = 0xd800;
            Err(io::Error::from_raw_os_error(5))
        }).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5));
    }
    #[test]
    fn success_requires_successful_normal_close() {
        let error = finish_resolution(Ok("C:/synthetic/Local".into()),
            Err(io::Error::from_raw_os_error(6))).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(6));
    }
    #[test]
    fn lookup_failure_remains_primary_when_close_also_fails() {
        let error = finish_resolution(Err(io::Error::from_raw_os_error(5)),
            Err(io::Error::from_raw_os_error(6))).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5));
    }
    #[test]
    fn successful_value_and_successful_close_preserve_exact_spelling() {
        let value = "C:/synthetic/100%Ready/研究/Local";
        assert_eq!(finish_resolution(Ok(value.into()), Ok(())).unwrap(), value);
    }
}
