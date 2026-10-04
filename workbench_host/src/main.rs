#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use std::io::{Read, Write};
fn main() {
    // Exact diagnostic mode exits before legacy argument parsing, product gates,
    // owner supervision, database opening, package installation or guest use.
    let mut diagnostic_args = std::env::args_os().skip(1);
    if diagnostic_args.next().as_deref()
        == Some(std::ffi::OsStr::new(
            morrow_workbench_host::sdk_preflight::COMMAND,
        ))
    {
        let result = morrow_workbench_host::sdk_preflight::command(diagnostic_args);
        println!("{}", result.json);
        std::process::exit(i32::from(result.exit_code));
    }
    if let Err(e) = run() {
        eprintln!("Morrow workbench host: {e}");
        std::process::exit(1);
    }
}
fn run() -> morrow_workbench_host::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut database = args.next().ok_or("database path")?;
    // Developer discovery is read-only and occurs before any owner/database gate.
    if database == "--sdk-capabilities" {
        if args.next().is_some() {
            return Err("unexpected SDK discovery arguments".into());
        }
        println!("{}", morrow_workbench_host::sdk_profiles::descriptor());
        return Ok(());
    }
    let supervised_incarnation = if database == "--supervised-owner" {
        let value = args.next().ok_or("supervised owner identity")?;
        database = args.next().ok_or("database path")?;
        let ui = args.next().ok_or("supervised UI PID")?.parse::<u32>()?;
        let supervisor = args.next().ok_or("supervisor PID")?.parse::<u32>()?;
        Some((value, ui, supervisor))
    } else {
        None
    };
    if database == "--activate-library" || database == "--restore-active-key" {
        let root = args.next().ok_or("library root")?;
        let selected = args.next().ok_or("selected path")?;
        if args.next().is_some() {
            return Err("unexpected library arguments".into());
        }
        #[cfg(target_os = "windows")]
        morrow_workbench_host::workbench_supervision::guard(
            std::path::Path::new(&root),
            true,
            None,
        )?;
        return if database == "--activate-library" {
            morrow_workbench_host::activate_library(
                std::path::Path::new(&root),
                std::path::Path::new(&selected),
            )
        } else {
            morrow_workbench_host::restore_active_key(
                std::path::Path::new(&root),
                std::path::Path::new(&selected),
            )
        };
    }
    if database == "--restore-snapshot" {
        let archive = args.next().ok_or("snapshot archive")?;
        let destination = args.next().ok_or("new restore directory")?;
        if args.next().is_some() {
            return Err("unexpected snapshot arguments".into());
        }
        return morrow_workbench_host::restore_snapshot(
            std::path::Path::new(&archive),
            std::path::Path::new(&destination),
        );
    }
    if database == "--restore-key" {
        let database = args.next().ok_or("database path")?;
        let selected = args.next().ok_or("original protected key path")?;
        if args.next().is_some() {
            return Err("unexpected recovery arguments".into());
        }
        #[cfg(target_os = "windows")]
        morrow_workbench_host::workbench_supervision::guard(
            std::path::Path::new(&database),
            false,
            None,
        )?;
        return morrow_workbench_host::restore_key(
            std::path::Path::new(&database),
            std::path::Path::new(&selected),
        );
    }
    let managed = database == "--managed";
    if managed {
        database = args.next().ok_or("library root")?;
    }
    let product_gate = morrow_workbench_host::product_gate::ProductGate::default();
    #[cfg(target_os = "windows")]
    let _supervised_watch = if let Some((ref incarnation, ui, pid)) = supervised_incarnation {
        let root = morrow_workbench_host::workbench_supervision::root_for(
            std::path::Path::new(&database),
            managed,
        )?;
        Some(
            morrow_workbench_host::workbench_supervision::HostWatch::start(
                root,
                incarnation.clone(),
                [ui, pid],
                product_gate.clone(),
            )?,
        )
    } else {
        None
    };
    product_gate.check()?;
    #[cfg(target_os = "windows")]
    {
        let requested = std::path::Path::new(&database);
        let parent = if managed {
            requested
        } else {
            requested.parent().unwrap_or(std::path::Path::new("."))
        };
        if parent.join("MIGRATION_INCOMPLETE.txt").exists() {
            return Err("迁移目标尚未完成验证，请保留原库并查看 MIGRATION_INCOMPLETE.txt。".into());
        }
        if managed {
            let registry = morrow_audit::library::Registry::open(requested)?;
            let selected = registry.selected_database()?;
            if selected
                .parent()
                .unwrap()
                .join("MIGRATION_INCOMPLETE.txt")
                .exists()
            {
                return Err(
                    "迁移目标尚未完成验证，请保留原库并查看 MIGRATION_INCOMPLETE.txt。".into(),
                );
            }
        }
    }
    #[cfg(target_os = "windows")]
    morrow_workbench_host::workbench_supervision::guard(
        std::path::Path::new(&database),
        managed,
        supervised_incarnation.as_ref().map(|v| v.0.as_str()),
    )?;
    #[cfg(target_os = "windows")]
    let expected_binding = if let Some((ref token, _, _)) = supervised_incarnation {
        Some(morrow_workbench_host::workbench_supervision::binding(
            std::path::Path::new(&database),
            managed,
            token,
        )?)
    } else {
        None
    };
    let package = args.next().ok_or("package path")?;
    if args.next().is_some() {
        return Err("unexpected host arguments".into());
    }
    let package = if std::path::Path::new(&package).is_file() {
        let mut package_bytes = Vec::new();
        std::fs::File::open(&package)?
            .take(morrow_core::plugin_package::MAX_PACKAGE_BYTES as u64 + 1)
            .read_to_end(&mut package_bytes)?;
        #[cfg(target_os = "windows")]
        if expected_binding.as_ref().is_some_and(|r| {
            r["package_sha256"] != morrow_workbench_host::workbench_supervision::sha(&package_bytes)
        }) {
            return Err("Supervisor package artifact differs from owner binding".into());
        }
        product_gate.check()?;
        match morrow_core::plugin_package::Package::decode(&package_bytes) {
            Ok(package) => Some(package),
            Err(error) => {
                eprintln!("工作台插件包未通过校验，将以只读方式打开已有内容。{error}");
                None
            }
        }
    } else {
        #[cfg(target_os = "windows")]
        if expected_binding.is_some() {
            return Err("Supervised package artifact is missing".into());
        }
        None
    };
    product_gate.check()?;
    let mut host = if managed {
        morrow_workbench_host::Workbench::open_managed(std::path::Path::new(&database), package)?
    } else {
        morrow_workbench_host::Workbench::open(std::path::Path::new(&database), package)?
    };
    host.bind_product_gate(product_gate.clone());
    #[cfg(windows)]
    if _supervised_watch.is_some() && expected_binding.is_some() {
        host.bind_supervised_channel_owner()?;
    }
    product_gate.check()?;
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    serve(&mut host, &mut input, &mut output)
}

