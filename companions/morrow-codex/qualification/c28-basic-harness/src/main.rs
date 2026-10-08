//! Default file-only preflight; live guest actions require exact interactive steps.
#![forbid(unsafe_code)]
use anyhow::{Context, Result, bail, ensure};
use morrow_protected_checked_vm_harness::preflight::{self, Artifact, Config, digest};
use std::{collections::BTreeMap, path::PathBuf};

struct Arguments {
    files: Config,
    phase: String,
    shell: Option<Artifact>,
    sid_query: Option<Artifact>,
    expected_uuid: Option<String>,
}
fn arguments() -> Result<Arguments> {
    let mut args = std::env::args().skip(1);
    let mut values = BTreeMap::new();
    while let Some(key) = args.next() {
        ensure!(
            matches!(
                key.as_str(),
                "--guest-root"
                    | "--runner"
                    | "--runner-sha256"
                    | "--setup"
                    | "--setup-sha256"
                    | "--helper"
                    | "--helper-sha256"
                    | "--phase"
                    | "--inventory-shell"
                    | "--inventory-shell-sha256"
                    | "--expected-guest-uuid"
                    | "--sid-query"
                    | "--sid-query-sha256"
            ),
            "unknown argument; no environment defaults or permission booleans"
        );
        let value = args.next().context("argument requires an explicit value")?;
        ensure!(
            value.len() <= 4096 && values.insert(key, value).is_none(),
            "duplicate or oversized argument"
        );
    }
    let mut take = |key: &str| values.remove(key).with_context(|| format!("missing {key}"));
    let root = PathBuf::from(take("--guest-root")?);
    let runner = Artifact {
        path: PathBuf::from(take("--runner")?),
        sha256: digest(&take("--runner-sha256")?)?,
    };
    let setup = Artifact {
        path: PathBuf::from(take("--setup")?),
        sha256: digest(&take("--setup-sha256")?)?,
    };
    let helper = Artifact {
        path: PathBuf::from(take("--helper")?),
        sha256: digest(&take("--helper-sha256")?)?,
    };
    let phase = values
        .remove("--phase")
        .unwrap_or_else(|| "preflight".into());
    let expected_uuid = values.remove("--expected-guest-uuid");
    let shell = match (
        values.remove("--inventory-shell"),
        values.remove("--inventory-shell-sha256"),
    ) {
        (None, None) => None,
        (Some(path), Some(sha)) => Some(Artifact {
            path: PathBuf::from(path),
            sha256: digest(&sha)?,
        }),
        _ => bail!("inventory shell path and full SHA must both be explicit"),
    };
    let sid_query = match (values.remove("--sid-query"), values.remove("--sid-query-sha256")) {
        (None, None) => None,
        (Some(path), Some(sha)) => Some(Artifact { path: PathBuf::from(path), sha256: digest(&sha)? }),
        _ => bail!("guest SID query path and full guest-observed SHA must both be explicit"),
    };
    ensure!(values.is_empty(), "unconsumed argument");
    Ok(Arguments {
        files: Config {
            synthetic_root: root,
            runner,
            setup,
            helper,
        },
        phase,
        shell,
        sid_query,
        expected_uuid,
    })
}

fn run() -> Result<()> {
    let child_args: Vec<_> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    if let Some(code) = morrow_protected_checked_vm_harness::basic_witness::run_child_if_requested(&child_args)? {
        std::process::exit(code);
    }
    if let Some(code) =
        morrow_protected_checked_vm_harness::witness::run_child_if_requested(&child_args)?
    {
        std::process::exit(code);
    }
    let args = arguments()?;
    // This branch never creates an owner, scheduler, backend, shell or VM connection.
    if args.phase == "preflight" {
        ensure!(
            args.shell.is_none() && args.expected_uuid.is_none() && args.sid_query.is_none(),
            "guest identity arguments require the explicit interactive phase"
        );
        println!(
            "{}",
            serde_json::to_string(&preflight::preflight(&args.files)?)?
        );
        return Ok(());
    }
    ensure!(
        args.phase == "live-guest-interactive",
        "unknown phase; no action"
    );
    #[cfg(windows)]
    {
        use morrow_protected_checked_vm_harness::live::{LiveConfig, Session};
        let config = LiveConfig {
            files: args.files,
            sid_query: args.sid_query,
            inventory_shell: args
                .shell
                .context("live guest phase requires the pinned inventory shell")?,
            expected_guest_uuid: args
                .expected_uuid
                .context("live guest phase requires the independently approved firmware UUID")?,
        };
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        let mut session = Session::begin(config, &mut input)?;
        match session.interact(&mut input) {
            Ok(()) => Ok(()),
            Err(error) => session.retain_after_disconnect(&error),
        }
    }
    #[cfg(not(windows))]
    bail!("live guest entry is Windows-only; no fallback");
}

fn main() {
    if let Err(error) = run() {
        eprintln!(
            "{}",
            serde_json::json!({"status":"FAIL_PRESERVED_NO_AUTOMATIC_RETRY","message":error.to_string(),"production_qualified":false,"protected_session_qualified":false,"sdk_complete":false})
        );
        std::process::exit(1);
    }
}
