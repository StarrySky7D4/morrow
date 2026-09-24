//! Local developer qualification, not package installation, approval, or a production sandbox.
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::catalog,
    store::Store,
    task::{FailureCode, Invocation, MAX_VALUE_BYTES, Transform},
};
use morrow_plugin_runtime::{Cancellation, Limits, package::PreparedPackage};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    process::ExitCode,
};

const USAGE: &str = "usage: plugin_check check PACKAGE | plugin_check transform PACKAGE HANDLER INPUT_TYPE OUTPUT_TYPE INPUT_FILE OUTPUT_FILE";

#[derive(Debug)]
struct CliError {
    code: u8,
    message: String,
}
impl CliError {
    fn new(code: u8, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
type Result<T> = std::result::Result<T, CliError>;

fn text(value: &OsString) -> Result<&str> {
    value
        .to_str()
        .ok_or_else(|| CliError::new(2, "argument is not valid UTF-8"))
}
fn prepare(path: &Path) -> Result<PreparedPackage> {
    let package = catalog::read_file(path)
        .map_err(|error| CliError::new(2, format!("package rejected: {error}")))?;
    PreparedPackage::new(package, Limits::default())
        .map_err(|fault| CliError::new(2, format!("prepare rejected: {fault:?}")))
}
fn check(path: &Path) -> Result<String> {
    let prepared = prepare(path)?;
    let package = prepared.package();
    let manifest = package.manifest();
    let effective = prepared.limits();
    let mut lines = vec![
        "PREPARED: checked only; not executed, installed, or authorized".to_owned(),
        format!(
            "package={} version={}",
            manifest.package_id, manifest.package_version
        ),
        format!("package_sha256={:x}", Sha256::digest(package.archive())),
        format!(
            "manifest_schema={} guest_abi={} runtime_protocol={}",
            manifest.schema_version, manifest.guest_abi_version, manifest.runtime_protocol_version
        ),
        format!(
            "runtime_schema_sha256={}",
            hex(&manifest.runtime_schema_sha256)
        ),
        format!(
            "content_schema_sha256={}",
            hex(&manifest.content_schema_sha256)
        ),
        format!("task_schema_sha256={}", hex(&manifest.task_schema_sha256)),
        format!(
            "dependency_schema_sha256={}",
            hex(&manifest.dependency_schema_sha256)
        ),
        format!(
            "requested_capabilities={:?} (declarations only; no grants)",
            package.capabilities()
        ),
        format!("declared_budget={:?}", manifest.budget),
        format!(
            "effective_budget fuel={} memory_bytes={} host_calls={}",
            effective.fuel, effective.memory_bytes, effective.host_calls
        ),
        format!("required_features={:?}", manifest.required_features),
    ];
    for handler in &manifest.transform_handlers {
        lines.push(format!(
            "handler={} input_type={} output_type={} max_input_bytes={} max_output_bytes={}",
            handler.handler,
            handler.input_type,
            handler.output_type,
            handler.max_input_bytes,
            handler.max_output_bytes
        ));
    }
    for dependency in &manifest.dependencies {
        lines.push(format!(
            "dependency slot={} handler={} optional={} provider_version={}",
            dependency.slot, dependency.handler, dependency.optional, dependency.provider_version
        ));
    }
    if let Some(declaration) = &manifest.io_declaration {
        lines.push(format!(
            "io_version={} io_schema_sha256={} io_capabilities={:?} io_handlers={:?} (declarations only; no grants)",
            declaration.io_version,
            hex(&declaration.io_schema_sha256),
            package.io_capabilities(),
            declaration.handlers
        ));
        if let Some(budget) = &declaration.budget {
            lines.push(format!(
                "io_budget resources={} jobs={} bytes={} job_bytes={} duration_ms={}",
                budget.max_resources,
                budget.max_jobs,
                budget.max_bytes,
                budget.max_job_bytes,
                budget.max_duration_ms
            ));
        }
    }
    Ok(lines.join("\n"))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn read_input(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path).map_err(|error| CliError::new(2, format!("input: {error}")))?;
    if !file
        .metadata()
        .map_err(|error| CliError::new(2, format!("input: {error}")))?
        .is_file()
    {
        return Err(CliError::new(2, "input must be a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_VALUE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| CliError::new(2, format!("input: {error}")))?;
    if bytes.len() > MAX_VALUE_BYTES {
        return Err(CliError::new(2, "input exceeds 65536 bytes"));
    }
    Ok(bytes)
}
fn output_parent(path: &Path) -> Result<&Path> {
    if path.file_name().is_none() {
        return Err(CliError::new(5, "output must name a new file"));
    }
    match fs::symlink_metadata(path) {
        Ok(_) => {
            return Err(CliError::new(
                5,
                "output already exists; refusing to overwrite",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(CliError::new(5, format!("output: {error}"))),
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(CliError::new(5, "output directory does not exist"));
    }
    Ok(parent)
}
fn save_output(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = output_parent(path)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| CliError::new(5, format!("output temporary file: {error}")))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| CliError::new(5, format!("output write: {error}")))?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| CliError::new(5, format!("output publish refused: {}", error.error)))?;
    Ok(())
}
fn failure_code(code: FailureCode) -> &'static str {
    match code {
        FailureCode::InvalidInput => "invalidInput",
        FailureCode::UnsupportedInput => "unsupportedInput",
        FailureCode::ResourceLimit => "resourceLimit",
        FailureCode::Failed => "failed",
    }
}
fn transform(args: &[OsString]) -> Result<String> {
    let prepared = prepare(Path::new(&args[1]))?;
    if prepared
        .package()
        .manifest()
        .dependencies
        .iter()
        .any(|dependency| !dependency.optional)
    {
        return Err(CliError::new(
            2,
            "required dependencies need an explicitly approved dependency host; simple transform runner cannot resolve them",
        ));
    }
    let input = Invocation::new_transform(
        "plugin-check-transform",
        Transform {
            handler: text(&args[2])?.into(),
            input_type: text(&args[3])?.into(),
            output_type: text(&args[4])?.into(),
            input: read_input(Path::new(&args[5]))?,
        },
    )
    .map_err(|error| CliError::new(2, format!("invalid transform arguments: {error}")))?;
    let output_path = Path::new(&args[6]);
    output_parent(output_path)?;
    let directory = tempfile::tempdir()
        .map_err(|error| CliError::new(4, format!("temporary store: {error}")))?;
    let store = Store::open(&directory.path().join("synthetic.db"), Default::default())
        .map_err(|error| CliError::new(4, format!("temporary store: {error}")))?;
    let mut host =
        HostRuntime::new(store).map_err(|error| CliError::new(4, format!("host: {error}")))?;
    // Empty approved ceiling and no runtime grants. Nothing is installed or persisted to a registry.
    let connection = prepared
        .connect_approved(&mut host, &Default::default())
        .map_err(|error| CliError::new(4, format!("connection: {error}")))?;
    let report = prepared.run_task(
        &mut host,
        &connection,
        &input,
        || 1,
        Cancellation::default(),
    );
    host.disconnect(&connection)
        .map_err(|error| CliError::new(4, format!("disconnect: {error}")))?;
    if report.execution.outcome != Ok(0)
        || report.execution.host_calls != 0
        || report.response.is_some()
    {
        return Err(CliError::new(
            4,
            format!(
                "execution rejected: {:?}; host_calls={}",
                report.execution.outcome, report.execution.host_calls
            ),
        ));
    }
    if let Some(failure) = report.failure {
        return Err(CliError::new(
            3,
            format!(
                "BUSINESS_FAILURE code={} message={:?}",
                failure_code(failure.code),
                failure.message
            ),
        ));
    }
    // run_task has already checked the completion against the exact invocation and handler bounds.
    let output = report
        .output
        .ok_or_else(|| CliError::new(4, "execution did not produce a correlated output"))?;
    if output.type_id != text(&args[4])? {
        return Err(CliError::new(4, "output type mismatch"));
    }
    save_output(output_path, &output.bytes)?;
    Ok(format!(
        "OUTPUT bytes={} sha256={:x}\nExecuted once with no content grants; no package installation or registry approval.",
        output.bytes.len(),
        Sha256::digest(&output.bytes)
    ))
}
fn run(args: &[OsString]) -> Result<String> {
    match args.first().and_then(|value| value.to_str()) {
        Some("check") if args.len() == 2 => check(Path::new(&args[1])),
        Some("transform") if args.len() == 7 => transform(args),
        _ => Err(CliError::new(2, USAGE)),
    }
}
fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).take(9).collect();
    match run(&args) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", error.message);
            ExitCode::from(error.code)
        }
    }
}

#[cfg(test)]
#[path = "plugin_check/tests.rs"]
mod tests;
