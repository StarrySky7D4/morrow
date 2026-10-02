//! Native source route prerequisites, separate from package declarations and
//! SDK discovery. Compiling the route never creates a protected storage owner.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prerequisite {
    ProtectedStorageOwnerUnavailable,
    SupervisedOwnerRequired,
}
impl Prerequisite {
    /// Existing private UI response field; no channel or guest ABI change.
    pub fn ui_code(self) -> u16 {
        match self {
            Self::ProtectedStorageOwnerUnavailable => 130,
            Self::SupervisedOwnerRequired => 131,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::ProtectedStorageOwnerUnavailable => "protected_storage_owner_unavailable",
            Self::SupervisedOwnerRequired => "supervised_owner_required",
        }
    }
}
impl std::fmt::Display for Prerequisite {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str(match self {
            Self::ProtectedStorageOwnerUnavailable => {
                "local channels require a protected original storage owner; this platform is unsupported"
            }
            Self::SupervisedOwnerRequired => {
                "local channels require the current Windows supervised owner"
            }
        })
    }
}
impl std::error::Error for Prerequisite {}

/// This is a trusted in-process proof marker, never a caller-supplied bool or
/// wire token. Only the existing supervised Windows entry may produce the
/// production marker; the explicit synthetic marker exists only in unit tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnerBinding {
    #[cfg(windows)]
    SupervisedWindows,
    #[cfg(test)]
    SyntheticFixture,
}
impl OwnerBinding {
    pub(crate) fn supervised() -> std::result::Result<Self, Prerequisite> {
        #[cfg(windows)]
        {
            Ok(Self::SupervisedWindows)
        }
        #[cfg(not(windows))]
        {
            Err(Prerequisite::ProtectedStorageOwnerUnavailable)
        }
    }
}

pub(crate) fn require_owner(
    owner: Option<OwnerBinding>,
) -> std::result::Result<OwnerBinding, Prerequisite> {
    owner.ok_or_else(|| {
        if cfg!(windows) {
            Prerequisite::SupervisedOwnerRequired
        } else {
            Prerequisite::ProtectedStorageOwnerUnavailable
        }
    })
}

/// Read-only compiled implementation information, not current owner state,
/// production qualification, authorization, or a promise to accept a package.
pub(crate) fn implementation_descriptor() -> serde_json::Value {
    serde_json::json!({
        "native_source_route_compiled": true,
        "production_public_binding_available": false,
        "protected_owner_backend": if cfg!(windows) { "windows_only" } else { "unavailable" },
        "unbound_prerequisite": if cfg!(windows) {
            Prerequisite::SupervisedOwnerRequired.code()
        } else {
            Prerequisite::ProtectedStorageOwnerUnavailable.code()
        },
        "required_owner_proofs": ["original_protected_storage_owner", "verified_supervisor_and_controller_watch"],
        "synthetic_fixture_is_production_authority": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_source_route_and_discovery_never_grant_an_owner() {
        let value = implementation_descriptor();
        assert_eq!(value["native_source_route_compiled"], true);
        assert_eq!(value["production_public_binding_available"], false);
        assert_eq!(value["synthetic_fixture_is_production_authority"], false);
        assert_eq!(
            Prerequisite::ProtectedStorageOwnerUnavailable.ui_code(),
            130
        );
        assert_eq!(Prerequisite::SupervisedOwnerRequired.ui_code(), 131);
        assert!(require_owner(None).is_err());
        assert_eq!(
            require_owner(Some(OwnerBinding::SyntheticFixture)),
            Ok(OwnerBinding::SyntheticFixture)
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn an_explicit_supervised_bind_cannot_invent_a_linux_protected_owner() {
        assert_eq!(
            OwnerBinding::supervised(),
            Err(Prerequisite::ProtectedStorageOwnerUnavailable)
        );
        assert_eq!(
            require_owner(None),
            Err(Prerequisite::ProtectedStorageOwnerUnavailable)
        );
    }
}
