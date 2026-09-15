//! Capture a fixed working-tree snapshot and a real local build, or verify its files.
//! This is an engineering witness, not a signature, sandbox or reproducibility proof.
#![forbid(unsafe_code)]
use morrow_audit::build_receipt::{
    self,
    proto::{BuildReceipt, CommandRecord, FileEntry},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const PROFILE: &str = "network-node-release";
const MAX_FILE: u64 = 256 * 1024 * 1024;
const MAX_TREE: u64 = 2 * 1024 * 1024 * 1024;

fn invalid(message: &str) -> Box<dyn Error> {
    io::Error::new(io::ErrorKind::InvalidData, message).into()
}
fn millis() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}
fn text_command(root: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()?;
    if !output.status.success() || output.stdout.len() > 4 * 1024 * 1024 {
        return Err(invalid(&format!("{program} observation failed")));
    }
    Ok(String::from_utf8(output.stdout)?
        .trim_end_matches(['\r', '\n'])
        .to_owned())
}
fn source_names(root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() || output.stdout.len() > 4 * 1024 * 1024 {
        return Err(invalid("git source enumeration failed"));
    }
    let mut paths = BTreeSet::new();
    for raw in output.stdout.split(|b| *b == 0).filter(|v| !v.is_empty()) {
        let name = std::str::from_utf8(raw)?.to_owned();
        // Deleted tracked files remain absent in the copied source tree.
        if root.join(&name).try_exists()? {
            paths.insert(name);
        }
    }
    if paths.is_empty() || paths.len() > build_receipt::MAX_SOURCES {
        return Err(invalid("source count outside receipt budget"));
    }
    Ok(paths.into_iter().collect())
}
fn checked_path(root: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.contains('\\')
        || name.contains(':')
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == ".." || part == ".git")
        || Path::new(name)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(invalid("nonportable receipt path"));
    }
    let path = root.join(name);
    let mut cursor = root.to_path_buf();
    for part in Path::new(name).components() {
        cursor.push(part);
        if fs::symlink_metadata(&cursor)?.file_type().is_symlink() {
            return Err(invalid("linked receipt input"));
        }
    }
    if !path.canonicalize()?.starts_with(root.canonicalize()?) {
        return Err(invalid("receipt input outside root"));
    }
    Ok(path)
}
fn file_entry(root: &Path, name: &str) -> Result<FileEntry> {
    let path = checked_path(root, name)?;
    let mut file = File::open(&path)?;
    let before = file.metadata()?;
    if !before.is_file() || before.len() > MAX_FILE {
        return Err(invalid("input file too large or not regular"));
    }
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| invalid("file size overflow"))?;
        if size > MAX_FILE {
            return Err(invalid("input grew beyond budget"));
        }
        hash.update(&buffer[..read]);
    }
    let after = file.metadata()?;
    let current = fs::metadata(&path)?;
    if before.len() != size
        || after.len() != size
        || current.len() != size
        || before.modified()? != after.modified()?
        || after.modified()? != current.modified()?
    {
        return Err(invalid("input changed during hashing"));
    }
    Ok(FileEntry {
        path: name.into(),
        size,
        sha256: hash.finalize().to_vec(),
    })
}
fn check_files(root: &Path, entries: &[FileEntry]) -> Result<()> {
    for expected in entries {
        if file_entry(root, &expected.path)? != *expected {
            return Err(invalid("observed file bytes differ from receipt"));
        }
    }
    Ok(())
}
fn walk_files(root: &Path, directory: &Path, output: &mut BTreeSet<String>) -> Result<()> {
    for item in fs::read_dir(directory)? {
        let path = item?.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            return Err(invalid("linked snapshot entry"));
        }
        if meta.is_dir() {
            walk_files(root, &path, output)?;
        } else if meta.is_file() {
            output.insert(
                path.strip_prefix(root)?
                    .to_str()
                    .ok_or_else(|| invalid("non UTF-8 snapshot path"))?
                    .replace('\\', "/"),
            );
            if output.len() > build_receipt::MAX_SOURCES {
                return Err(invalid("snapshot count exceeded"));
            }
        } else {
            return Err(invalid("nonregular snapshot entry"));
        }
    }
    Ok(())
}
fn verify_snapshot(root: &Path, entries: &[FileEntry]) -> Result<()> {
    let mut actual = BTreeSet::new();
    walk_files(root, root, &mut actual)?;
    let expected: BTreeSet<_> = entries.iter().map(|v| v.path.clone()).collect();
    if actual != expected {
        return Err(invalid("snapshot file set changed"));
    }
    check_files(root, entries)
}
fn new_file(path: &Path) -> Result<File> {
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}
fn copy_snapshot(project: &Path, destination: &Path, names: &[String]) -> Result<Vec<FileEntry>> {
    fs::create_dir(destination)?;
    let mut entries = Vec::new();
    let mut total = 0u64;
    for name in names {
        let original = file_entry(project, name)?;
        total = total
            .checked_add(original.size)
            .ok_or_else(|| invalid("source tree size overflow"))?;
        if total > MAX_TREE {
            return Err(invalid("source tree budget exceeded"));
        }
        let target = destination.join(name);
        fs::create_dir_all(target.parent().ok_or_else(|| invalid("snapshot parent"))?)?;
        let mut from = File::open(checked_path(project, name)?)?;
        let mut to = new_file(&target)?;
        // A length-bound copy cannot hang indefinitely on a growing input.
        let count = io::copy(&mut (&mut from).take(original.size + 1), &mut to)?;
        to.sync_all()?;
        if count != original.size || file_entry(destination, name)? != original {
            return Err(invalid("source changed while copying"));
        }
        fs::set_permissions(&target, fs::metadata(project.join(name))?.permissions())?;
        entries.push(original);
    }
    check_files(project, &entries)?;
    verify_snapshot(destination, &entries)?;
    Ok(entries)
}
fn expected_args(target: &str) -> Vec<String> {
    [
        "build",
        "--offline",
        "--locked",
        "--release",
        "--manifest-path",
        "network_node/Cargo.toml",
        "--features",
        "plugin-adapter",
        "--bin",
        "morrow-api-node",
        "--target",
        target,
        "--target-dir",
        "../target",
        "-j",
        "2",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
fn binary_name(target: &str) -> &'static str {
    if target.contains("windows") {
        "morrow-api-node.exe"
    } else {
        "morrow-api-node"
    }
}
fn receipt_path(run: &Path) -> PathBuf {
    run.join("build-receipt.pb.lz4")
}
fn verify(run: &Path) -> Result<BuildReceipt> {
    let path = checked_path(run, "build-receipt.pb.lz4")?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take((build_receipt::MAX_RAW_BYTES + build_receipt::MAX_RAW_BYTES / 255 + 129) as u64)
        .read_to_end(&mut bytes)?;
    let receipt = build_receipt::decode(&bytes)?;
    if receipt.profile != PROFILE
        || receipt.features != ["plugin-adapter"]
        || receipt.commands.len() != 1
        || receipt.commands[0].label != "build"
        || receipt.commands[0].program != "cargo"
        || receipt.commands[0].args != expected_args(&receipt.target)
    {
        return Err(invalid("unsupported receipt execution profile"));
    }
    verify_snapshot(&run.join("source"), &receipt.sources)?;
    for command in &receipt.commands {
        check_files(
            run,
            &[
                command
                    .stdout
                    .clone()
                    .ok_or_else(|| invalid("missing stdout"))?,
                command
                    .stderr
                    .clone()
                    .ok_or_else(|| invalid("missing stderr"))?,
            ],
        )?;
    }
    check_files(run, &receipt.artifacts)?;
    if receipt.build_succeeded
        && (receipt.artifacts.len() != 1
            || receipt.artifacts[0].path != format!("artifacts/{}", binary_name(&receipt.target)))
    {
        return Err(invalid("unexpected successful build artifact"));
    }
    Ok(receipt)
}
fn capture(project: &Path, output: &Path) -> Result<bool> {
    let project = project.canonicalize()?;
    let head = text_command(&project, "git", &["rev-parse", "HEAD"])?;
    let state = text_command(
        &project,
        "git",
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    let names = source_names(&project)?;
    // Only a fresh directory is permitted. Existing artifacts and receipts are never replaced.
    fs::create_dir(output)?;
    let run = output.canonicalize()?;
    if names
        .iter()
        .any(|name| project.join(name).starts_with(&run))
    {
        return Err(invalid("output overlaps captured source"));
    }
    let started = millis()?;
    let sources = copy_snapshot(&project, &run.join("source"), &names)?;
    if text_command(&project, "git", &["rev-parse", "HEAD"])? != head
        || text_command(
            &project,
            "git",
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )? != state
        || source_names(&project)? != names
    {
        return Err(invalid(
            "working tree changed while creating snapshot; partial output preserved",
        ));
    }
    let rustc = text_command(&run.join("source"), "rustc", &["-Vv"])?;
    let cargo = text_command(&run.join("source"), "cargo", &["-V"])?;
    let target = rustc
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| invalid("rustc host not found"))?
        .to_owned();
    if !target
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(invalid("invalid target"));
    }
    fs::create_dir(run.join("logs"))?;
    let args = expected_args(&target);
    println!("Building fixed source snapshot; output {}", run.display());
    let mut command = Command::new("cargo");
    command.args(&args).current_dir(run.join("source"));
    // Record an explicit target and avoid hidden caller build wrappers/flags.
    // User/global Cargo configuration and external tools still prevent a hermeticity claim.
    for name in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_BUILD_TARGET",
        "CARGO_TARGET_DIR",
    ] {
        command.env_remove(name);
    }
    command.stdout(Stdio::from(new_file(&run.join("logs/build.stdout"))?));
    command.stderr(Stdio::from(new_file(&run.join("logs/build.stderr"))?));
    let status = command.status()?;
    let success = status.success();
    let mut artifacts = Vec::new();
    if success {
        let name = binary_name(&target);
        let built_name = format!("target/{target}/release/{name}");
        let built = file_entry(&run, &built_name)?;
        fs::create_dir(run.join("artifacts"))?;
        let artifact_name = format!("artifacts/{name}");
        let mut source = File::open(checked_path(&run, &built_name)?)?;
        let mut target_file = new_file(&run.join(&artifact_name))?;
        let count = io::copy(&mut (&mut source).take(built.size + 1), &mut target_file)?;
        target_file.sync_all()?;
        let captured = file_entry(&run, &artifact_name)?;
        if count != built.size || captured.sha256 != built.sha256 {
            return Err(invalid("built artifact changed during capture"));
        }
        artifacts.push(captured);
    }
    verify_snapshot(&run.join("source"), &sources)?;
    let collector = fs::read(std::env::current_exe()?)?;
    let receipt = BuildReceipt {
        version: build_receipt::VERSION,
        source_head: head,
        source_dirty: !state.is_empty(),
        source_root_sha256: build_receipt::source_digest(&sources)?.to_vec(),
        sources,
        profile: PROFILE.into(),
        target,
        features: vec!["plugin-adapter".into()],
        rustc_version: rustc,
        cargo_version: cargo,
        collector_sha256: Sha256::digest(collector).to_vec(),
        commands: vec![CommandRecord {
            label: "build".into(),
            program: "cargo".into(),
            args,
            exit_code: status.code().unwrap_or(-1),
            completed: true,
            stdout: Some(file_entry(&run, "logs/build.stdout")?),
            stderr: Some(file_entry(&run, "logs/build.stderr")?),
        }],
        artifacts,
        build_succeeded: success,
        started_unix_ms: started,
        finished_unix_ms: millis()?,
    };
    let bytes = build_receipt::encode(&receipt)?;
    let mut file = new_file(&receipt_path(&run))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    let verified = verify(&run)?;
    println!(
        "{}: {} source files; receipt {}",
        if verified.build_succeeded {
            "BUILD_CAPTURED"
        } else {
            "BUILD_FAILED_RECORDED"
        },
        verified.sources.len(),
        receipt_path(&run).display()
    );
    Ok(success)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn markdown(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
        .replace('`', "&#96;")
        .replace('[', "&#91;")
        .replace(']', "&#93;")
        .replace('\r', "")
        .replace('\n', "<br>")
}
fn print_report(receipt: &BuildReceipt) {
    println!(
        "# Morrow local build receipt\n\nVerified file integrity only; unsigned and non-hermetic. No guest execution, application, device, channel or reproducible-build qualification.\n"
    );
    println!("| Observation | Value |\n| --- | --- |");
    for (label, value) in [
        ("Profile", receipt.profile.clone()),
        ("Source HEAD", receipt.source_head.clone()),
        ("Working tree was dirty", receipt.source_dirty.to_string()),
        ("Copied source files", receipt.sources.len().to_string()),
        ("Source-set SHA-256", hex(&receipt.source_root_sha256)),
        ("Explicit target", receipt.target.clone()),
        ("Features", receipt.features.join(", ")),
        ("Build succeeded", receipt.build_succeeded.to_string()),
        ("rustc", receipt.rustc_version.clone()),
        ("cargo", receipt.cargo_version.clone()),
        ("Collector SHA-256", hex(&receipt.collector_sha256)),
        ("Started Unix ms", receipt.started_unix_ms.to_string()),
        ("Finished Unix ms", receipt.finished_unix_ms.to_string()),
    ] {
        println!("| {label} | {} |", markdown(&value));
    }
    println!(
        "\n## Observed commands\n\nAll commands ran in the preserved source directory.\n\n| Label | Program | Arguments | Completed | Exit |\n| --- | --- | --- | --- | --- |"
    );
    for command in &receipt.commands {
        println!(
            "| {} | {} | {} | {} | {} |",
            markdown(&command.label),
            markdown(&command.program),
            markdown(&command.args.join(" ")),
            command.completed,
            command.exit_code
        );
    }
    println!(
        "\n## Bound artifacts and logs\n\nPaths are relative to this run directory.\n\n| Path | Bytes | SHA-256 |\n| --- | --- | --- |"
    );
    for entry in receipt.artifacts.iter().chain(
        receipt
            .commands
            .iter()
            .flat_map(|c| c.stdout.iter().chain(c.stderr.iter())),
    ) {
        println!(
            "| {} | {} | {} |",
            markdown(&entry.path),
            entry.size,
            hex(&entry.sha256)
        );
    }
    println!(
        "\nThe Protobuf + LZ4 original retains the full source-file manifest. Verification requires that exact source file set and every recorded log/artifact. Missing final receipt means incomplete capture, not successful build or proven failure.\n"
    );
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result: Result<bool> = match args.as_slice() {
        [action, project, output] if action == "capture" => capture(Path::new(project), Path::new(output)),
        [action, run] if action == "verify" => verify(Path::new(run)).map(|receipt| {
            println!("PASS_SCOPED: receipt and file integrity; recorded build_succeeded={}; target={}; sources={}; no signature, test/device/channel or reproducible-build claim", receipt.build_succeeded, receipt.target, receipt.sources.len());
            receipt.build_succeeded
        }),
        [action, run] if action == "report" => verify(Path::new(run)).map(|receipt| {
            print_report(&receipt);
            receipt.build_succeeded
        }),
        _ => Err(invalid("usage: morrow-build-receipt capture PROJECT NEW_OUTPUT_DIRECTORY | verify RUN_DIRECTORY | report RUN_DIRECTORY")),
    };
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!(
                "RECEIPT ERROR: {error}; partial output is retained, not evidence of build success"
            );
            std::process::exit(2);
        }
    }
}
