//! Finite diagnostic facts only. No native handle, callback or release authority.
//! The existing ErrorPayload string carries an exact allowlisted atom; arbitrary
//! error text, command arguments, credentials and paths remain redacted.
use std::fmt;

macro_rules! sites {
    ($($site:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub enum RunnerStartupSite { #[default] NotObserved, $($site),+ }
        impl RunnerStartupSite {
            pub fn wire_message(self) -> &'static str {
                match self {
                    Self::NotObserved => "v8 operation rejected; original owner retained",
                    $(Self::$site => concat!("v8 startup site: ", stringify!($site))),+
                }
            }
            pub fn from_wire_message(message: &str) -> Self {
                match message {
                    $(concat!("v8 startup site: ", stringify!($site)) => Self::$site,)+
                    _ => Self::NotObserved,
                }
            }
        }
    }
}
sites! {
    ReserveAttempt, ResolveTokenMode, ParseCapabilitySid, ParseProxySid,
    CreateFallbackJob, OpenOriginalToken, DuplicateRestrictionToken,
    CreateCapabilityToken, CreateRestrictedToken, SetRestrictedDefaultDacl,
    EnableChangeNotifyPrivilege, CloseOriginalQueryToken, CloseIntermediateToken,
    OpenPrivateDesktop, PrepareStdioSecurity, CreateStdinPipe, CreateStdoutPipe,
    CreateStderrPipe, CreateProcessJob, QueryAppPackage, AllocateProcessAttributes,
    SetProcessJobAttribute, PreserveDesktopAppContext, SetStdioInherit,
    SetInheritedHandleList, CreateProcessAsUser, SpawnStdioChild, SpawnConptyChild,
    AdmitChildContext, CloseRestrictedToken,
}
impl RunnerStartupSite {
    /// Preserve a more specific typed site already recorded by the same call.
    pub fn context(self, error: anyhow::Error) -> anyhow::Error {
        if error.downcast_ref::<Self>().is_some() { error } else { error.context(self) }
    }
    /// Never infer a site from a Windows error number or a formatted error body.
    pub fn from_error(error: &anyhow::Error) -> Self {
        error.downcast_ref::<Self>().copied().unwrap_or_default()
    }
}
impl fmt::Display for RunnerStartupSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.wire_message()) }
}
impl std::error::Error for RunnerStartupSite {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_finite_atoms_survive_redaction() {
        for site in [RunnerStartupSite::OpenOriginalToken, RunnerStartupSite::CreateProcessAsUser,
            RunnerStartupSite::CreateStderrPipe, RunnerStartupSite::CloseRestrictedToken] {
            assert_eq!(RunnerStartupSite::from_wire_message(site.wire_message()), site);
            assert!(site.wire_message().len() < 96);
            for changed in [format!("{} secret", site.wire_message()),
                format!(" {}", site.wire_message()), site.wire_message().to_lowercase()] {
                assert_eq!(RunnerStartupSite::from_wire_message(&changed), RunnerStartupSite::NotObserved);
            }
        }
        for body in ["C:\\private\\credential.txt", "password=private", "OpenOriginalToken", ""] {
            assert_eq!(RunnerStartupSite::from_wire_message(body), RunnerStartupSite::NotObserved);
        }
    }
    #[test]
    fn typed_inner_site_keeps_the_original_os_error_without_inferring_from_text() {
        let original = anyhow::Error::from(std::io::Error::from_raw_os_error(5));
        assert_eq!(RunnerStartupSite::from_error(&original), RunnerStartupSite::NotObserved);
        let error = RunnerStartupSite::OpenOriginalToken.context(original);
        let error = RunnerStartupSite::CreateCapabilityToken.context(error);
        assert_eq!(RunnerStartupSite::from_error(&error), RunnerStartupSite::OpenOriginalToken);
        assert_eq!(error.downcast_ref::<std::io::Error>().unwrap().raw_os_error(), Some(5));
        let text_only = anyhow::anyhow!("v8 startup site: OpenOriginalToken");
        assert_eq!(RunnerStartupSite::from_error(&text_only), RunnerStartupSite::NotObserved);
    }
}
