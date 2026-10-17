use crate::allow::compute_allow_paths_for_permissions;
use crate::deny_read_acl::plan_deny_read_acl_paths;
use crate::logging;
use crate::path_normalization::canonicalize_path;
use crate::resolved_permissions::ResolvedWindowsSandboxPermissions;
use crate::setup::SandboxSetupRequest;
use crate::setup::SetupRootOverrides;
use crate::setup::build_payload_deny_write_paths;
use crate::setup::build_payload_roots;
use crate::setup::gather_read_roots;
use crate::spawn_prep::LegacySessionSecurity;
use crate::token::get_current_token_for_restriction;
use crate::token::get_logon_sid_bytes;
use crate::token::get_user_sid_bytes;
use crate::winutil::format_last_error;
use crate::winutil::resolve_sid;
use crate::winutil::sid_bytes_from_string;
use crate::winutil::string_from_sid_bytes;
use crate::winutil::to_wide;
use anyhow::Result;
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;
use std::path::PathBuf;
use std::ptr;
use std::sync::Mutex;
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::HLOCAL;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows_sys::Win32::Security::Authorization::EXPLICIT_ACCESS_W;
use windows_sys::Win32::Security::Authorization::GRANT_ACCESS;
use windows_sys::Win32::Security::Authorization::SE_WINDOW_OBJECT;
use windows_sys::Win32::Security::Authorization::SetEntriesInAclW;
use windows_sys::Win32::Security::Authorization::SetSecurityInfo;
use windows_sys::Win32::Security::Authorization::TRUSTEE_IS_SID;
use windows_sys::Win32::Security::Authorization::TRUSTEE_IS_UNKNOWN;
use windows_sys::Win32::Security::Authorization::TRUSTEE_W;
use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
use windows_sys::Win32::Security::PSECURITY_DESCRIPTOR;
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::StationsAndDesktops::CloseDesktop;
use windows_sys::Win32::System::StationsAndDesktops::CreateDesktopW;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_CREATEMENU;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_CREATEWINDOW;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_DELETE;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_ENUMERATE;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_HOOKCONTROL;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_JOURNALPLAYBACK;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_JOURNALRECORD;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_READ_CONTROL;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_READOBJECTS;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_SWITCHDESKTOP;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_WRITE_DAC;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_WRITE_OWNER;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_WRITEOBJECTS;
use windows_sys::Win32::System::StationsAndDesktops::GetProcessWindowStation;
use windows_sys::Win32::System::StationsAndDesktops::GetUserObjectInformationW;
use windows_sys::Win32::System::StationsAndDesktops::OpenDesktopW;
use windows_sys::Win32::System::StationsAndDesktops::UOI_NAME;

const PRIVATE_DESKTOP_PREFIX: &str = "CodexSandboxDesktop-";

const DESKTOP_ALL_ACCESS: u32 = DESKTOP_READOBJECTS
    | DESKTOP_CREATEWINDOW
    | DESKTOP_CREATEMENU
    | DESKTOP_HOOKCONTROL
    | DESKTOP_JOURNALRECORD
    | DESKTOP_JOURNALPLAYBACK
    | DESKTOP_ENUMERATE
    | DESKTOP_WRITEOBJECTS
    | DESKTOP_SWITCHDESKTOP
    | DESKTOP_DELETE
    | DESKTOP_READ_CONTROL
    | DESKTOP_WRITE_DAC
    | DESKTOP_WRITE_OWNER;

const DESKTOP_PARTICIPANT_ACCESS: u32 =
    DESKTOP_ALL_ACCESS & !(DESKTOP_WRITE_DAC | DESKTOP_WRITE_OWNER | DESKTOP_DELETE);

/// A1 alone sends a qualified name through the unchanged private-desktop
/// request field. Its original AsUser STARTUPINFO already selected this exact
/// station. The name is correlation, never authority to change stations or to
/// adopt an old desktop. A malformed/foreign A1 name has no legacy fallback.
fn asuser_a1_desktop_parts(name: &str) -> Result<Option<(&str, &str)>> {
    if !name.starts_with("C28StationA1-") { return Ok(None); }
    let (station, desktop) = name.split_once('\\').ok_or_else(|| anyhow::anyhow!("invalid A1 qualified desktop"))?;
    let station_tail=station.strip_prefix("C28StationA1-").ok_or_else(|| anyhow::anyhow!("invalid A1 station"))?;
    let (generation,suffix)=station_tail.split_once('-').ok_or_else(|| anyhow::anyhow!("invalid A1 generation"))?;
    if generation.is_empty() || generation.len()>20 || generation.starts_with('0')
        || !generation.bytes().all(|byte|byte.is_ascii_digit())
        || generation.parse::<u64>().ok().filter(|value|*value!=0).is_none()
        || suffix.len()!=32 || !suffix.bytes().all(|byte|byte.is_ascii_digit()||(b'a'..=b'f').contains(&byte))
        || desktop.strip_prefix("C28DesktopA1-")!=Some(suffix) {
        anyhow::bail!("invalid A1 qualified desktop");
    }
    Ok(Some((station,desktop)))
}

#[cfg(test)]
mod asuser_a1_name_tests {
    use super::asuser_a1_desktop_parts;
    #[test] fn qualified_a1_name_binds_one_canonical_generation_and_the_same_nonce_suffix() {
        let suffix="0123456789abcdef0123456789abcdef";
        for generation in ["1","18446744073709551615"] {
            let station=format!("C28StationA1-{generation}-{suffix}");
            let desktop=format!("C28DesktopA1-{suffix}");
            let qualified=format!("{station}\\{desktop}");
            assert_eq!(asuser_a1_desktop_parts(&qualified).unwrap(),Some((station.as_str(),desktop.as_str())));
        }
        for generation in ["","0","01","-1","+1","18446744073709551616"] {
            let name=format!("C28StationA1-{generation}-{suffix}\\C28DesktopA1-{suffix}");
            assert!(asuser_a1_desktop_parts(&name).is_err());
        }
        for name in [format!("C28StationA1-1-{suffix}"),
            format!("C28StationA1-1-{suffix}\\Default"),
            format!("C28StationA1-1-{suffix}\\C28DesktopA1-{}","ab".repeat(16)),
            format!("C28StationA1-1-{suffix}\\C28DesktopA1-{suffix}\\extra"),
            format!("C28StationA1-1-{}\\C28DesktopA1-{}",suffix.to_uppercase(),suffix.to_uppercase())] {
            assert!(asuser_a1_desktop_parts(&name).is_err());
        }
        assert_eq!(asuser_a1_desktop_parts("CodexSandboxDesktop-0123456789abcdef").unwrap(),None);
        assert_eq!(asuser_a1_desktop_parts("Default").unwrap(),None);
    }
}

