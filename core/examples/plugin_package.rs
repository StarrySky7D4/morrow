//! Offline developer packaging tool. Inspection/install never executes guest code.
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use morrow_core::plugin_package::{
        DEPENDENCIES_FEATURE, DEPENDENCY_CALLS_FEATURE, MAX_MODULE_BYTES, Package,
        catalog::{self, Catalog},
        io::{self, IoCapability},
        proto::{Capability, DependencyRequirement, TransformHandler},
    };
    use std::{
        collections::BTreeSet,
        fs::File,
        io::{Read, Write},
        path::{Path, PathBuf},
    };
    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
    const USAGE: &str = "usage: plugin_package pack|pack-task MODULE OUTPUT ID VERSION CAPS; pack-transform MODULE OUTPUT ID VERSION HANDLERS; pack-v2|pack-v2-catalog MODULE OUTPUT ID VERSION [--name VALUE] [--capability NAME] [--handler NAME INPUT OUTPUT MAX_INPUT MAX_OUTPUT] [--dependency SLOT HANDLER INPUT OUTPUT VERSION_RANGE required|optional] [--dependency-calls] [--io-capability file-read|http-request|credential-use] [--io-handler NAME] [--fuel N] [--memory-bytes N] [--host-calls N]; inspect PACKAGE; install PACKAGE CATALOG";
    fn capability(name: &str) -> Result<Capability> {
        Ok(match name {
            "rename"=>Capability::RenameCard,"summary"=>Capability::ReadSummary,
            "operation"=>Capability::QueryOperation,"attachment"=>Capability::ReadAttachment,
            "create-content"=>Capability::CreateContent,"edit-content"=>Capability::EditContent,
            "read-content"=>Capability::ReadContent,
            _=>return Err(format!("unknown capability '{name}'; expected rename, summary, operation, attachment, create-content, edit-content or read-content").into()),
        })
    }
    fn io_capability(name: &str) -> Result<IoCapability> {
        Ok(match name {
            "file-read" => IoCapability::FileRead,
            "http-request" => IoCapability::HttpRequest,
            "credential-use" => IoCapability::CredentialUse,
            _ => return Err(format!(
                "unknown IO capability '{name}'; expected file-read, http-request or credential-use"
            )
            .into()),
        })
    }
    fn read_module(path: &str) -> Result<Vec<u8>> {
        let mut module = Vec::new();
        File::open(path)?
            .take(MAX_MODULE_BYTES as u64 + 1)
            .read_to_end(&mut module)?;
        if module.len() > MAX_MODULE_BYTES {
            return Err("module exceeds 4 MiB".into());
        }
        Ok(module)
    }
    fn publish_file(package: &Package, target: &Path) -> Result<PathBuf> {
        let parent = target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut staged = tempfile::NamedTempFile::new_in(parent)?;
        staged.write_all(package.archive())?;
        staged.as_file().sync_all()?;
        staged.persist_noclobber(target)?;
        Ok(target.canonicalize()?)
    }
    fn value<'a>(
        args: &'a [String],
        position: &mut usize,
        flag: &str,
        label: &str,
    ) -> Result<&'a str> {
        let result = args
            .get(*position)
            .filter(|v| !v.starts_with("--"))
            .ok_or_else(|| format!("{flag} requires {label}"))?;
        *position += 1;
        Ok(result)
    }
    fn number(text: &str, flag: &str, min: u64, max: u64) -> Result<u64> {
        let n = text.parse::<u64>().map_err(|_| {
            format!("{flag} expects an unsigned integer in {min}..={max}, got '{text}'")
        })?;
        if !(min..=max).contains(&n) {
            return Err(format!("{flag} expects {min}..={max}, got {n}").into());
        }
        Ok(n)
    }
    #[derive(Default)]
    struct Options {
        name: Option<String>,
        caps: Vec<Capability>,
        handlers: Vec<TransformHandler>,
        dependencies: Vec<DependencyRequirement>,
        dependency_calls: bool,
        io_caps: Vec<IoCapability>,
        io_handlers: Vec<String>,
        fuel: Option<u64>,
        memory: Option<u64>,
        calls: Option<u32>,
    }
    fn options(args: &[String]) -> Result<Options> {
        let mut o = Options::default();
        let mut seen = BTreeSet::new();
        let mut i = 0;
        while i < args.len() {
            let flag = args[i].as_str();
            i += 1;
            if matches!(
                flag,
                "--name" | "--dependency-calls" | "--fuel" | "--memory-bytes" | "--host-calls"
            ) && !seen.insert(flag)
            {
                return Err(format!("duplicate single-value option {flag}").into());
            }
            match flag {
                "--name" => o.name = Some(value(args, &mut i, flag, "VALUE")?.into()),
                "--capability" => o.caps.push(capability(value(args, &mut i, flag, "NAME")?)?),
                "--handler" => {
                    let handler = value(args, &mut i, flag, "NAME")?.into();
                    let input_type = value(args, &mut i, flag, "INPUT_TYPE")?.into();
                    let output_type = value(args, &mut i, flag, "OUTPUT_TYPE")?.into();
                    let max_input_bytes = number(
                        value(args, &mut i, flag, "MAX_INPUT")?,
                        "--handler MAX_INPUT",
                        0,
                        65536,
                    )? as u32;
                    let max_output_bytes = number(
                        value(args, &mut i, flag, "MAX_OUTPUT")?,
                        "--handler MAX_OUTPUT",
                        0,
                        65536,
                    )? as u32;
                    o.handlers.push(TransformHandler {
                        handler,
                        input_type,
                        output_type,
                        max_input_bytes,
                        max_output_bytes,
                    });
                }
                "--dependency" => {
                    let slot = value(args, &mut i, flag, "SLOT")?.into();
                    let handler = value(args, &mut i, flag, "HANDLER")?.into();
                    let input_type = value(args, &mut i, flag, "INPUT_TYPE")?.into();
                    let output_type = value(args, &mut i, flag, "OUTPUT_TYPE")?.into();
                    let provider_version = value(args, &mut i, flag, "VERSION_RANGE")?.into();
                    let optional = match value(args, &mut i, flag, "required|optional")? {
                        "required" => false,
                        "optional" => true,
                        v => {
                            return Err(format!(
                                "--dependency expects required|optional, got '{v}'"
                            )
                            .into());
                        }
                    };
                    o.dependencies.push(DependencyRequirement {
                        slot,
                        handler,
                        input_type,
                        output_type,
                        provider_version,
                        optional,
                    });
                }
                "--dependency-calls" => o.dependency_calls = true,
                "--io-capability" => o
                    .io_caps
                    .push(io_capability(value(args, &mut i, flag, "NAME")?)?),
                "--io-handler" => o
                    .io_handlers
                    .push(value(args, &mut i, flag, "NAME")?.into()),
                "--fuel" => {
                    o.fuel = Some(number(
                        value(args, &mut i, flag, "N")?,
                        flag,
                        1,
                        100_000_000,
                    )?)
                }
                "--memory-bytes" => {
                    let n = number(
                        value(args, &mut i, flag, "N")?,
                        flag,
                        65536,
                        64 * 1024 * 1024,
                    )?;
                    if n % 65536 != 0 {
                        return Err("--memory-bytes must be a multiple of 65536".into());
                    }
                    o.memory = Some(n);
                }
                "--host-calls" => {
                    o.calls = Some(number(value(args, &mut i, flag, "N")?, flag, 0, 1024)? as u32)
                }
                _ => return Err(format!("unknown option '{flag}'").into()),
            }
        }
        if o.dependency_calls && o.handlers.is_empty() {
            return Err("--dependency-calls requires at least one --handler".into());
        }
        if o.io_caps.is_empty() != o.io_handlers.is_empty() {
            return Err("IO declaration requires both --io-capability and --io-handler".into());
        }
        if o.io_caps.contains(&IoCapability::CredentialUse)
            && !o.io_caps.contains(&IoCapability::HttpRequest)
        {
            return Err("credential-use requires http-request".into());
        }
        if !o.io_caps.is_empty()
            && (!o.caps.is_empty()
                || !o.handlers.is_empty()
                || !o.dependencies.is_empty()
                || o.dependency_calls)
        {
            return Err(
                "IO profile cannot be mixed with content capabilities, pure handlers or dependency calls by this tool"
                    .into(),
            );
        }
        Ok(o)
    }
    fn pack_v2(args: &[String]) -> Result<Package> {
        let o = options(&args[5..])?;
        let module = read_module(&args[1])?;
        let mut manifest = if o.handlers.is_empty() {
            Package::manifest_for_task(&args[3], &args[4], &module, o.caps.clone())
        } else {
            let mut manifest =
                Package::manifest_for_transform(&args[3], &args[4], &module, o.handlers);
            manifest.requested_capabilities = o.caps.iter().map(|v| *v as i32).collect();
            manifest
        };
        if let Some(name) = o.name {
            manifest.display_name = name;
        }
        if !o.dependencies.is_empty() || o.dependency_calls {
            manifest.required_features.push(DEPENDENCIES_FEATURE.into());
        }
        manifest.dependencies = o.dependencies;
        if o.dependency_calls {
            manifest
                .required_features
                .push(DEPENDENCY_CALLS_FEATURE.into());
            manifest.dependency_schema_sha256 =
                morrow_core::dependency_call::schema_digest().to_vec();
        }
        if !o.io_caps.is_empty() {
            manifest.required_features.push(io::FEATURE.into());
            let mut declaration = io::declaration(o.io_caps, o.io_handlers);
            let io_budget = declaration
                .budget
                .as_mut()
                .expect("IO declaration factory supplies budget");
            io_budget.max_resources = 2;
            io_budget.max_jobs = 1;
            io_budget.max_bytes = 1024 * 1024;
            io_budget.max_job_bytes = 1024 * 1024;
            manifest.io_declaration = Some(declaration);
        }
        let budget = manifest
            .budget
            .as_mut()
            .expect("manifest factory supplies budget");
        if let Some(fuel) = o.fuel {
            budget.fuel = fuel;
        }
        if let Some(memory) = o.memory {
            budget.memory_bytes = memory;
        }
        if let Some(calls) = o.calls {
            budget.host_calls = calls;
        }
        Ok(Package::build(manifest, &module)?)
    }
    fn legacy(args: &[String]) -> Result<Package> {
        let module = read_module(&args[1])?;
        let caps = if args[0] == "pack-transform" || args[5] == "none" {
            vec![]
        } else {
            args[5]
                .split(',')
                .map(capability)
                .collect::<Result<Vec<_>>>()?
        };
        let manifest = if args[0] == "pack-transform" {
            let handlers = args[5]
                .split(';')
                .map(|s| {
                    let fields: Vec<_> = s.split(',').collect();
                    if fields.len() != 5 {
                        return Err(
                            "handler requires name,input_type,output_type,max_input,max_output"
                                .into(),
                        );
                    }
                    Ok(TransformHandler {
                        handler: fields[0].into(),
                        input_type: fields[1].into(),
                        output_type: fields[2].into(),
                        max_input_bytes: fields[3].parse()?,
                        max_output_bytes: fields[4].parse()?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Package::manifest_for_transform(&args[3], &args[4], &module, handlers)
        } else if args[0] == "pack-task" {
            Package::manifest_for_task(&args[3], &args[4], &module, caps)
        } else {
            Package::manifest_for(&args[3], &args[4], &module, caps)
        };
        Ok(Package::build(manifest, &module)?)
    }
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
    fn describe(package: &Package, out: &mut impl Write) -> Result<()> {
        let m = package.manifest();
        writeln!(
            out,
            "{} {} archive-sha256={} module-bytes={} capabilities={:?}",
            m.package_id,
            m.package_version,
            hex(&package.digest()),
            package.module().len(),
            package.capabilities()
        )?;
        writeln!(
            out,
            "name={:?} guest-abi={} features={:?}",
            m.display_name, m.guest_abi_version, m.required_features
        )?;
        if let Some(b) = &m.budget {
            writeln!(
                out,
                "fuel={} memory-bytes={} host-calls={}",
                b.fuel, b.memory_bytes, b.host_calls
            )?;
        }
        writeln!(
            out,
            "runtime-schema-sha256={} content-schema-sha256={} task-schema-sha256={} dependency-schema-sha256={}",
            hex(&m.runtime_schema_sha256),
            hex(&m.content_schema_sha256),
            hex(&m.task_schema_sha256),
            hex(&m.dependency_schema_sha256)
        )?;
        for h in &m.transform_handlers {
            writeln!(
                out,
                "handler={} input={} output={} max-input={} max-output={}",
                h.handler, h.input_type, h.output_type, h.max_input_bytes, h.max_output_bytes
            )?;
        }
        for d in &m.dependencies {
            writeln!(
                out,
                "dependency={} handler={} input={} output={} version={:?} {}",
                d.slot,
                d.handler,
                d.input_type,
                d.output_type,
                d.provider_version,
                if d.optional { "optional" } else { "required" }
            )?;
        }
        if let Some(declaration) = &m.io_declaration {
            writeln!(
                out,
                "io-version={} io-schema-sha256={} io-capabilities={:?} io-handlers={:?} (declarations only; no grants)",
                declaration.io_version,
                hex(&declaration.io_schema_sha256),
                package.io_capabilities(),
                declaration.handlers
            )?;
            if let Some(b) = &declaration.budget {
                writeln!(
                    out,
                    "io-budget resources={} jobs={} bytes={} job-bytes={} duration-ms={}",
                    b.max_resources, b.max_jobs, b.max_bytes, b.max_job_bytes, b.max_duration_ms
                )?;
            }
        }
        writeln!(
            out,
            "Container integrity only; no guest execution, author trust, authorization, runtime preparation or activation implied."
        )?;
        Ok(())
    }
    pub(super) fn run(args: &[String], out: &mut impl Write) -> Result<()> {
        let package = match args.first().map(String::as_str) {
            Some("pack-v2" | "pack-v2-catalog") if args.len() >= 5 => {
                // Parse and validate everything before creating the destination directory/file.
                let p = pack_v2(args)?;
                let path = if args[0] == "pack-v2-catalog" {
                    Catalog::open(Path::new(&args[2]))?.install(&p)?
                } else {
                    publish_file(&p, Path::new(&args[2]))?
                };
                writeln!(out, "published {}", path.display())?;
                p
            }
            Some("pack" | "pack-task" | "pack-transform") if args.len() == 6 => {
                let p = legacy(args)?;
                publish_file(&p, Path::new(&args[2]))?;
                p
            }
            Some("inspect") if args.len() == 2 => catalog::read_file(Path::new(&args[1]))?,
            Some("install") if args.len() == 3 => {
                let p = catalog::read_file(Path::new(&args[1]))?;
                let catalog = Catalog::open(Path::new(&args[2]))?;
                let path = catalog.install(&p)?;
                writeln!(out, "installed {}", path.display())?;
                catalog.load(p.digest())?
            }
            _ => return Err(USAGE.into()),
        };
        describe(&package, out)
    }
    #[cfg(test)]
    mod tests {
        include!("support/plugin_package_tests.rs");
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run(
        &std::env::args().skip(1).collect::<Vec<_>>(),
        &mut std::io::stdout().lock(),
    )
}
#[cfg(target_arch = "wasm32")]
fn main() {}
