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
    loop {
        let mut header = [0; 4];
        match input.read_exact(&mut header) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        let size = u32::from_le_bytes(header) as usize;
        if size == 0 {
            break;
        }
        if size > 128 * 1024 {
            return Err("incoming frame budget".into());
        }
        let mut bytes = vec![0; size];
        input.read_exact(&mut bytes)?;
        let response = morrow_workbench_host::protocol::respond(&mut host, &bytes)?;
        output.write_all(&(response.len() as u32).to_le_bytes())?;
        output.write_all(&response)?;
        output.flush()?;
    }
    host.finish()?;
    Ok(())
}