static SHARED_PRIVATE_DESKTOPS: OnceLock<Mutex<HashMap<(String, DesktopPolicy), PrivateDesktop>>> =
    OnceLock::new();

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct DesktopPolicy {
    uses_write_capabilities: bool,
    capability_sids: BTreeSet<Vec<u8>>,
    network_enabled: bool,
    network_proxy_restricting_sid: Option<Vec<u8>>,
    // None denotes the legacy backend's unrestricted reads and different desktop ACLs.
    read_roots: Option<BTreeSet<PathBuf>>,
    write_roots: BTreeSet<PathBuf>,
    deny_read_paths: BTreeSet<PathBuf>,
    deny_write_paths: BTreeSet<PathBuf>,
}

impl DesktopPolicy {
    pub(crate) fn elevated(
        request: SandboxSetupRequest<'_>,
        mut overrides: SetupRootOverrides,
        capability_sids: &[String],
        network_proxy_restricting_sid: Option<&str>,
    ) -> Result<Self> {
        // Match the complete read override passed by credential setup to the ACL helper.
        let runtime = crate::setup::current_setup_runtime();
        overrides.read_roots.get_or_insert_with(|| {
            gather_read_roots(
                request.command_cwd,
                request.permissions,
                request.env_map,
                request.codex_home,
                runtime,
            )
        });
        let (read_roots, write_roots) = build_payload_roots(&request, &overrides, runtime);
        Ok(Self {
            uses_write_capabilities: request
                .permissions
                .uses_write_capabilities_for_cwd(request.command_cwd, request.env_map),
            capability_sids: capability_sids
                .iter()
                .map(|sid| sid_bytes_from_string(sid))
                .collect::<Result<_>>()?,
            network_enabled: request.permissions.network_policy().is_enabled(),
            network_proxy_restricting_sid: network_proxy_restricting_sid
                .map(sid_bytes_from_string)
                .transpose()?,
            read_roots: Some(read_roots.into_iter().collect()),
            write_roots: write_roots.into_iter().collect(),
            deny_read_paths: plan_deny_read_acl_paths(
                overrides.deny_read_paths.as_deref().unwrap_or_default(),
            )?
            .into_iter()
            .collect(),
            deny_write_paths: build_payload_deny_write_paths(&request, overrides.deny_write_paths)
                .into_iter()
                .map(|path| canonicalize_path(&path))
                .collect(),
        })
    }
}

// A bounded name query only: this never changes the process window station or its ACL.
const MAX_WINDOW_STATION_NAME_UNITS: usize = 512;

fn window_station_name_units(reported_bytes: u32) -> Result<usize> {
    let bytes = reported_bytes as usize;
    anyhow::ensure!(
        bytes >= 2 * std::mem::size_of::<u16>()
            && bytes % std::mem::size_of::<u16>() == 0
            && bytes <= MAX_WINDOW_STATION_NAME_UNITS * std::mem::size_of::<u16>(),
        "invalid window station name length"
    );
    Ok(bytes / std::mem::size_of::<u16>())
}

fn decode_window_station_name(buffer: &[u16], reported_bytes: u32) -> Result<String> {
    let units = window_station_name_units(reported_bytes)?;
    anyhow::ensure!(buffer.len() == units, "window station name length changed");
    anyhow::ensure!(buffer.last() == Some(&0), "window station name is not terminated");
    let name_units = &buffer[..units - 1];
    anyhow::ensure!(!name_units.contains(&0), "window station name contains an embedded NUL");
    let name = String::from_utf16(name_units)?;
    anyhow::ensure!(
        !name.chars().any(|ch| ch == '\\' || ch == '/' || ch.is_control()),
        "invalid window station name character"
    );
    Ok(name)
}

fn current_window_station_name() -> Result<String> {
    current_window_station().map(|(_, name)| name)
}
fn current_window_station() -> Result<(isize, String)> {
    // This is a borrowed system handle; GetProcessWindowStation forbids closing it.
    let station = unsafe { GetProcessWindowStation() };
    anyhow::ensure!(station != 0, "GetProcessWindowStation failed: {}", unsafe { GetLastError() });
    let mut needed = 0;
    let size_result = unsafe {
        GetUserObjectInformationW(station, UOI_NAME, ptr::null_mut(), 0, &mut needed)
    };
    let size_error = unsafe { GetLastError() };
    anyhow::ensure!(
        size_result == 0 && size_error == ERROR_INSUFFICIENT_BUFFER,
        "window station name size query failed: {size_error}"
    );
    let units = window_station_name_units(needed)?;
    let mut buffer = vec![u16::MAX; units];
    let mut returned = 0;
    let result = unsafe {
        GetUserObjectInformationW(station, UOI_NAME, buffer.as_mut_ptr().cast(), needed, &mut returned)
    };
    anyhow::ensure!(result != 0, "window station name query failed: {}", unsafe { GetLastError() });
    anyhow::ensure!(returned == needed, "window station name length changed");
    Ok((station, decode_window_station_name(&buffer, returned)?))
}

fn qualified_desktop_name(station_name: &str, desktop_name: &str) -> Vec<u16> {
    to_wide(format!("{station_name}\\{desktop_name}"))
}

pub struct LaunchDesktop {
    _private_desktop: PrivateDesktop,
    startup_name: Vec<u16>,
}

