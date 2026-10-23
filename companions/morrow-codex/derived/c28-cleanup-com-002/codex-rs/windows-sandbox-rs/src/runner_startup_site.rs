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
    CloseChildPipe, CloseUnusedChildStdin, AdmitChildJob, AdmitChildProcess,
    AdmitChildThread, AdmitChildStdout, ValidateChildIdentity, BindChildIdentity,
}

/// A bounded copy of facts from an error held by the original pipe owner.
/// It owns no native resource and cannot settle or replay that owner's attempt.
#[derive(Debug)]
struct RetainedStartupFailure { site: RunnerStartupSite, original: std::io::Error }
impl fmt::Display for RetainedStartupFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.site.wire_message()) }
}
impl std::error::Error for RetainedStartupFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.original) }
}
impl RunnerStartupSite {
    /// Preserve a more specific typed site already recorded by the same call.
    pub fn context(self, error: anyhow::Error) -> anyhow::Error {
        if Self::from_error(&error) != Self::NotObserved { error } else { error.context(self) }
    }
    /// Never infer a site from a Windows error number or a formatted error body.
    pub fn from_error(error: &anyhow::Error) -> Self {
        // io::Error's source chain can skip the boxed error itself. Inspect its
        // typed payload explicitly before considering a broader outer context.
        for cause in error.chain() {
            if let Some(failure) = cause.downcast_ref::<std::io::Error>()
                .and_then(std::io::Error::get_ref)
                .and_then(|inner| inner.downcast_ref::<RetainedStartupFailure>()) {
                if failure.site != Self::NotObserved { return failure.site; }
            }
        }
        error.downcast_ref::<Self>().copied().unwrap_or_default()
    }
    pub fn io_context(self, original: std::io::Error) -> std::io::Error {
        if original.get_ref().is_some_and(|inner| inner.is::<RetainedStartupFailure>()) { return original; }
        std::io::Error::new(original.kind(), RetainedStartupFailure { site: self, original })
    }
    /// Copy only an actual typed site and native error code, never formatted
    /// launch details, arguments or paths. The original error remains retained.
    pub fn retained_io_error(error: &anyhow::Error) -> std::io::Error {
        let native = error.chain().filter_map(|cause| cause.downcast_ref::<std::io::Error>())
            .find_map(std::io::Error::raw_os_error);
        let original = native.map(std::io::Error::from_raw_os_error)
            .unwrap_or_else(|| std::io::Error::other("retained launch failure without native error code"));
        Self::from_error(error).io_context(original)
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
    #[test]
    fn retained_launch_error_preserves_finite_site_and_native_code_through_io_admission() {
        let original = anyhow::Error::from(std::io::Error::from_raw_os_error(5))
            .context("private command and path must remain in original owner")
            .context(RunnerStartupSite::CreateProcessAsUser);
        let copied = RunnerStartupSite::retained_io_error(&original);
        assert!(!copied.to_string().contains("private"));
        let error = anyhow::Error::from(copied).context(RunnerStartupSite::AdmitChildContext);
        assert_eq!(RunnerStartupSite::from_error(&error), RunnerStartupSite::CreateProcessAsUser);
        assert_eq!(error.chain().filter_map(|cause| cause.downcast_ref::<std::io::Error>())
            .find_map(std::io::Error::raw_os_error), Some(5));
        assert_eq!(original.downcast_ref::<std::io::Error>().unwrap().raw_os_error(), Some(5));
        let error = RunnerStartupSite::SpawnStdioChild.context(error);
        assert_eq!(RunnerStartupSite::from_error(&error), RunnerStartupSite::CreateProcessAsUser);
    }
    #[test]
    fn retained_text_cannot_create_a_native_code_or_site_and_validation_sites_stay_bounded() {
        let text = anyhow::anyhow!("v8 startup site: CreateProcessAsUser; native error 5; private path");
        let error = anyhow::Error::from(RunnerStartupSite::retained_io_error(&text));
        assert_eq!(RunnerStartupSite::from_error(&error), RunnerStartupSite::NotObserved);
        assert!(error.chain().filter_map(|cause| cause.downcast_ref::<std::io::Error>())
            .all(|cause| cause.raw_os_error().is_none()));
        for site in [RunnerStartupSite::CloseChildPipe, RunnerStartupSite::CloseUnusedChildStdin,
            RunnerStartupSite::AdmitChildJob, RunnerStartupSite::AdmitChildProcess,
            RunnerStartupSite::AdmitChildThread, RunnerStartupSite::AdmitChildStdout,
            RunnerStartupSite::ValidateChildIdentity, RunnerStartupSite::BindChildIdentity] {
            let error = anyhow::Error::from(site.io_context(std::io::Error::from_raw_os_error(6)))
                .context(RunnerStartupSite::AdmitChildContext);
            assert_eq!(RunnerStartupSite::from_error(&error), site);
            assert_eq!(RunnerStartupSite::from_wire_message(site.wire_message()), site);
            assert!(site.wire_message().len() < 96);
        }
    }
}
