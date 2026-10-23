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

/// Pure argv construction only. The mode/SID/port are bound into the original
/// ExecParams -> Intent; none of these public expected values is an observation.
pub(crate) fn basic_arguments(root:&std::path::Path,nonce:&str,spec:&crate::basic_security::SecuritySpec)->Result<Vec<String>> {
    ensure!(crate::witness::valid_nonce(nonce) && crate::basic_security::valid_sid(&spec.expected_sid)
        && spec.port!=0,"basic argv identity/nonce/port");
    let (allowed,denied)=crate::basic_witness::paths(root,nonce)?;
    let mut args=vec![spec.source.witness_flag().to_owned(),nonce.to_owned()];
    for path in [allowed,denied] {args.push(path.to_str().ok_or_else(||anyhow::anyhow!("basic witness Unicode path"))?.to_owned());}
    if let Some(query)=spec.source.legacy_query() {
        args.push(query.path.to_str().ok_or_else(||anyhow::anyhow!("SID query Unicode path"))?.to_owned());
        args.push(crate::preflight::hex(&query.sha256));
    }
    args.push(spec.expected_sid.clone());args.push(spec.port.to_string());Ok(args)
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
        Witness::Basic(spec) => spec.source.witness_flag(),
        Witness::PipeEof => "--witness-eof",
        Witness::ConPtySize => "--witness-pty",
    };
    let tty = matches!(witness, Witness::ConPtySize);
    let mut argv = vec![helper.path.to_str().ok_or_else(|| anyhow::anyhow!("helper Unicode path"))?.to_owned()];
    if let Witness::Basic(spec) = witness {
        argv.extend(basic_arguments(lifecycle.synthetic_root(),nonce,spec)?);
    }else{argv.extend([mode.to_owned(),nonce.to_owned()]);}
    Ok(ExecParams {
        process_id: ProcessId::new(operation),
        metadata: None,
        argv,
        cwd,
        env_policy: None,
        shell_snapshot: None,
        env: lifecycle.environment()?,
        tty,
        pipe_stdin: !basic,
        arg0: None,
        sandbox: Some(sandbox),
        enforce_managed_network: false,
        managed_network: None,
        network_proxy: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basic_security::{IdentitySource,SecuritySpec};
    #[test]
    fn basic_argv_roundtrip_binds_explicit_primary_mode_and_preserves_legacy_query_binding() {
        let nonce="00112233445566778899aabbccddeeff";
        let root=std::path::Path::new(r"C:\memory-only-basic");
        let spec=SecuritySpec {source:IdentitySource::CurrentPrimaryTokenV8,
            expected_sid:"S-1-5-21-1-2-3-1001".into(),port:23456};
        let native=basic_arguments(root,nonce,&spec).unwrap();assert_eq!(native.len(),6);
        assert_eq!(native[0],"--witness-basic-v8-primary");
        let parsed=crate::basic_witness::security_from_args(&native).unwrap().unwrap();
        assert!(parsed.source.is_native_v8());assert!(parsed.source.legacy_query().is_none());
        assert_eq!(parsed.expected_sid,spec.expected_sid);assert_eq!(parsed.port,spec.port);
        let mut changed=spec.clone();changed.expected_sid="S-1-5-21-1-2-3-1002".into();
        assert_ne!(native,basic_arguments(root,nonce,&changed).unwrap());
        changed=spec.clone();changed.port+=1;assert_ne!(native,basic_arguments(root,nonce,&changed).unwrap());
        let mut legacy=spec;
        legacy.source=IdentitySource::Whoami(Artifact {path:crate::basic_security::SYSTEM_QUERY.into(),sha256:[7;32]});
        let old=basic_arguments(root,nonce,&legacy).unwrap();assert_eq!(old.len(),8);assert_eq!(old[0],"--witness-basic");
        let parsed=crate::basic_witness::security_from_args(&old).unwrap().unwrap();
        assert!(!parsed.source.is_native_v8());assert_eq!(parsed.source.legacy_query().unwrap().sha256,[7;32]);
        assert_ne!(native,old);
        let mut mixed=old;mixed[0]="--witness-basic-v8-primary".into();
        assert!(crate::basic_witness::security_from_args(&mixed).is_err());
    }
}