impl LaunchDesktop {
    pub(crate) fn prepare_legacy(
        permissions: &ResolvedWindowsSandboxPermissions,
        cwd: &Path,
        env: &HashMap<String, String>,
        security: &LegacySessionSecurity,
        additional_deny_write_paths: &[PathBuf],
        logs_base_dir: Option<&Path>,
    ) -> Result<Self> {
        Self::open_private(&Self::shared_legacy_name(
            permissions,
            cwd,
            env,
            security,
            additional_deny_write_paths,
            logs_base_dir,
        )?)
    }

    pub(crate) fn shared_legacy_name(
        permissions: &ResolvedWindowsSandboxPermissions,
        cwd: &Path,
        env: &HashMap<String, String>,
        security: &LegacySessionSecurity,
        additional_deny_write_paths: &[PathBuf],
        logs_base_dir: Option<&Path>,
    ) -> Result<String> {
        let sandbox_sid = unsafe { get_user_sid_bytes(security.h_token)? };
        let sandbox_sid = string_from_sid_bytes(&sandbox_sid).map_err(anyhow::Error::msg)?;
        let paths = compute_allow_paths_for_permissions(permissions, cwd, env);
        let policy = DesktopPolicy {
            uses_write_capabilities: security.readonly_sid.is_none(),
            capability_sids: security
                .readonly_sid_str
                .iter()
                .chain(security.write_root_sids.iter().map(|root| &root.sid_str))
                .map(|sid| sid_bytes_from_string(sid))
                .collect::<Result<_>>()?,
            network_enabled: permissions.network_policy().is_enabled(),
            network_proxy_restricting_sid: None,
            read_roots: None,
            write_roots: paths.allow.into_iter().collect(),
            deny_read_paths: BTreeSet::new(),
            deny_write_paths: paths
                .deny
                .into_iter()
                .chain(additional_deny_write_paths.iter().cloned())
                .map(|path| canonicalize_path(&path))
                .collect(),
        };
        let mut desktops = SHARED_PRIVATE_DESKTOPS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .map_err(|_| anyhow::anyhow!("shared private desktop cache was poisoned"))?;
        let desktop = match desktops.entry((sandbox_sid, policy)) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(PrivateDesktop::create(logs_base_dir)?)
            }
        };
        Ok(desktop.name.clone())
    }

    pub fn prepare(logs_base_dir: Option<&Path>) -> Result<Self> {
        let station_name = current_window_station_name()?;
        let private_desktop = PrivateDesktop::create(logs_base_dir)?;
        let startup_name = qualified_desktop_name(&station_name, &private_desktop.name);
        Ok(Self {
            _private_desktop: private_desktop,
            startup_name,
        })
    }

    /// Opens the caller-owned private desktop without creating one or falling back to Default.
    pub fn open_private(name: &str) -> Result<Self> {
        if let Some((station,desktop))=asuser_a1_desktop_parts(name)? {
            // No SetProcessWindowStation/OpenWindowStation, requested RC, or
            // label/DACL repair. The actual existing station must agree before
            // the one new desktop alias is opened. No wire/schema change.
            if current_window_station_name()?!=station {
                anyhow::bail!("A1 desktop differs from the original AsUser station");
            }
            let startup_name=qualified_desktop_name(station,desktop);
            let name_wide=to_wide(desktop);
            let handle=unsafe {OpenDesktopW(name_wide.as_ptr(),0,0,crate::asuser_a1_rights::DESKTOP_TARGET)};
            if handle==0 {anyhow::bail!("A1 OpenDesktopW failed: {}",unsafe{GetLastError()});}
            return Ok(Self{_private_desktop:PrivateDesktop{handle,name:desktop.to_owned()},startup_name});
        }
        if !name
            .strip_prefix(PRIVATE_DESKTOP_PREFIX)
            .is_some_and(|nonce| {
                !nonce.is_empty()
                    && nonce.len() <= 32
                    && nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            anyhow::bail!("invalid private desktop name");
        }
        // Resolve and validate the complete startup name before opening a new desktop handle.
        let station_name = current_window_station_name()?;
        let startup_name = qualified_desktop_name(&station_name, name);
        let name_wide = to_wide(name);
        let handle = unsafe {
            OpenDesktopW(
                name_wide.as_ptr(),
                /*dwflags*/ 0,
                /*finherit*/ 0,
                DESKTOP_PARTICIPANT_ACCESS,
            )
        };
        if handle == 0 {
            anyhow::bail!("OpenDesktopW failed: {}", unsafe { GetLastError() });
        }
        Ok(Self {
            _private_desktop: PrivateDesktop {
                handle,
                name: name.to_owned(),
            },
            startup_name,
        })
    }

    pub fn startup_info_desktop(&self) -> *mut u16 {
        self.startup_name.as_ptr() as *mut u16
    }
}

/// Reuses a private desktop only for the same sandbox account and effective permissions.
pub(crate) fn shared_private_desktop_for_user(
    sandbox_username: &str,
    policy: &DesktopPolicy,
    logs_base_dir: Option<&Path>,
) -> Result<String> {
    let sandbox_sid =
        string_from_sid_bytes(&resolve_sid(sandbox_username)?).map_err(anyhow::Error::msg)?;
    let mut desktops = SHARED_PRIVATE_DESKTOPS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| anyhow::anyhow!("shared private desktop cache was poisoned"))?;
    let key = (sandbox_sid.clone(), policy.clone());
    if let Some(desktop) = desktops.get(&key) {
        return Ok(desktop.name.clone());
    }

    let desktop = create_private_desktop_for_sid(&sandbox_sid,logs_base_dir)?;
    let name=desktop.name.clone();
    desktops.insert(key,desktop);
    Ok(name)
}

