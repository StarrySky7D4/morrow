//! Read-only artifact observations. These are not execution authorization.
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub const MAX_PE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Artifact {
    pub path: PathBuf,
    pub sha256: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct Config {
    pub synthetic_root: PathBuf,
    pub runner: Artifact,
    pub setup: Artifact,
    pub helper: Artifact,
}

#[derive(Serialize)]
pub struct ArtifactObservation {
    pub canonical_path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub machine: &'static str,
}

#[derive(Serialize)]
pub struct Observation {
    pub status: &'static str,
    pub synthetic_root: PathBuf,
    pub artifacts: BTreeMap<&'static str, ArtifactObservation>,
    pub system_actions: u32,
    pub production_qualified: bool,
    pub protected_session_qualified: bool,
    pub sdk_complete: bool,
}

pub fn digest(value: &str) -> Result<[u8; 32]> {
    ensure!(
        value.len() == 64,
        "SHA-256 must have exactly 64 lowercase hex digits"
    );
    let mut out = [0; 32];
    for (i, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let nibble = |b| match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            _ => Err(anyhow::anyhow!("SHA-256 must be lowercase hex")),
        };
        out[i] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    ensure!(out != [0; 32], "zero artifact digest is not an identity");
    Ok(out)
}

pub fn hex(value: &[u8]) -> String {
    value.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn plain_absolute(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "paths must be explicitly absolute");
    let value = path.to_str().context("path is not Unicode")?;
    ensure!(
        value.len() <= 4096 && !value.chars().any(char::is_control),
        "invalid path text"
    );
    ensure!(
        !value.starts_with("\\\\") && !value.starts_with("//"),
        "UNC/device paths are not accepted"
    );
    ensure!(
        !value
            .split(['/', '\\'])
            .any(|component| matches!(component, "." | "..")),
        "relative path text components are not accepted"
    );
    ensure!(
        !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
        "relative path components are not accepted"
    );
    #[cfg(windows)]
    ensure!(
        value.as_bytes().get(1) == Some(&b':') && !value[2..].contains(':'),
        "device or alternate-stream path is not accepted"
    );
    Ok(())
}

pub fn no_reparse_ancestors(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor)
            .context("artifact ancestor metadata unavailable")?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "symlink paths are not accepted"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "reparse paths are not accepted"
            );
        }
    }
    Ok(())
}

pub fn observe_artifact(pin: &Artifact) -> Result<ArtifactObservation> {
    plain_absolute(&pin.path)?;
    no_reparse_ancestors(&pin.path)?;
    let mut file = File::open(&pin.path).context("pinned artifact cannot be opened")?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() >= 256 && before.len() <= MAX_PE_BYTES,
        "artifact size/type is outside the PE bound"
    );
    let mut header = vec![0; 4096.min(before.len() as usize)];
    file.read_exact(&mut header)?;
    ensure!(&header[..2] == b"MZ", "artifact is not a PE image");
    let offset = u32::from_le_bytes(header[60..64].try_into()?) as usize;
    ensure!(
        offset >= 64
            && offset
                .checked_add(26)
                .is_some_and(|end| end <= header.len()),
        "unsupported PE header offset"
    );
    ensure!(
        &header[offset..offset + 4] == b"PE\0\0",
        "invalid PE signature"
    );
    ensure!(
        u16::from_le_bytes(header[offset + 4..offset + 6].try_into()?) == 0x8664,
        "artifact is not AMD64"
    );
    ensure!(
        u16::from_le_bytes(header[offset + 24..offset + 26].try_into()?) == 0x20b,
        "artifact is not PE32+"
    );
    ensure!(
        u16::from_le_bytes(header[offset + 22..offset + 24].try_into()?) & 0x2000 == 0,
        "artifact must not be a DLL"
    );
    let mut hasher = Sha256::new();
    hasher.update(&header);
    let mut count = header.len() as u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        count = count
            .checked_add(read as u64)
            .context("artifact byte counter overflow")?;
        ensure!(count <= MAX_PE_BYTES, "artifact grew outside the bound");
        hasher.update(&buffer[..read]);
    }
    let after = file.metadata()?;
    let actual: [u8; 32] = hasher.finalize().into();
    ensure!(
        count == before.len()
            && before.len() == after.len()
            && before.modified()? == after.modified()?,
        "artifact changed while being observed"
    );
    ensure!(actual == pin.sha256, "artifact full SHA-256 mismatch");
    Ok(ArtifactObservation {
        canonical_path: pin.path.canonicalize()?,
        sha256: hex(&actual),
        bytes: count,
        machine: "AMD64_PE32_PLUS",
    })
}

