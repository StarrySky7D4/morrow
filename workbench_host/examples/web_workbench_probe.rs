//! Run the same protocol fixture as the browser against a native audited host.
#[path = "support/browser_workbench_probe.rs"]
mod probe;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let archive=std::fs::read(&args[1])?;
    let package=morrow_core::plugin_package::Package::decode(&archive)?;
    let dir=tempfile::tempdir()?;let path=dir.path().join("library.db");
    let mut host=morrow_workbench_host::Workbench::open(&path,Some(package))?;
    probe::run(&mut |bytes|morrow_workbench_host::protocol::respond(&mut host,bytes),false)?;
    host.finish()?;drop(host);
    let mut host=morrow_workbench_host::Workbench::open(&path,Some(morrow_core::plugin_package::Package::decode(&archive)?))?;
    probe::run(&mut |bytes|morrow_workbench_host::protocol::respond(&mut host,bytes),true)?;
    host.finish()?;
    let key=morrow_core::audit::SigningKey::from_bytes(&[42;32]);
    std::fs::write(&args[2],key.verifying_key().to_bytes())?;
    println!("PASS: native shared protocol create/edit/conflict, segmented Unicode draft, locale, approval and reopen");
    Ok(())
}