// Shared legacy and one-invocation v8 desktops use the same existing DACL.
// The v8 instance never enters SHARED_PRIVATE_DESKTOPS.
fn create_private_desktop_for_sid(sandbox_sid:&str,logs_base_dir:Option<&Path>)->Result<PrivateDesktop> {

    let owner_user_sid = unsafe {
        let token = get_current_token_for_restriction()?;
        let sid = get_user_sid_bytes(token);
        CloseHandle(token);
        sid?
    };
    let owner_user_sid = string_from_sid_bytes(&owner_user_sid).map_err(anyhow::Error::msg)?;
    create_private_desktop_for_sids(sandbox_sid,&owner_user_sid,logs_base_dir,None)?
        .ok_or_else(||anyhow::anyhow!("shared desktop creation owner missing"))
}
fn create_private_desktop_for_sids(sandbox_sid:&str,owner_user_sid:&str,logs_base_dir:Option<&Path>,
    mut retained:Option<&mut OwnedLaunchDesktop>)->Result<Option<PrivateDesktop>> {
    // Keep the existing caller-user management / target-user participant policy.
    // Caller/target logon-SID equality is not an invariant established here;
    // the v8 original-token diagnostic reports the observed relation separately.
    // https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createprocesswithlogonw
    let sddl = to_wide(format!(
        "D:P(A;;0x{DESKTOP_ALL_ACCESS:x};;;{owner_user_sid})(A;;0x{DESKTOP_PARTICIPANT_ACCESS:x};;;{sandbox_sid})"
    ));
    let mut security_descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            /*stringsdrevision*/ 1,
            &mut security_descriptor,
            ptr::null_mut(),
        )
    } == 0
    {
        anyhow::bail!(
            "ConvertStringSecurityDescriptorToSecurityDescriptorW failed: {}",
            unsafe { GetLastError() }
        );
    }
    if let Some(owner)=retained.as_deref_mut(){owner.security_descriptor=Some(security_descriptor as isize);}

    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security_descriptor,
        bInheritHandle: 0,
    };
    let mut rng = SmallRng::from_entropy();
    let name = format!("{PRIVATE_DESKTOP_PREFIX}{:032x}", rng.r#gen::<u128>());
    let name_wide = to_wide(&name);
    let handle = unsafe {
        CreateDesktopW(
            name_wide.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            /*dwflags*/ 0,
            DESKTOP_ALL_ACCESS,
            &attributes,
        )
    };
    let error = unsafe { GetLastError() };
    if let Some(owner)=retained.as_deref_mut(){
        if handle!=0 {owner.desktop=Some(std::mem::ManuallyDrop::new(PrivateDesktop {handle,name:name.clone()}));}
        // The actual descriptor and any newly created desktop were anchored
        // before checking LocalFree; an uncertain free is retained, never retried.
        if !unsafe{LocalFree(security_descriptor as HLOCAL)}.is_null() {
            let close_error=std::io::Error::last_os_error();
            owner.descriptor_close_error=Some(close_error.raw_os_error().unwrap_or(0));
            if handle!=0{return Err(close_error.into());}
        }else{owner.security_descriptor=None;}
    }else{
        unsafe{LocalFree(security_descriptor as HLOCAL);}
    }
    if handle == 0 {
        logging::debug_log(
            &format!("CreateDesktopW failed for shared private desktop: {error}"),
            logs_base_dir,
        );
        anyhow::bail!("CreateDesktopW failed for shared private desktop: {error}");
    }

    if retained.is_some(){Ok(None)}else{Ok(Some(PrivateDesktop {handle,name}))}
}

