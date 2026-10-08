//! Fixed synthetic child parameters, reviewed by the real production port.
use crate::{
    preflight::{Artifact, observe_artifact},
    provisioning::GuestLifecycle,
};
use anyhow::{Result, ensure};
use codex_exec_server::{FileSystemSandboxContext, WindowsSandboxSelection};
use codex_utils_path_uri::PathUri;
use morrow_codex_session_exec_windows_v1::{ExecParams, ProcessId};

#[derive(Clone, Copy)]
pub enum Witness<'a> {
    Basic(&'a crate::basic_security::SecuritySpec),
    PipeEof,
    ConPtySize,
}

pub fn witness_params(
    lifecycle: &GuestLifecycle,
    helper: &Artifact,
    operation: &str,
    nonce: &str,
    witness: Witness<'_>,
) -> Result<ExecParams> {
    ensure!(
        crate::witness::valid_nonce(nonce),
        "invalid fixed child witness nonce"
    );
    let image = observe_artifact(helper)?;
    ensure!(
        std::env::current_exe()?.canonicalize()? == image.canonical_path,
        "the fixed child/helper must be this actual pinned harness executable"
    );
    let cwd = PathUri::from_host_native_path(lifecycle.workspace())?;
    let mut sandbox = FileSystemSandboxContext::from_permission_profile(
        lifecycle.permission_profile()?,
        cwd.clone(),
    );
    sandbox.windows_sandbox_selection = WindowsSandboxSelection::Elevated;
    let basic = matches!(witness, Witness::Basic(_));
    let mode = match witness {
        Witness::Basic(_) => "--witness-basic",
        Witness::PipeEof => "--witness-eof",
        Witness::ConPtySize => "--witness-pty",
    };
    let tty = matches!(witness, Witness::ConPtySize);
    let mut argv = vec![helper.path.to_str().ok_or_else(|| anyhow::anyhow!("helper Unicode path"))?.to_owned(),
        mode.to_owned(), nonce.to_owned()];
    if let Witness::Basic(spec) = witness {
        let (allowed, denied) = crate::basic_witness::paths(lifecycle.synthetic_root(), nonce)?;
        for path in [allowed, denied] {
            argv.push(path.to_str().ok_or_else(|| anyhow::anyhow!("basic witness Unicode path"))?.to_owned());
        }
        // Full system image identity and exact network target are part of the
        // original immutable ExecParams -> Intent review and Claim binding.
        argv.push(spec.query.path.to_str().ok_or_else(|| anyhow::anyhow!("SID query Unicode path"))?.to_owned());
        argv.push(crate::preflight::hex(&spec.query.sha256));
        argv.push(spec.expected_sid.clone());
        argv.push(spec.port.to_string());
    }
    Ok(ExecParams {
        process_id: ProcessId::new(operation),
        metadata: None,
        argv,
        cwd,
        env_policy: None,
        shell_snapshot: None,
        env: lifecycle.environment(),
        tty,
        pipe_stdin: !basic,
        arg0: None,
        sandbox: Some(sandbox),
        enforce_managed_network: false,
        managed_network: None,
        network_proxy: None,
    })
}