/// Every ordinary transport exit drains the original worker before the process
/// drops the library's ownership guards. Requests are never retried here.
fn serve(
    host: &mut morrow_workbench_host::Workbench,
    input: &mut impl Read,
    output: &mut impl Write,
) -> morrow_workbench_host::Result<()> {
    let transport = frames(host, input, output);
    let shutdown = finish_wait(host);
    match (transport, shutdown) {
        (Ok(()), result) | (result, Ok(())) => result,
        (Err(transport), Err(shutdown)) => {
            Err(format!("{transport}; host shutdown failed: {shutdown}").into())
        }
    }
}

fn frames(
    host: &mut morrow_workbench_host::Workbench,
    input: &mut impl Read,
    output: &mut impl Write,
) -> morrow_workbench_host::Result<()> {
    loop {
        let mut header = [0; 4];
        // Only EOF before the first byte is a clean boundary. A partial header
        // is a transport error, but must still take the same draining exit.
        loop {
            match input.read(&mut header[..1]) {
                Ok(0) => return Ok(()),
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        input.read_exact(&mut header[1..])?;
        let size = u32::from_le_bytes(header) as usize;
        if size == 0 {
            return Ok(());
        }
        if size > 128 * 1024 {
            return Err("incoming frame budget".into());
        }
        let mut bytes = zeroize::Zeroizing::new(vec![0; size]);
        input.read_exact(&mut bytes)?;
        host.product_gate().check()?;
        let response =
            zeroize::Zeroizing::new(morrow_workbench_host::protocol::respond(host, &bytes)?);
        output.write_all(&(response.len() as u32).to_le_bytes())?;
        output.write_all(&response)?;
        output.flush()?;
    }
}

fn finish_wait(host: &mut morrow_workbench_host::Workbench) -> morrow_workbench_host::Result<()> {
    loop {
        match host.finish() {
            Err(error)
                if error.downcast_ref::<morrow_workbench_host::io_tasks::AccessError>()
                    == Some(&morrow_workbench_host::io_tasks::AccessError::Busy) =>
            {
                // Cancellation is a request, not a joined thread. In particular,
                // a synchronous router can still own the audited Storage here.
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            // finish already attempts maintenance after actual reclamation.
            // Report failed cleanup/sealing without implicit repair or replay.
            result => return result,
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
#[path = "../tests/support/cli_shutdown.rs"]
mod shutdown_tests;
