//! Offline developer packaging tool. Inspection/install never executes guest code.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use morrow_core::plugin_package::{
        MAX_MODULE_BYTES, Package,
        catalog::{self, Catalog},
        proto::Capability,
    };
    use std::{
        fs::File,
        io::{Read, Write},
        path::Path,
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    let package=match args.first().map(String::as_str) {
        Some("pack") if args.len()==6 => {
            let mut module=Vec::new();
            File::open(&args[1])?.take(MAX_MODULE_BYTES as u64+1).read_to_end(&mut module)?;
            if module.len()>MAX_MODULE_BYTES {return Err("module too large".into());}
            let caps=if args[5]=="none" {vec![]} else {args[5].split(',').map(|s|match s {
                "rename"=>Ok(Capability::RenameCard),"summary"=>Ok(Capability::ReadSummary),
                "operation"=>Ok(Capability::QueryOperation),"attachment"=>Ok(Capability::ReadAttachment),
                _=>Err("unknown capability")}).collect::<Result<Vec<_>,_>>()?};
            let package=Package::build(Package::manifest_for(&args[3],&args[4],&module,caps),&module)?;
            let target=Path::new(&args[2]);
            let parent=target.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or(Path::new("."));
            let mut staged=tempfile::NamedTempFile::new_in(parent)?;
            staged.write_all(package.archive())?;staged.as_file().sync_all()?;
            staged.persist_noclobber(target)?;
            package
        },
        Some("inspect") if args.len()==2 => catalog::read_file(Path::new(&args[1]))?,
        Some("install") if args.len()==3 => {
            let package=catalog::read_file(Path::new(&args[1]))?;
            let catalog=Catalog::open(Path::new(&args[2]))?;
            let path=catalog.install(&package)?;
            println!("installed {}",path.display());
            catalog.load(package.digest())?
        },
        _=>return Err("usage: plugin_package pack module.wasm output.mplugin id semver rename,summary,operation,attachment|none; inspect package; install package catalog".into())
    };
    let digest = package
        .digest()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    println!(
        "{} {} archive-sha256={} module-bytes={} capabilities={:?}",
        package.manifest().package_id,
        package.manifest().package_version,
        digest,
        package.module().len(),
        package.capabilities()
    );
    println!(
        "Container integrity only; no author trust, runtime preparation or activation implied."
    );
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {}