/// Original invocation-owned desktop. No Drop close: its checked CloseDesktop
/// attempt and uncertain raw debt remain with the original checked owner.
pub(crate) struct OwnedLaunchDesktop {
    // Borrowed current station: never CloseWindowStation, reopen or switch it.
    borrowed_station:isize,
    station_name:Vec<u16>,
    intended_sid:Vec<u8>,
    caller_facts:Option<crate::start_diagnostic::context_query::PrivateTokenFacts>,
    startup_queries:crate::start_diagnostic::context_query::StartupQueries,
    startup_context:crate::start_diagnostic::WindowsStartupContext,
    desktop:Option<std::mem::ManuallyDrop<PrivateDesktop>>,
    name:String,
    startup_name:Vec<u16>,
    creation_attempted:bool,
    source_token:Option<isize>,
    source_token_close_error:Option<i32>,
    security_descriptor:Option<isize>,
    descriptor_close_error:Option<i32>,
    close_attempted:bool,
    close_error:Option<i32>,
    closed:bool,
}
impl OwnedLaunchDesktop {
    /// Installed in the original checked owner BEFORE any resource allocation.
    pub(crate) fn new_attempt()->Self {Self {desktop:None,name:String::new(),startup_name:Vec::new(),
        borrowed_station:0,station_name:Vec::new(),intended_sid:Vec::new(),caller_facts:None,
        startup_queries:crate::start_diagnostic::context_query::StartupQueries::default(),
        startup_context:crate::start_diagnostic::WindowsStartupContext::default(),
        creation_attempted:false,source_token:None,source_token_close_error:None,
        security_descriptor:None,descriptor_close_error:None,
        close_attempted:false,close_error:None,closed:false}}
    pub(crate) fn create_once(&mut self,sandbox_username:&str,logs_base_dir:Option<&Path>)->Result<()> {
        anyhow::ensure!(!self.creation_attempted,"original desktop creation already attempted");
        self.creation_attempted=true;
        let intended_sid=resolve_sid(sandbox_username)?;
        let sid=string_from_sid_bytes(&intended_sid).map_err(anyhow::Error::msg)?;
        self.intended_sid=intended_sid;
        // Resolve station before allocating the original desktop.
        let (borrowed_station,station)=current_window_station()?;
        self.borrowed_station=borrowed_station;self.station_name=to_wide(&station);
        let token=unsafe{get_current_token_for_restriction()?};
        self.source_token=Some(token);
        self.caller_facts=Some(crate::start_diagnostic::context_query::PrivateTokenFacts::read(token));
        let owner_sid=unsafe{get_user_sid_bytes(token)};
        let closed=unsafe{CloseHandle(token)};
        let close_error=if closed==0 {let error=std::io::Error::last_os_error();
            self.source_token_close_error=Some(error.raw_os_error().unwrap_or(0));Some(error)
        }else{self.source_token=None;None};
        // Preserve the original SID-query error first, while any checked token
        // close debt remains anchored in THIS same preinstalled resource owner.
        let owner_sid=owner_sid?;
        if let Some(error)=close_error{return Err(error.into());}
        let owner_sid=string_from_sid_bytes(&owner_sid).map_err(anyhow::Error::msg)?;
        create_private_desktop_for_sids(&sid,&owner_sid,logs_base_dir,Some(self))?;
        let desktop=self.desktop.as_ref().ok_or_else(||anyhow::anyhow!("actual owned desktop absent"))?;
        let startup_name=qualified_desktop_name(&station,&desktop.name);
        let name=desktop.name.clone();
        self.name=name;self.startup_name=startup_name;Ok(())
    }
    pub(crate) fn startup_info_desktop(&self)->*mut u16 {self.startup_name.as_ptr() as *mut u16}
    pub(crate) fn name(&self)->&str {&self.name}
    pub(crate) fn record_startup_call(&mut self,startup_id:u64,mut call:crate::start_diagnostic::StartupCallParameters)
        ->crate::start_diagnostic::WindowsStartupContext {
        use crate::start_diagnostic::{ContextFact as F,ContextUnavailable as U};
        if (self.startup_context.startup_id!=0 && self.startup_context.startup_id!=startup_id)
            || self.startup_context.call!=F::NotObserved(U::NotAttempted) {
            return self.startup_context;
        }
        self.startup_context.startup_id=startup_id;
        let mut qualified=self.station_name.clone();
        if qualified.pop()!=Some(0) {
            call.desktop_pointer_is_same_owned_qualified_name=false;
        }else{
            qualified.push('\\' as u16);qualified.extend(to_wide(&self.name));
            call.desktop_pointer_is_same_owned_qualified_name &= qualified==self.startup_name;
        }
        // The pointer is compared by the caller against this exact retained allocation.
        call.station_still_same_borrowed_object=if self.borrowed_station==0 {
            F::NotObserved(U::OriginalUnavailable)
        }else{
            let current=unsafe{GetProcessWindowStation()};
            if current==0 {F::NotObserved(U::Win32{code:unsafe{GetLastError()}})}
            else {F::Observed(current==self.borrowed_station)}
        };
        self.startup_context.call=F::Observed(call);self.startup_context
    }
    pub(crate) fn observe_original_token_context(&mut self,process:isize,startup_id:u64,pid:u32,creation:u64)
        ->crate::start_diagnostic::WindowsStartupContext {
        self.observe_original_token_context_with_sample(process,startup_id,pid,creation,
            crate::start_diagnostic::ContextFact::NotObserved(
                crate::start_diagnostic::ContextUnavailable::NotAttempted))
    }
    pub(crate) fn observe_original_token_context_with_sample(&mut self,process:isize,startup_id:u64,pid:u32,creation:u64,
        before:crate::start_diagnostic::ContextFact<crate::WindowsStartRunnerObservation>)
        ->crate::start_diagnostic::WindowsStartupContext {
        if (self.startup_context.startup_id!=0 && self.startup_context.startup_id!=startup_id)
            || self.startup_context.original_runner_identity.is_some_and(|old|old!=(pid,creation)) {
            return self.startup_context;
        }
        if self.startup_queries.has_attempted() {return self.startup_context;}
        self.startup_context.startup_id=startup_id;
        self.startup_context.original_runner_identity=Some((pid,creation));
        use crate::start_diagnostic::{ContextFact as F,ContextUnavailable as U};
        self.startup_context.runner_before_token_query=match before {
            F::Observed(sample) if (sample.startup_id,sample.runner_pid,sample.runner_creation_filetime_100ns)
                != (startup_id,pid,creation)=>F::NotObserved(U::InvalidBuffer),
            other=>other,
        };
        if !self.closed {
            if let (Some(caller),Some(desktop))=(&self.caller_facts,&self.desktop) {
                self.startup_queries.observe(process,caller,&self.intended_sid,self.borrowed_station,
                    &self.station_name,desktop.handle,&to_wide(&desktop.name),&mut self.startup_context);
                self.startup_context.runner_after_token_query=F::Observed(
                    crate::start_diagnostic::context_query::sample_runner(process,startup_id,pid,creation));
            }else{
                self.startup_context.query_status=F::NotObserved(U::OriginalUnavailable);
            }
        }else{
            self.startup_context.query_status=crate::start_diagnostic::ContextFact::NotObserved(
                crate::start_diagnostic::ContextUnavailable::OriginalUnavailable);
        }
        self.startup_context.query_handle_closes=self.startup_queries.states();
        self.startup_context.query_slot_closes=self.startup_queries.all_states();
        self.startup_context.authz_dacl=self.startup_queries.authz_observation();
        self.startup_context
    }
    pub(crate) fn has_unresolved_startup_queries(&self)->bool {self.startup_queries.unresolved()}
    pub(crate) fn close_debt_target(&self)->Option<crate::elevated::native_close_protocol::CloseDebtTarget> {
        use crate::elevated::native_close_protocol::CloseDebtTarget;
        if self.startup_queries.unresolved() || self.source_token.is_some(){Some(CloseDebtTarget::HostDesktopToken)}
        else if self.security_descriptor.is_some(){Some(CloseDebtTarget::HostDesktopSecurityDescriptor)}
        else if self.close_error.is_some(){Some(CloseDebtTarget::HostDesktopHandle)}else{None}
    }
    pub(crate) fn close_once(&mut self)->std::io::Result<()> {
        if self.closed {return Ok(());}
        // Also covers an unwind after allocation. Failed/uncertain closes cannot
        // be retried, and must block the original desktop/owner normal close.
        let query_close=self.startup_queries.close_once();
        self.startup_context.authz_dacl=self.startup_queries.authz_observation();
        query_close?;
        if self.source_token.is_some(){return Err(self.source_token_close_error.map(std::io::Error::from_raw_os_error)
            .unwrap_or_else(||std::io::Error::other("original desktop token close debt retained")));}
        if self.security_descriptor.is_some(){return Err(self.descriptor_close_error.map(std::io::Error::from_raw_os_error)
            .unwrap_or_else(||std::io::Error::other("original desktop descriptor free debt retained")));}
        if !self.creation_attempted {return Err(std::io::Error::other("original desktop creation not observed"));}
        if self.close_attempted {
            return Err(self.close_error.map(std::io::Error::from_raw_os_error)
                .unwrap_or_else(||std::io::Error::other("original desktop close uncertain; no replay")));
        }
        self.close_attempted=true;
        // Absence is from this actual one-shot prelaunch creation path, never
        // inferred from a public numeric handle or foreign launch attempt.
        let Some(desktop)=self.desktop.as_mut() else{self.closed=true;return Ok(())};
        if unsafe{CloseDesktop(desktop.handle)}==0 {
            let error=std::io::Error::last_os_error();self.close_error=error.raw_os_error();
            return Err(error);
        }
        desktop.handle=0;self.closed=true;
        // The successful OS close precedes dropping only inert allocation data.
        let mut desktop=self.desktop.take().unwrap();
        unsafe{std::mem::ManuallyDrop::drop(&mut desktop)};
        Ok(())
    }
}

