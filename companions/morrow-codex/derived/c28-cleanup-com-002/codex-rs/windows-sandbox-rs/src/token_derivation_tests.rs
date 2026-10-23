use super::*;
use std::ptr;
use windows_sys::Win32::Foundation::{ERROR_NO_TOKEN, GetHandleInformation, HANDLE_FLAG_INHERIT};
use windows_sys::Win32::Security::{
    DuplicateTokenEx, RevertToSelf, SecurityImpersonation, TokenImpersonation,
    TokenIntegrityLevel, TokenStatistics, TokenPrivileges, TOKEN_DEFAULT_DACL,
    TOKEN_IMPERSONATE, TOKEN_MANDATORY_LABEL,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentThread, OpenThreadToken, SetThreadToken,
};
const SE_GROUP_INTEGRITY: u32 = 0x20;

struct Token(HANDLE);
impl Token {
    fn close(mut self) {
        assert_ne!(unsafe { CloseHandle(self.0) }, 0);
        self.0 = 0;
    }
}
impl Drop for Token {
    fn drop(&mut self) {
        if self.0 != 0 { let _ = unsafe { CloseHandle(self.0) }; }
    }
}
struct Impersonation(bool);
impl Impersonation {
    fn finish(mut self) {
        assert_ne!(unsafe { RevertToSelf() }, 0);
        self.0 = false;
    }
}
impl Drop for Impersonation {
    fn drop(&mut self) {
        if self.0 { let _ = unsafe { RevertToSelf() }; }
    }
}
unsafe fn info(token: HANDLE, kind: i32) -> Vec<u64> {
    let mut value = vec![0_u64; 8192];
    let mut size = 0;
    assert_ne!(GetTokenInformation(token, kind, value.as_mut_ptr().cast(),
        (value.len() * 8) as u32, &mut size), 0);
    value.truncate((size as usize).div_ceil(8));
    value
}
unsafe fn default_acl(token: HANDLE) -> Vec<u8> {
    let value = info(token, TokenDefaultDacl);
    let acl = (*(value.as_ptr() as *const TOKEN_DEFAULT_DACL)).DefaultDacl;
    assert!(!acl.is_null());
    std::slice::from_raw_parts(acl.cast(), (*acl).AclSize as usize).to_vec()
}
unsafe fn integrity(token: HANDLE) -> u32 {
    let value = info(token, TokenIntegrityLevel);
    let sid = (*(value.as_ptr() as *const TOKEN_MANDATORY_LABEL)).Label.Sid as *const u8;
    let count = *sid.add(1) as usize;
    std::ptr::read_unaligned(sid.add(8 + (count - 1) * 4).cast())
}

