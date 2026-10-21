//! Creation-only security for A1 child stdio. The trusted runner's token default
//! DACL remains RC-only; anonymous pipes need explicit data access for their own
//! logon. No existing DACL, token, window station or desktop is repaired here.
use anyhow::{Result, ensure};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_GENERIC_WRITE, READ_CONTROL};

pub(super) struct StdioSecurity {
    // A self-relative descriptor requires DWORD alignment. Its storage lives
    // through all original CreatePipe calls, without a LocalAlloc/free owner.
    descriptor: Vec<u32>,
}

impl StdioSecurity {
    pub(super) fn for_original_token(token: HANDLE) -> Result<Self> {
        let bytes = unsafe { crate::token::get_logon_sid_bytes(token)? };
        let logon = crate::asuser_a1_policy::Sid::from_bytes(&bytes)?;
        ensure!(logon.is_logon(), "stdio requires the original token logon SID");
        Self::from_logon(logon.bytes())
    }

    fn from_logon(logon: &[u8]) -> Result<Self> {
        ensure!(crate::asuser_a1_policy::Sid::from_bytes(logon)?.is_logon(),
            "stdio requires a logon SID");
        // SELF_RELATIVE | DACL_PRESENT | DACL_PROTECTED; owner/group are chosen
        // by the kernel. OWNER_RIGHTS suppresses implicit WRITE_DAC for that owner.
        let mut bytes = vec![1, 0, 0x04, 0x90];
        bytes.extend_from_slice(&[0; 12]);
        bytes.extend_from_slice(&20_u32.to_le_bytes());
        let acl_size = 8 + 8 + logon.len() + 8 + 12;
        bytes.extend_from_slice(&[2, 0]);
        bytes.extend_from_slice(&(acl_size as u16).to_le_bytes());
        bytes.extend_from_slice(&[2, 0, 0, 0]);
        let owner_rights = [1, 1, 0, 0, 0, 0, 0, 3, 4, 0, 0, 0];
        for (sid, mask) in [(logon, FILE_GENERIC_READ | FILE_GENERIC_WRITE),
            (owner_rights.as_slice(), READ_CONTROL)] {
            bytes.extend_from_slice(&[0, 0]);
            bytes.extend_from_slice(&((8 + sid.len()) as u16).to_le_bytes());
            bytes.extend_from_slice(&mask.to_le_bytes());
            bytes.extend_from_slice(sid);
        }
        ensure!(bytes.len() == 20 + acl_size && bytes.len() % 4 == 0,
            "invalid private stdio descriptor length");
        let descriptor = bytes.chunks_exact(4)
            .map(|word| u32::from_le_bytes([word[0],word[1],word[2],word[3]]))
            .collect();
        Ok(Self { descriptor })
    }

    pub(super) fn attributes(&mut self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor.as_mut_ptr().cast(),
            // The existing explicit child handle list separately selects stdio.
            bInheritHandle: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;
    use windows_sys::Win32::Foundation::{CloseHandle,GetLastError,ERROR_NO_TOKEN};
    use windows_sys::Win32::Security::{DuplicateTokenEx,GetTokenInformation,
        RevertToSelf,SetTokenInformation,
        SecurityImpersonation,TokenImpersonation,TokenDefaultDacl,TOKEN_DEFAULT_DACL,
        TOKEN_QUERY,TOKEN_DUPLICATE,TOKEN_ADJUST_DEFAULT,TOKEN_IMPERSONATE};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess,GetCurrentThread,
        OpenProcessToken,OpenThreadToken,SetThreadToken};
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::Storage::FileSystem::{ReadFile,WriteFile};