#[cfg(test)]
mod owned_native_debt_tests {
    use super::*;
    fn current_query_fixture()->(OwnedLaunchDesktop,isize,u32,u64) {
        // New local test resource only; no account logon/product/guest startup.
        let mut owner=OwnedLaunchDesktop::new_attempt();owner.creation_attempted=true;
        let (station,name)=current_window_station().unwrap();
        owner.borrowed_station=station;owner.station_name=to_wide(&name);
        let token=unsafe{get_current_token_for_restriction().unwrap()};
        owner.source_token=Some(token);
        owner.caller_facts=Some(crate::start_diagnostic::context_query::PrivateTokenFacts::read(token));
        let sid=unsafe{get_user_sid_bytes(token)}.unwrap();
        owner.intended_sid=sid.clone();
        if unsafe{CloseHandle(token)}==0 {
            owner.source_token_close_error=Some(unsafe{GetLastError()} as i32);
            panic!("fixture caller token close failed; original retained");
        }
        owner.source_token=None;
        let sid_text=string_from_sid_bytes(&sid).unwrap();
        create_private_desktop_for_sids(&sid_text,&sid_text,None,Some(&mut owner)).unwrap();
        owner.name=owner.desktop.as_ref().unwrap().name.clone();
        owner.startup_name=qualified_desktop_name(&name,&owner.name);
        let process=unsafe{windows_sys::Win32::System::Threading::GetCurrentProcess()};
        let mut creation:windows_sys::Win32::Foundation::FILETIME=unsafe{std::mem::zeroed()};
        let mut exit=creation;let mut kernel=creation;let mut user=creation;
        assert_ne!(unsafe{windows_sys::Win32::System::Threading::GetProcessTimes(
            process,&mut creation,&mut exit,&mut kernel,&mut user)},0);
        let filetime=((creation.dwHighDateTime as u64)<<32)|creation.dwLowDateTime as u64;
        assert_ne!(filetime,0);
        (owner,process,std::process::id(),filetime)
    }

    #[test]
    fn split_query_context_has_actual_masks_three_closes_and_dedicated_original_samples() {
        use crate::start_diagnostic::{ContextFact as F,QueryHandleClose,TokenOpenPurpose};
        let (mut owner,process,pid,creation)=current_query_fixture();
        let before=crate::start_diagnostic::context_query::sample_runner(process,39,pid,creation);
        let context=owner.observe_original_token_context_with_sample(process,39,pid,creation,F::Observed(before));
        assert_eq!(context.runner_before_token_query,F::Observed(before));
        match context.runner_after_token_query {
            F::Observed(after)=>{
                assert_eq!((after.startup_id,after.runner_pid,after.runner_creation_filetime_100ns),(39,pid,creation));
                assert_eq!(after.state,crate::WindowsStartRunnerState::RunningAtObservation);
            }
            other=>panic!("actual original post sample unavailable: {other:?}"),
        }
        assert_eq!(context.query_stages.primary_open.requested_mask,8);
        assert_eq!(context.query_stages.primary_open.purpose,TokenOpenPurpose::PrimaryQuery);
        assert_eq!(context.query_stages.duplicate_source_open.requested_mask,2);
        assert_eq!(context.query_stages.duplicate_source_open.purpose,TokenOpenPurpose::DuplicationSource);
        assert_eq!(context.query_slot_closes,[QueryHandleClose::ClosedVerified;3]);
        assert!(context.desktop.access_check_invoked);
        assert_eq!(context.desktop.descriptor_read,F::Observed(()));
        match context.desktop.access {
            F::Observed(access)=>assert!(access.access_status),
            other=>panic!("real owned fixture AccessCheck unavailable: {other:?}"),
        }
        assert!(!owner.has_unresolved_startup_queries());
        // Invalid numeric input is never queried after the one-shot latch.
        assert_eq!(owner.observe_original_token_context_with_sample(0,39,pid,creation,
            F::NotObserved(crate::start_diagnostic::ContextUnavailable::Busy)),context);
        owner.close_once().unwrap();assert!(owner.closed);
        assert_eq!(context.query_slot_closes,[QueryHandleClose::ClosedVerified;3]);
    }

    #[test]
    fn missing_token_is_upstream_while_real_descriptor_reads_do_not_invoke_accesscheck() {
        use crate::start_diagnostic::{ContextFact as F,ContextUnavailable as U,ContextApiStage,ContextFailure};
        let (mut owner,_,_,_)=current_query_fixture();
        let private=owner.desktop.as_ref().unwrap();
        let observed=crate::start_diagnostic::context_query::missing_token_object_fixture(
            private.handle,&to_wide(&private.name));
        assert_eq!(observed.name_equals_creation_object,F::Observed(true));
        assert_eq!(observed.descriptor_read,F::Observed(()));
        assert_eq!(observed.dacl_present,F::Observed(true));
        assert!(!observed.access_check_invoked);
        assert_eq!(observed.access,F::NotObserved(U::Upstream {
            stage:ContextApiStage::QueryPrimaryOpen,reason:ContextFailure::Win32{code:5}}));
        owner.close_once().unwrap();assert!(owner.closed);
    }

