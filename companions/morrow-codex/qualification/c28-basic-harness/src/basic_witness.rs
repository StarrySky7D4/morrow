//! Fixed self-terminating child; no owner, input, PTY or process controls.
#![forbid(unsafe_code)]
use std::{fs::OpenOptions, io::{self, Write}, path::{Path, PathBuf}};

pub fn paths(root: &Path, nonce: &str) -> io::Result<(PathBuf, PathBuf)> {
    if !crate::witness::valid_nonce(nonce) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "basic nonce"));
    }
    Ok((root.join("workspace").join(format!("basic-write-{nonce}.bin")),
        root.join("basic-outside").join(format!("denied-{nonce}.bin"))))
}

pub fn file_bytes(nonce: &str) -> Vec<u8> {
    format!("C28-BASIC-WORKSPACE {nonce}\n").into_bytes()
}

pub fn expected_stdout(nonce: &str) -> Vec<u8> {
    format!("C28-BASIC {nonce} workspace_write=ok outside_write=PermissionDenied\n").into_bytes()
}
pub fn expected_stderr(nonce: &str) -> Vec<u8> {
    format!("C28-BASIC-STDERR {nonce}\n").into_bytes()
}

pub fn run_child_if_requested(args: &[String]) -> io::Result<Option<i32>> {
    if args.first().map(String::as_str) != Some("--witness-basic") { return Ok(None); }
    if args.len() != 8 || !crate::witness::valid_nonce(&args[1]) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "basic arguments"));
    }
    let security = crate::basic_security::SecuritySpec {
        query: crate::preflight::Artifact { path: PathBuf::from(&args[4]),
            sha256: crate::preflight::digest(&args[5]).map_err(io::Error::other)? },
        expected_sid: args[6].clone(),
        port: args[7].parse::<u16>().map_err(io::Error::other)?,
    };
    if !crate::basic_security::valid_sid(&security.expected_sid) || security.port == 0
        || security.port.to_string() != args[7] {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "basic security identity/port"));
    }
    let _query_pin = crate::basic_security::pin_query(&security.query).map_err(io::Error::other)?;
    let allowed = PathBuf::from(&args[2]);
    let denied = PathBuf::from(&args[3]);
    let root = allowed.parent().and_then(Path::parent)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "basic root"))?;
    let (expected_allowed, expected_denied) = paths(root, &args[1])?;
    if !root.is_absolute() || allowed != expected_allowed || denied != expected_denied {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "basic fixed path mismatch"));
    }
    crate::preflight::plain_absolute(root).map_err(io::Error::other)?;
    for parent in [allowed.parent().unwrap(), denied.parent().unwrap()] {
        crate::preflight::no_reparse_ancestors(parent).map_err(io::Error::other)?;
    }
    let marker = std::fs::symlink_metadata(root.join("vm-harness-owner.lock"))?;
    if !marker.is_file() || marker.file_type().is_symlink() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "owned lifecycle marker absent"));
    }
    let baseline = denied.parent().unwrap().join("trusted-positive.bin");
    crate::preflight::no_reparse_ancestors(&baseline).map_err(io::Error::other)?;
    if std::fs::metadata(&baseline)?.len() != 21
        || std::fs::read(&baseline)? != b"C28-TRUSTED-POSITIVE\n" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "trusted positive sentinel mismatch"));
    }
    // Parent owns fresh directories, retains ancestor locks and reviewed these
    // exact argv paths. Do not read personal files or adopt an existing target.
    if allowed.try_exists()? || denied.try_exists()? {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "basic target collision"));
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(&allowed)?;
    file.write_all(&file_bytes(&args[1]))?;
    file.sync_all()?;
    drop(file);
    match OpenOptions::new().write(true).create_new(true).open(&denied) {
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {}
        Err(error) => return Err(error),
        Ok(file) => {
            drop(file); // Preserve unexpected side effect; never delete it to claim denial.
            return Err(io::Error::new(io::ErrorKind::Other, "outside write unexpectedly succeeded"));
        }
    }
    if denied.try_exists()? {
        return Err(io::Error::new(io::ErrorKind::Other, "denied target appeared"));
    }
    crate::basic_security::query_actual_sid(&security, allowed.parent().unwrap(), &args[1])
        .map_err(io::Error::other)?;
    crate::basic_security::observe_child_denial(security.port).map_err(io::Error::other)?;
    let mut stdout = io::stdout().lock(); let mut stderr = io::stderr().lock();
    stdout.write_all(&expected_security_stdout(&args[1], &security))?;
    stderr.write_all(&expected_stderr(&args[1]))?;
    stdout.flush()?; stderr.flush()?;
    Ok(Some(0))
}

pub fn expected_security_stdout(nonce: &str, security: &crate::basic_security::SecuritySpec) -> Vec<u8> {
    let mut raw = expected_stdout(nonce);
    raw.extend(crate::basic_security::security_stdout(nonce, security)); raw
}

fn emit(nonce: &str, stdout: &mut impl Write, stderr: &mut impl Write) -> io::Result<()> {
    stdout.write_all(&expected_stdout(nonce))?;
    stderr.write_all(&expected_stderr(nonce))?;
    stdout.flush()?;
    stderr.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    const NONCE: &str = "00112233445566778899aabbccddeeff";
    #[test]
    fn fixed_basic_output_is_noninteractive_and_nonce_bound() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(NONCE, &mut out, &mut err).unwrap();
        assert_eq!(out, expected_stdout(NONCE));
        assert_eq!(err, expected_stderr(NONCE));
        assert_ne!(out, expected_stdout("11112233445566778899aabbccddeeff"));
    }
    #[test]
    fn security_output_binds_original_sid_image_hash_and_listener_port() {
        let spec = crate::basic_security::SecuritySpec {
            query: crate::preflight::Artifact { path: PathBuf::from(crate::basic_security::SYSTEM_QUERY),
                sha256: crate::preflight::digest(crate::basic_security::SYSTEM_QUERY_SHA256).unwrap() },
            expected_sid: "S-1-5-21-1-2-3-1001".into(), port: 23456,
        };
        let bytes = expected_security_stdout(NONCE, &spec);
        assert!(bytes.starts_with(&expected_stdout(NONCE)));
        assert!(bytes.ends_with(format!("C28-NET {NONCE} 127.0.0.1 23456 denied=10013\n").as_bytes()));
        let mut other = spec.clone(); other.port += 1;
        assert_ne!(bytes, expected_security_stdout(NONCE, &other));
        other = spec.clone(); other.expected_sid = "S-1-5-21-1-2-3-1002".into();
        assert_ne!(bytes, expected_security_stdout(NONCE, &other));
        other = spec; other.query.sha256[0] ^= 1;
        assert_ne!(bytes, expected_security_stdout(NONCE, &other));
    }
    #[test]
    fn malformed_basic_arguments_reject_before_file_effects() {
        assert!(run_child_if_requested(&["--witness-basic".into(), NONCE.into()]).is_err());
        assert!(run_child_if_requested(&["--witness-basic".into(), "wrong".into(),
            "relative/workspace/basic.bin".into(), "relative/outside.bin".into()]).is_err());
        assert_eq!(run_child_if_requested(&["--phase".into(), "preflight".into()]).unwrap(), None);
    }
}
