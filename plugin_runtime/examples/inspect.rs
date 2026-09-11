//! Read-only developer inspection; this does not grant authority or execute a module.
use std::io::Read;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let mut bytes = Vec::new();
        std::fs::File::open(&path)?
            .take(morrow_plugin_runtime::MAX_MODULE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > morrow_plugin_runtime::MAX_MODULE_BYTES {
            return Err("module size limit".into());
        }
        let mut config = wasmi::Config::default();
        config.enforced_limits(wasmi::EnforcedLimits::strict());
        let engine = wasmi::Engine::new(&config);
        let module = wasmi::Module::new(&engine, &bytes)?;
        println!("{path} ({} bytes)", bytes.len());
        for import in module.imports() {
            println!(
                "  import {}.{} {:?}",
                import.module(),
                import.name(),
                import.ty()
            );
        }
    }
    Ok(())
}