pub fn observe_images(config: &Config) -> Result<BTreeMap<&'static str, ArtifactObservation>> {
    let runner = observe_artifact(&config.runner)?;
    let setup = observe_artifact(&config.setup)?;
    let helper = observe_artifact(&config.helper)?;
    ensure!(
        runner.canonical_path != setup.canonical_path
            && runner.canonical_path != helper.canonical_path
            && setup.canonical_path != helper.canonical_path,
        "independent helper identities must not alias"
    );
    ensure!(
        runner.canonical_path.parent() == setup.canonical_path.parent()
            && runner.canonical_path.parent() == helper.canonical_path.parent(),
        "pinned helper images must be materialized in the same approved artifact directory"
    );
    Ok(BTreeMap::from([
        ("runner", runner),
        ("setup", setup),
        ("helper", helper),
    ]))
}

fn check_materialized_paths(
    stage: &BTreeMap<&'static str, ArtifactObservation>,
    canonical_root: &Path,
    actual: &ArtifactObservation,
) -> Result<()> {
    let bin = canonical_root.join("codex-home").join(".sandbox-bin");
    ensure!(
        actual.canonical_path.parent() == Some(bin.as_path())
            && actual.canonical_path != stage["runner"].canonical_path
            && actual.canonical_path != stage["setup"].canonical_path
            && actual.canonical_path != stage["helper"].canonical_path,
        "materialized runner must be a distinct image directly in the owned .sandbox-bin"
    );
    ensure!(
        actual.sha256 == stage["runner"].sha256,
        "materialized runner differs from the approved source full SHA-256"
    );
    Ok(())
}

/// Separate production observation after the original lifecycle has materialized
/// and locked its exact version-resolved image. The original staged three-image
/// sibling policy is still mandatory. This observation grants no authority.
pub fn observe_materialized_images(
    config: &Config,
    materialized: &Artifact,
) -> Result<BTreeMap<&'static str, ArtifactObservation>> {
    let mut stage = observe_images(config)?;
    plain_absolute(&config.synthetic_root)?;
    no_reparse_ancestors(&config.synthetic_root)?;
    ensure!(config.synthetic_root.is_dir(), "owned synthetic root is absent");
    let canonical_root = config.synthetic_root.canonicalize()?;
    let actual = observe_artifact(materialized)?;
    check_materialized_paths(&stage, &canonical_root, &actual)?;
    stage.insert("materialized_runner", actual);
    Ok(stage)
}

pub fn preflight(config: &Config) -> Result<Observation> {
    plain_absolute(&config.synthetic_root)?;
    match std::fs::symlink_metadata(&config.synthetic_root) {
        Ok(_) => {
            bail!("synthetic root already exists; automatic adoption or deletion is forbidden")
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error).context("synthetic root status unavailable"),
    }
    let parent = config
        .synthetic_root
        .parent()
        .context("synthetic root has no parent")?;
    no_reparse_ancestors(parent)?;
    ensure!(parent.is_dir(), "synthetic root parent must already exist");
    Ok(Observation {
        status: "PASS_READONLY_ARTIFACT_PREFLIGHT_NOT_AUTHORIZATION",
        synthetic_root: config.synthetic_root.clone(),
        artifacts: observe_images(config)?,
        system_actions: 0,
        production_qualified: false,
        protected_session_qualified: false,
        sdk_complete: false,
    })
}

#[cfg(test)]
#[path = "preflight_tests.rs"]
mod tests;