    #[test]
    fn any_of_three_query_debts_blocks_the_original_desktop_normal_close() {
        for index in 0..3 {
            let mut owner=OwnedLaunchDesktop::new_attempt();owner.creation_attempted=true;
            owner.startup_queries=crate::start_diagnostic::context_query::StartupQueries::retained_close_debt_for_slot_fixture(index);
            for _ in 0..2 {
                assert_eq!(owner.close_once().unwrap_err().raw_os_error(),Some(5));
                assert!(!owner.closed);assert!(!owner.close_attempted);
                assert!(owner.has_unresolved_startup_queries());
                assert_eq!(owner.close_debt_target(),
                    Some(crate::elevated::native_close_protocol::CloseDebtTarget::HostDesktopToken));
            }
        }
    }
    #[test]
    fn diagnostic_query_close_debt_blocks_original_desktop_close_and_cannot_be_retried() {
        let mut owner=OwnedLaunchDesktop::new_attempt();
        owner.creation_attempted=true;
        owner.startup_queries=crate::start_diagnostic::context_query::StartupQueries::retained_close_debt_fixture();
        for _ in 0..2 {
            assert_eq!(owner.close_once().unwrap_err().raw_os_error(),Some(5));
            assert!(!owner.close_attempted);assert!(!owner.closed);
            assert!(owner.has_unresolved_startup_queries());
            assert_eq!(owner.close_debt_target(),
                Some(crate::elevated::native_close_protocol::CloseDebtTarget::HostDesktopToken));
        }
    }
    #[test]
    fn current_token_owned_private_desktop_access_query_is_bounded_and_explicitly_closed() {
        // Real local test process and newly owned desktop, no alternate logon,
        // product execution, WFP/ACL edits to shared objects or receipt authority.
        use crate::start_diagnostic::{ContextFact as F,QueryHandleClose};
        let mut owner=OwnedLaunchDesktop::new_attempt();owner.creation_attempted=true;
        let (station,name)=current_window_station().unwrap();
        owner.borrowed_station=station;owner.station_name=to_wide(&name);
        let token=unsafe{get_current_token_for_restriction().unwrap()};
        owner.source_token=Some(token);
        owner.caller_facts=Some(crate::start_diagnostic::context_query::PrivateTokenFacts::read(token));
        let sid=unsafe{get_user_sid_bytes(token)}.unwrap();
        owner.intended_sid=sid.clone();
        if unsafe{CloseHandle(token)}==0 {
            owner.source_token_close_error=Some(unsafe{GetLastError()} as i32);
            panic!("fixture source token close failed; retained original debt");
        }
        owner.source_token=None;
        let sid_text=string_from_sid_bytes(&sid).unwrap();
        create_private_desktop_for_sids(&sid_text,&sid_text,None,Some(&mut owner)).unwrap();
        let private=owner.desktop.as_ref().unwrap();
        owner.name=private.name.clone();
        owner.startup_name=qualified_desktop_name(&name,&owner.name);
        let process=unsafe{windows_sys::Win32::System::Threading::GetCurrentProcess()};
        let mut creation:windows_sys::Win32::Foundation::FILETIME=unsafe{std::mem::zeroed()};
        let mut exit=creation;let mut kernel=creation;let mut user=creation;
        assert_ne!(unsafe{windows_sys::Win32::System::Threading::GetProcessTimes(
            process,&mut creation,&mut exit,&mut kernel,&mut user)},0);
        let filetime=((creation.dwHighDateTime as u64)<<32)|creation.dwLowDateTime as u64;
        assert_ne!(filetime,0);
        let context=owner.observe_original_token_context(process,38,std::process::id(),filetime);
        assert_eq!(context.token.user_equals_intended_sandbox,F::Observed(true));
        assert_eq!(context.token.user_equals_caller_user,F::Observed(true));
        assert_eq!(context.token.session_equals_caller_session,F::Observed(true));
        assert_eq!(context.desktop.name_equals_creation_object,F::Observed(true));
        assert_eq!(context.station.name_equals_creation_object,F::Observed(true));
        match context.desktop.access {
            F::Observed(access)=>{
                assert!(access.access_status);
                assert_eq!(access.granted_mask & DESKTOP_ALL_ACCESS,DESKTOP_ALL_ACCESS);
                assert_eq!(access.denial_win32_error,None);
            }
            other=>panic!("actual fixture desktop AccessCheck unavailable: {other:?}"),
        }
        assert_eq!(context.query_handle_closes,[QueryHandleClose::ClosedVerified;2]);
        assert!(!owner.has_unresolved_startup_queries());
        let repeated=owner.observe_original_token_context(0,38,std::process::id(),filetime);
        assert_eq!(repeated,context); // No second OpenProcessToken on supplied value0.
        let foreign=owner.observe_original_token_context(0,39,std::process::id(),filetime);
        assert_eq!(foreign,context);
        owner.close_once().unwrap();assert!(owner.closed);
        drop(owner);
        assert_eq!(context.query_handle_closes,[QueryHandleClose::ClosedVerified;2]);
    }
    #[test]
    fn uninitialized_or_retained_transient_desktop_debt_cannot_be_reported_closed_or_retried() {
        let mut uninitialized=OwnedLaunchDesktop::new_attempt();
        assert!(uninitialized.close_once().is_err());assert!(!uninitialized.closed);
        // Labelled scalar debt-negative fixture. Its guard rejects BEFORE any
        // OS call; neither value is used as a real handle or close witness.
        for token in [true,false] {
            let mut owner=OwnedLaunchDesktop::new_attempt();owner.creation_attempted=true;
            if token {owner.source_token=Some(1234);owner.source_token_close_error=Some(5);}
            else {owner.security_descriptor=Some(2345);owner.descriptor_close_error=Some(5);}
            for _ in 0..2 {assert_eq!(owner.close_once().unwrap_err().raw_os_error(),Some(5));}
            assert!(!owner.closed);assert!(!owner.close_attempted);
            assert_eq!(owner.source_token.is_some(),token);assert_eq!(owner.security_descriptor.is_some(),!token);
            assert!(owner.close_debt_target().is_some());
        }
    }
}