#[test]
fn medium_rc_only_source_derives_restricted_token_without_original_mutation() {
    unsafe {
        let mut previous = 0;
        assert_eq!(OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut previous), 0);
        assert_eq!(GetLastError(), ERROR_NO_TOKEN);
        let process_source = Token(open_current_token_for_query_duplicate().unwrap());
        let mut raw = 0;
        assert_ne!(DuplicateTokenEx(process_source.0,
            TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ADJUST_DEFAULT | TOKEN_IMPERSONATE,
            ptr::null(), SecurityImpersonation, TokenImpersonation, &mut raw), 0);
        let original = Token(raw);
        process_source.close();
        let starting_integrity = integrity(original.0);
        assert!(starting_integrity >= 0x2000);
        let user = crate::asuser_a1_policy::Sid::from_bytes(&get_user_sid_bytes(original.0).unwrap()).unwrap();
        let logon = crate::asuser_a1_policy::Sid::from_bytes(&get_logon_sid_bytes(original.0).unwrap()).unwrap();
        let mut template = crate::asuser_a1_security::default_template(user, logon).unwrap();
        let value = TOKEN_DEFAULT_DACL { DefaultDacl: template.as_mut_ptr().cast() };
        assert_ne!(SetTokenInformation(original.0, TokenDefaultDacl,
            (&value as *const TOKEN_DEFAULT_DACL).cast(), std::mem::size_of_val(&value) as u32), 0);
        let medium = LocalSid::from_string("S-1-16-8192").unwrap();
        let label = TOKEN_MANDATORY_LABEL { Label: SID_AND_ATTRIBUTES {
            Sid: medium.as_ptr(), Attributes: SE_GROUP_INTEGRITY,
        }};
        assert_ne!(SetTokenInformation(original.0, TokenIntegrityLevel,
            (&label as *const TOKEN_MANDATORY_LABEL).cast(), std::mem::size_of_val(&label) as u32 + 12), 0);
        let default_before = default_acl(original.0);
        let statistics_before = info(original.0, TokenStatistics);
        let privileges_before = info(original.0, TokenPrivileges);
        let groups_before = token_groups(original.0, 65536).unwrap();
        assert_eq!(default_before, template);
        assert_ne!(SetThreadToken(ptr::null(), original.0), 0);
        let impersonation = Impersonation(true);
        let mut raw = 0;
        assert_ne!(OpenThreadToken(GetCurrentThread(), TOKEN_QUERY | TOKEN_DUPLICATE, 0, &mut raw), 0);
        let readonly = Token(raw);
        // With the high object label, the old overbroad open fails. Both cases
        // still exercise actual DuplicateTokenEx and actual restricted tokens.
        let mut legacy = 0;
        let old_ok = OpenThreadToken(GetCurrentThread(), 0x1ab, 0, &mut legacy);
        let old_error = if old_ok == 0 { GetLastError() } else { Token(legacy).close(); 0 };
        if starting_integrity > 0x2000 { assert_eq!(old_error, 5); }
        let capability = LocalSid::from_string("S-1-5-21-10-20-30-40").unwrap();
        let restricted = Token(derive_uninitialized_restriction_with_caps_and_user_from(
            readonly.0, &[capability.as_ptr()], &[],
        ).unwrap());
        // The intermediate inherits only QUERY/DUPLICATE, so adjustment is
        // denied even though this is a newly owned object. No source is widened.
        let unchanged = TOKEN_DEFAULT_DACL { DefaultDacl: template.as_mut_ptr().cast() };
        assert_eq!(SetTokenInformation(restricted.0, TokenDefaultDacl,
            (&unchanged as *const TOKEN_DEFAULT_DACL).cast(), std::mem::size_of_val(&unchanged) as u32), 0);
        assert_eq!(GetLastError(), 5);
        let derived = Token(duplicate_token_for_restriction(restricted.0).unwrap());
        initialize_owned_restricted_token(derived.0).unwrap();
        assert_eq!(integrity(derived.0), 0x2000);
        let mut flags = 0;
        assert_ne!(GetHandleInformation(derived.0, &mut flags), 0);
        assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
        assert_eq!(integrity(restricted.0), 0x2000);
        assert_ne!(default_acl(derived.0), default_before);
        let restricting = info(derived.0, windows_sys::Win32::Security::TokenRestrictedSids);
        let groups = &*(restricting.as_ptr() as *const TOKEN_GROUPS);
        let entries = std::slice::from_raw_parts(groups.Groups.as_ptr(), groups.GroupCount as usize);
        assert!(entries.iter().any(|entry| windows_sys::Win32::Security::EqualSid(entry.Sid, capability.as_ptr()) != 0));
        assert!(info(derived.0, TokenPrivileges)[0] as u32 <= 1);
        assert_eq!(default_acl(original.0), default_before);
        assert_eq!(info(original.0, TokenStatistics), statistics_before);
        assert_eq!(info(original.0, TokenPrivileges), privileges_before);
        assert_eq!(token_groups(original.0, 65536).unwrap(), groups_before);
        restricted.close();
        derived.close();
        readonly.close();
        impersonation.finish();
        assert_eq!(OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut previous), 0);
        assert_eq!(GetLastError(), ERROR_NO_TOKEN);
        original.close();
        eprintln!("owned fixture: source IL {starting_integrity}, legacy open error {old_error}, derived IL 8192, five original handles checked closed");
    }
}