    struct Token(HANDLE);
    impl Token {
        fn close(mut self) { assert_ne!(unsafe{CloseHandle(self.0)},0); self.0=0; }
    }
    impl Drop for Token {
        fn drop(&mut self) { if self.0!=0 { let _=unsafe{CloseHandle(self.0)}; } }
    }
    struct Impersonation(bool);
    impl Impersonation {
        fn finish(mut self) { assert_ne!(unsafe{RevertToSelf()},0); self.0=false; }
    }
    impl Drop for Impersonation {
        fn drop(&mut self) { if self.0 { let _=unsafe{RevertToSelf()}; } }
    }
    unsafe fn default_acl(token:HANDLE)->Vec<u8> {
        let mut buffer=vec![0_u32;16384];let mut length=0;
        assert_ne!(GetTokenInformation(token,TokenDefaultDacl,buffer.as_mut_ptr().cast(),
            (buffer.len()*4) as u32,&mut length),0);
        let dacl=(*(buffer.as_ptr() as *const TOKEN_DEFAULT_DACL)).DefaultDacl;
        assert!(!dacl.is_null());
        std::slice::from_raw_parts(dacl.cast(),(*dacl).AclSize as usize).to_vec()
    }

    #[test]
    fn a1_stdio_round_trip_preserves_the_original_rc_only_default_dacl() {
        unsafe {
            let mut previous=0;
            assert_eq!(OpenThreadToken(GetCurrentThread(),TOKEN_QUERY,1,&mut previous),0);
            assert_eq!(GetLastError(),ERROR_NO_TOKEN);
            let mut raw=0;
            assert_ne!(OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY|TOKEN_DUPLICATE,&mut raw),0);
            let base=Token(raw);
            let mut raw=0;
            assert_ne!(DuplicateTokenEx(base.0,TOKEN_QUERY|TOKEN_ADJUST_DEFAULT|TOKEN_IMPERSONATE,
                ptr::null(),SecurityImpersonation,TokenImpersonation,&mut raw),0);
            let duplicate=Token(raw);base.close();
            let user=crate::asuser_a1_policy::Sid::from_bytes(
                &crate::token::get_user_sid_bytes(duplicate.0).unwrap()).unwrap();
            let logon=crate::asuser_a1_policy::Sid::from_bytes(
                &crate::token::get_logon_sid_bytes(duplicate.0).unwrap()).unwrap();
            let mut template=crate::asuser_a1_security::default_template(user,logon).unwrap();
            let value=TOKEN_DEFAULT_DACL{DefaultDacl:template.as_mut_ptr().cast()};
            assert_ne!(SetTokenInformation(duplicate.0,TokenDefaultDacl,(&value as *const TOKEN_DEFAULT_DACL).cast(),
                std::mem::size_of::<TOKEN_DEFAULT_DACL>() as u32),0);
            let before=default_acl(duplicate.0);
            assert_eq!(before,template);
            let mut security=StdioSecurity::for_original_token(duplicate.0).unwrap();
            let attributes=security.attributes();
            assert_eq!(attributes.bInheritHandle,0);
            assert_ne!(SetThreadToken(ptr::null(),duplicate.0),0);
            let impersonation=Impersonation(true);
            for _ in 0..3 {
                let(mut input,mut output)=(0,0);
                assert_ne!(CreatePipe(&mut input,&mut output,&attributes,0),0);
                let input=Token(input);let output=Token(output);
                let data=[0_u8,255,42,9];let mut written=0;
                assert_ne!(WriteFile(output.0,data.as_ptr(),4,&mut written,ptr::null_mut()),0);
                assert_eq!(written,4);
                let mut result=[0_u8;4];let mut read=0;
                assert_ne!(ReadFile(input.0,result.as_mut_ptr(),4,&mut read,ptr::null_mut()),0);
                assert_eq!(read,4);assert_eq!(data,result);
                input.close();output.close();
            }
            assert_eq!(default_acl(duplicate.0),before);
            impersonation.finish();
            let mut previous=0;
            assert_eq!(OpenThreadToken(GetCurrentThread(),TOKEN_QUERY,1,&mut previous),0);
            assert_eq!(GetLastError(),ERROR_NO_TOKEN);
            duplicate.close();
        }
    }

    #[test]
    fn stdio_descriptor_rejects_non_logon_sids_and_has_no_security_mutation_access() {
        assert!(StdioSecurity::from_logon(&[1,1,0,0,0,0,0,1,0,0,0,0]).is_err());
        assert_eq!((FILE_GENERIC_READ|FILE_GENERIC_WRITE)&(0x40000|0x80000|0x1000000),0);
    }
}