struct PrivateDesktop {
    handle: isize,
    name: String,
}

impl PrivateDesktop {
    fn create(logs_base_dir: Option<&Path>) -> Result<Self> {
        let mut rng = SmallRng::from_entropy();
        let name = format!("CodexSandboxDesktop-{:x}", rng.r#gen::<u128>());
        let name_wide = to_wide(&name);
        let handle = unsafe {
            CreateDesktopW(
                name_wide.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
                0,
                DESKTOP_ALL_ACCESS,
                ptr::null_mut(),
            )
        };
        if handle == 0 {
            let err = unsafe { GetLastError() } as i32;
            logging::debug_log(
                &format!(
                    "CreateDesktopW failed for {name}: {} ({})",
                    err,
                    format_last_error(err),
                ),
                logs_base_dir,
            );
            return Err(anyhow::anyhow!("CreateDesktopW failed: {err}"));
        }

        unsafe {
            if let Err(err) = grant_desktop_access(handle, logs_base_dir) {
                let _ = CloseDesktop(handle);
                return Err(err);
            }
        }

        Ok(Self { handle, name })
    }
}

unsafe fn grant_desktop_access(handle: isize, logs_base_dir: Option<&Path>) -> Result<()> {
    let token = get_current_token_for_restriction()?;
    let mut logon_sid = get_logon_sid_bytes(token)?;
    CloseHandle(token);

    let entries = [EXPLICIT_ACCESS_W {
        grfAccessPermissions: DESKTOP_ALL_ACCESS,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: 0,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: ptr::null_mut(),
            MultipleTrusteeOperation: 0,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: logon_sid.as_mut_ptr() as *mut c_void as *mut u16,
        },
    }];

    let mut updated_dacl = ptr::null_mut();
    let set_entries_code = SetEntriesInAclW(
        entries.len() as u32,
        entries.as_ptr(),
        ptr::null_mut(),
        &mut updated_dacl,
    );
    if set_entries_code != ERROR_SUCCESS {
        logging::debug_log(
            &format!("SetEntriesInAclW failed for private desktop: {set_entries_code}"),
            logs_base_dir,
        );
        return Err(anyhow::anyhow!(
            "SetEntriesInAclW failed for private desktop: {set_entries_code}"
        ));
    }

    let set_security_code = SetSecurityInfo(
        handle,
        SE_WINDOW_OBJECT,
        DACL_SECURITY_INFORMATION,
        ptr::null_mut(),
        ptr::null_mut(),
        updated_dacl,
        ptr::null_mut(),
    );
    if !updated_dacl.is_null() {
        LocalFree(updated_dacl as HLOCAL);
    }
    if set_security_code != ERROR_SUCCESS {
        logging::debug_log(
            &format!("SetSecurityInfo failed for private desktop: {set_security_code}"),
            logs_base_dir,
        );
        return Err(anyhow::anyhow!(
            "SetSecurityInfo failed for private desktop: {set_security_code}"
        ));
    }

    Ok(())
}

impl Drop for PrivateDesktop {
    fn drop(&mut self) {
        unsafe {
            if self.handle != 0 {
                let _ = CloseDesktop(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod window_station_name_tests {
    use super::*;

    fn encoded(name: &str) -> Vec<u16> {
        name.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[test]
    fn station_name_preserves_noninteractive_and_unicode_names() {
        for name in ["WinSta0", "Service-0x0-3e7$", "CodexStation-0123", "隔离Station"] {
            let raw = encoded(name);
            assert_eq!(decode_window_station_name(&raw, (raw.len() * 2) as u32).unwrap(), name);
        }
    }

    #[test]
    fn station_name_rejects_empty_separators_and_controls() {
        for name in ["", "station\\desktop", "station/desktop", "station\n", "station\u{7f}"] {
            let raw = encoded(name);
            assert!(decode_window_station_name(&raw, (raw.len() * 2) as u32).is_err());
        }
    }

    #[test]
    fn station_name_rejects_bad_utf16_and_nul_placement() {
        for raw in [vec![0xd800, 0], vec![65, 66], vec![65, 0, 66, 0]] {
            assert!(decode_window_station_name(&raw, (raw.len() * 2) as u32).is_err());
        }
    }

    #[test]
    fn station_name_rejects_changed_odd_and_over_cap_lengths() {
        let raw = encoded("Station");
        for bytes in [0, 2, 3, (raw.len() * 2 - 2) as u32, (raw.len() * 2 + 2) as u32, 1026, u32::MAX] {
            assert!(decode_window_station_name(&raw, bytes).is_err());
        }
    }

    #[test]
    fn station_name_accepts_exact_cap_and_rejects_next_unit() {
        let raw = encoded(&"s".repeat(MAX_WINDOW_STATION_NAME_UNITS - 1));
        assert!(decode_window_station_name(&raw, (raw.len() * 2) as u32).is_ok());
        let raw = encoded(&"s".repeat(MAX_WINDOW_STATION_NAME_UNITS));
        assert!(decode_window_station_name(&raw, (raw.len() * 2) as u32).is_err());
    }

    #[test]
    fn startup_name_uses_the_observed_station_and_one_final_nul() {
        let desktop = "CodexSandboxDesktop-0123456789abcdef0123456789abcdef";
        let raw = qualified_desktop_name("Service-0x0-3e7$", desktop);
        assert_eq!(raw.last(), Some(&0));
        assert_eq!(raw.iter().filter(|&&unit| unit == 0).count(), 1);
        assert_eq!(String::from_utf16(&raw[..raw.len() - 1]).unwrap(), format!("Service-0x0-3e7$\\{desktop}"));
    }
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;
