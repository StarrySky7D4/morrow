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
        Some("pack"|"pack-task"|"pack-transform") if args.len()==6 => {
            let mut module=Vec::new();
            File::open(&args[1])?.take(MAX_MODULE_BYTES as u64+1).read_to_end(&mut module)?;
            if module.len()>MAX_MODULE_BYTES {return Err("module too large".into());}
            let caps=if args[0]=="pack-transform" || args[5]=="none" {vec![]} else {args[5].split(',').map(|s|match s {
                "rename"=>Ok(Capability::RenameCard),"summary"=>Ok(Capability::ReadSummary),
                "operation"=>Ok(Capability::QueryOperation),"attachment"=>Ok(Capability::ReadAttachment),
                _=>Err("unknown capability")}).collect::<Result<Vec<_>,_>>()?};
            let manifest=if args[0]=="pack-transform" {
                let handlers=args[5].split(';').map(|s| {
                    let fields:Vec<_>=s.split(',').collect();
                    if fields.len()!=5 {return Err("handler requires name,input_type,output_type,max_input,max_output".into());}
                    Ok(morrow_core::plugin_package::proto::TransformHandler {
                        handler:fields[0].into(),input_type:fields[1].into(),output_type:fields[2].into(),
                        max_input_bytes:fields[3].parse()?,max_output_bytes:fields[4].parse()?,
                    })
                }).collect::<Result<Vec<_>,Box<dyn std::error::Error>>>()?;
                Package::manifest_for_transform(&args[3],&args[4],&module,handlers)
            } else if args[0]=="pack-task"{Package::manifest_for_task(&args[3],&args[4],&module,caps)}else{Package::manifest_for(&args[3],&args[4],&module,caps)};
            let package=Package::build(manifest,&module)?;
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
        _=>return Err("usage: plugin_package pack|pack-task module.wasm output.mplugin id semver rename,summary,operation,attachment|none; pack-transform module.wasm output.mplugin id semver handler,input_type,output_type,max_input,max_output;...; inspect package; install package catalog".into())
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
    for handler in &package.manifest().transform_handlers {
        println!(
            "handler={} input={} output={} max-input={} max-output={}",
            handler.handler,
            handler.input_type,
            handler.output_type,
            handler.max_input_bytes,
            handler.max_output_bytes
        );
    }
    println!(
        "Container integrity only; no author trust, runtime preparation or activation implied."
    );
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {}
