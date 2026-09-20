#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use std::io::{Read, Write};
fn main() {
    if let Err(e) = run() {
        eprintln!("Morrow workbench host: {e}");
        std::process::exit(1);
    }
}
fn run() -> morrow_workbench_host::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut database = args.next().ok_or("database path")?;
    if database == "--activate-library" || database == "--restore-active-key" {
        let root = args.next().ok_or("library root")?;
        let selected = args.next().ok_or("selected path")?;
        if args.next().is_some() {
            return Err("unexpected library arguments".into());
        }
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
        return morrow_workbench_host::restore_key(
            std::path::Path::new(&database),
            std::path::Path::new(&selected),
        );
    }
    let managed = database == "--managed";
    if managed {
        database = args.next().ok_or("library root")?;
    }
    let package = args.next().ok_or("package path")?;
    if args.next().is_some() {
        return Err("unexpected host arguments".into());
    }
    let package = if std::path::Path::new(&package).is_file() {
        match morrow_core::plugin_package::catalog::read_file(std::path::Path::new(&package)) {
            Ok(package) => Some(package),
            Err(error) => {
                eprintln!("工作台插件包未通过校验，将以只读方式打开已有内容。{error}");
                None
            }
        }
    } else {
        None
    };
    let mut host = if managed {
        morrow_workbench_host::Workbench::open_managed(std::path::Path::new(&database), package)?
    } else {
        morrow_workbench_host::Workbench::open(std::path::Path::new(&database), package)?
    };
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
        let response = morrow_workbench_host::protocol::respond(host, &bytes)?;
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
