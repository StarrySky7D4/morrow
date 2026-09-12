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
    let database = args.next().ok_or("database path")?;
    let package = args.next().ok_or("package path")?;
    let package = if std::path::Path::new(&package).is_file() {
        Some(morrow_core::plugin_package::catalog::read_file(
            std::path::Path::new(&package),
        )?)
    } else {
        None
    };
    let mut host =
        morrow_workbench_host::Workbench::open(std::path::Path::new(&database), package)?;
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
