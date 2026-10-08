//! Trusted independent artifact acquisition and compatibility metadata binding.
//! Metadata is not binary identity, approval, helper handshake or sandbox proof.
use std::path::Path;
use std::sync::Arc;

use codex_exec_server::{ExecParams, ExecServerRuntimeOptions, WindowsSandboxSelection};
use codex_http_client::HttpClientFactory;
use codex_sandboxing::{MatchedRunnerArtifact, SandboxType};
use morrow_agent_session_exec_v1_r2::{Error, Result};
use tokio::runtime::Runtime;

use crate::ProvisionedWindowsBackend;

const CHECKED_PROTOCOL_VERSION: u32 = 7;
// Pin of the independent derived runner control layout, never a Core wire copy.
const CHECKED_SCHEMA_SHA256: [u8; 32] = [
    47, 129, 224, 48, 226, 133, 48, 72, 12, 233, 197, 28, 212, 105, 181, 9, 123, 169, 250, 219, 59,
    123, 116, 198, 60, 193, 216, 59, 17, 20, 235, 24,
];

#[derive(Clone)]
struct RunnerIdentity {
    canonical_path: String,
    full_sha256: [u8; 32],
    protocol_version: u32,
    control_schema_sha256: [u8; 32],
}

impl RunnerIdentity {
    fn validate(&self) -> Result<()> {
        if self.protocol_version != CHECKED_PROTOCOL_VERSION
            || self.control_schema_sha256 != CHECKED_SCHEMA_SHA256
            || self.canonical_path.is_empty()
        {
            return Err(Error::Denied);
        }
        Ok(())
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "canonical_path":self.canonical_path, "full_sha256":self.full_sha256,
            "protocol_version":self.protocol_version,"control_schema_sha256":self.control_schema_sha256,
        })
    }
}

/// A trusted host-only pin; no guest path or artifact handle is exposed.
/// Acquiring this spec locks the exact file and its ancestors independently of
/// the original codex self helper. Hold both locks until actual cleanup finishes.
#[derive(Clone)]
pub struct WindowsRunnerProvisioningSpec {
    artifact: Arc<MatchedRunnerArtifact>,
    identity: RunnerIdentity,
}

impl WindowsRunnerProvisioningSpec {
    pub fn acquire(path: &Path, full_sha256: [u8; 32]) -> Result<Self> {
        let artifact =
            Arc::new(MatchedRunnerArtifact::acquire(path, full_sha256).map_err(|_| Error::Denied)?);
        let identity = RunnerIdentity {
            canonical_path: artifact.path().to_str().ok_or(Error::Invalid)?.to_owned(),
            full_sha256: artifact.sha256(),
            protocol_version: artifact.protocol_version(),
            control_schema_sha256: artifact.control_schema_sha256(),
        };
        identity.validate()?;
        Ok(Self { artifact, identity })
    }

    pub(crate) fn validate_metadata(&self) -> Result<()> {
        self.identity.validate()
    }
    pub(crate) fn identity_json(&self) -> serde_json::Value {
        self.identity.json()
    }
    pub(crate) fn artifact(&self) -> Arc<MatchedRunnerArtifact> {
        Arc::clone(&self.artifact)
    }
}

pub(crate) fn validate_checked_selection(expected: SandboxType, params: &ExecParams) -> Result<()> {
    if expected != SandboxType::WindowsRestrictedToken
        || params.sandbox.as_ref().is_none_or(|context| {
            context.windows_sandbox_selection != WindowsSandboxSelection::Elevated
        })
    {
        return Err(Error::Denied);
    }
    Ok(())
}

impl ProvisionedWindowsBackend {
    /// Provisions one actual local backend using the independently locked v7 runner.
    /// The old None/MXC/ordinary entries retain their original route and policy.
    pub fn production_with_checked_runner(
        options: ExecServerRuntimeOptions,
        factory: HttpClientFactory,
        helper_sha256: [u8; 32],
        runtime: Arc<Runtime>,
        spec: WindowsRunnerProvisioningSpec,
    ) -> Result<Arc<Self>> {
        spec.validate_metadata()?;
        Self::create(
            options,
            factory,
            helper_sha256,
            runtime,
            SandboxType::WindowsRestrictedToken,
            true,
            Some(spec),
            |backend| backend,
        )
    }
}

#[cfg(test)]
#[path = "windows_runner_tests.rs"]
mod tests;
