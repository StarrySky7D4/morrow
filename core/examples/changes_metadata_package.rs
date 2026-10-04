//! Explicit metadata-only extension pack entry. Does not change the old SDK tool.
#[cfg(not(target_arch="wasm32"))]
fn main()->Result<(),Box<dyn std::error::Error>> {
    use morrow_core::{channel,changes_metadata,plugin_package::{Package,MAX_MODULE_BYTES,proto::TransformHandler}};
    use std::{fs::File,io::{Read,Write},path::Path};
    let args:Vec<_>=std::env::args().skip(1).collect();
    let usage="changes_metadata_package pack MODULE OUTPUT ID VERSION PROFILE VERSION DIGEST | inspect PACKAGE";
    if args.first().is_some_and(|s|s=="inspect") && args.len()==2 {
        let p=morrow_core::plugin_package::catalog::read_file(Path::new(&args[1]))?;
        if !p.is_changes_metadata(){return Err("not a changes-metadata-v1 package".into());}
        println!("package={} sha256={} profile={}",p.manifest().package_id,hex(&p.digest()),hex(&changes_metadata::profile_digest()));
        return Ok(());
    }
    if args.len()!=8 || args[0]!="pack" {return Err(usage.into());}
    if args[5]!=changes_metadata::FEATURE || args[6]!=changes_metadata::VERSION.to_string()
        || args[7]!=hex(&changes_metadata::profile_digest()) {return Err("unknown profile/version/digest".into());}
    let mut module=Vec::new();File::open(&args[1])?.take(MAX_MODULE_BYTES as u64+1).read_to_end(&mut module)?;
    if module.len()>MAX_MODULE_BYTES {return Err("module too large".into());}
    let handler=TransformHandler{handler:"changes.metadata".into(),input_type:"morrow.channel.directory.v1".into(),output_type:"morrow.changes.metadata.count.v1".into(),max_input_bytes:morrow_core::task::MAX_VALUE_BYTES as u32,max_output_bytes:8};
    let mut manifest=Package::manifest_for_changes_metadata(&args[3],&args[4],&module,vec![handler]);
    let budget=channel::Budget{max_channels:1,max_frame_bytes:662,max_bytes:662*32,max_messages:32,max_requests:128,max_duration_ms:30_000};
    manifest.channel_declaration.as_mut().ok_or("missing channel declaration")?.budget=Some(budget.to_proto());
    let execution=manifest.budget.as_mut().ok_or("missing execution budget")?;
    execution.fuel=100_000_000;execution.memory_bytes=16*1024*1024;execution.host_calls=128;
    let package=Package::build(manifest,&module)?;
    if !package.is_changes_metadata(){return Err("metadata admission failed".into());}
    let target=Path::new(&args[2]);let parent=target.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut staged=tempfile::NamedTempFile::new_in(parent)?;staged.write_all(package.archive())?;staged.as_file().sync_all()?;staged.persist_noclobber(target)?;
    println!("published={} sha256={} profile={}",target.display(),hex(&package.digest()),hex(&changes_metadata::profile_digest()));
    Ok(())
}
#[cfg(not(target_arch="wasm32"))]
fn hex(bytes:&[u8])->String {bytes.iter().map(|b|format!("{b:02x}")).collect()}
#[cfg(target_arch="wasm32")]
fn main(){}
