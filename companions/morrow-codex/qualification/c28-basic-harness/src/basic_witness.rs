//! Fixed self-terminating child; no owner, input, PTY or process controls.
#![forbid(unsafe_code)]
use std::{fs::OpenOptions, io::{self, Read, Write}, path::{Path, PathBuf}};

pub fn paths(root: &Path, nonce: &str) -> io::Result<(PathBuf, PathBuf)> {
    if !crate::witness::valid_nonce(nonce) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "basic nonce"));
    }
    Ok((root.join("workspace").join(format!("basic-write-{nonce}.bin")),
        root.join("basic-outside").join(format!("denied-{nonce}.bin"))))
}

/// Native child-visible positive evidence is in the already granted workspace.
/// C retains this newly created file without write/delete sharing until review.
pub fn readable_positive_path(root: &Path, nonce: &str) -> io::Result<PathBuf> {
    paths(root, nonce)?;
    Ok(root.join("workspace").join(format!("trusted-positive-{nonce}.bin")))
}

fn native_root(root: &Path) -> io::Result<()> {
    let text = root.to_str().unwrap_or("");
    let tag = text.strip_prefix(r"C:\Morrow-C28-A1-047-")
        .and_then(|text| text.strip_suffix(r"\case"));
    if !tag.is_some_and(crate::witness::valid_nonce) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "native basic namespace"));
    }
    Ok(())
}

fn plain_leaf(path: &Path, directory: bool) -> io::Result<std::fs::Metadata> {
    let metadata = std::fs::symlink_metadata(path)?;
    let mut reparse = metadata.file_type().is_symlink();
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        reparse |= metadata.file_attributes() & 0x400 != 0;
    }
    if reparse || (if directory { !metadata.is_dir() } else { !metadata.is_file() }) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "basic plain leaf required"));
    }
    Ok(metadata)
}

pub(crate) fn verify_readable_positive(root: &Path, nonce: &str) -> io::Result<()> {
    plain_leaf(&root.join("workspace"), true)?;
    let path = readable_positive_path(root, nonce)?;
    if plain_leaf(&path, false)?.len() != 21 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "readable positive size"));
    }
    let mut file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file).take(22).read_to_end(&mut bytes)?;
    if bytes != b"C28-TRUSTED-POSITIVE\n" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "readable positive mismatch"));
    }
    Ok(())
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

pub(crate) fn security_from_args(args:&[String])->io::Result<Option<crate::basic_security::SecuritySpec>> {
    use crate::basic_security::IdentitySource;
    let native=match args.first().map(String::as_str) {
        Some("--witness-basic")=>false,Some("--witness-basic-v8-primary")=>true,_=>return Ok(None),
    };
    if args.len() != (if native {6}else{8}) || !crate::witness::valid_nonce(&args[1]) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "basic arguments"));
    }
    let (source,sid_index,port_index)=if native {(IdentitySource::CurrentPrimaryTokenV8,4,5)}else{
        (IdentitySource::Whoami(crate::preflight::Artifact {path:PathBuf::from(&args[4]),
            sha256:crate::preflight::digest(&args[5]).map_err(io::Error::other)?}),6,7)
    };
    let security = crate::basic_security::SecuritySpec {
        source,expected_sid:args[sid_index].clone(),
        port:args[port_index].parse::<u16>().map_err(io::Error::other)?,
    };
    if !crate::basic_security::valid_sid(&security.expected_sid) || security.port == 0
        || security.port.to_string() != args[port_index] {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "basic security identity/port"));
    }
    Ok(Some(security))
}
pub fn run_child_if_requested(args: &[String]) -> io::Result<Option<i32>> {
    let Some(security)=security_from_args(args)? else{return Ok(None)};
    let _query_pin=security.source.legacy_query().map(crate::basic_security::pin_query)
        .transpose().map_err(io::Error::other)?;
    let allowed = PathBuf::from(&args[2]);
    let denied = PathBuf::from(&args[3]);
    let root = allowed.parent().and_then(Path::parent)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "basic root"))?;
    let (expected_allowed, expected_denied) = paths(root, &args[1])?;
    if !root.is_absolute() || allowed != expected_allowed || denied != expected_denied {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "basic fixed path mismatch"));
    }
    crate::preflight::plain_absolute(root).map_err(io::Error::other)?;
    let native = security.source.is_native_v8();
    if native {
        native_root(root)?;
        // C already verified and retains the private ancestor/owner handles.
        // The restricted child can inspect only its granted workspace leaves.
        // It must never read the private owner marker or outside baseline.
        verify_readable_positive(root, &args[1])?;
    } else {
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
    }
    // Outside absence is checked by C before launch and after terminal review.
    // A denied metadata read is not an absence proof for the restricted child.
    if allowed.try_exists()? || (!native && denied.try_exists()?) {
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
    if !native && denied.try_exists()? {
        return Err(io::Error::new(io::ErrorKind::Other, "denied target appeared"));
    }
    let identity=crate::basic_security::observe_actual_identity(&security, allowed.parent().unwrap(), &args[1])
        .map_err(io::Error::other)?;
    crate::basic_security::observe_child_denial(security.port).map_err(io::Error::other)?;
    let mut stdout = io::stdout().lock(); let mut stderr = io::stderr().lock();
    stdout.write_all(&expected_stdout(&args[1]))?;
    stdout.write_all(&identity.stdout(&args[1]))?;
    stdout.write_all(&crate::basic_security::network_stdout(&args[1],security.port))?;
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
    fn native_namespace_is_exact_and_positive_is_nonce_bound() {
        let root = Path::new(r"C:\Morrow-C28-A1-047-00112233445566778899aabbccddeeff\case");
        native_root(root).unwrap();
        for wrong in [r"D:\Morrow-C28-A1-047-00112233445566778899aabbccddeeff\case",
            r"C:\Morrow-C28-A1-047-00112233445566778899aabbccddeeff\case\extra",
            r"C:\Morrow-C28-A1-047-00112233445566778899AABBCCDDEEFF\case",
            r"C:\arbitrary\case"] {
            assert!(native_root(Path::new(wrong)).is_err());
        }
        assert_ne!(readable_positive_path(root, NONCE).unwrap(),
            readable_positive_path(root, "11112233445566778899aabbccddeeff").unwrap());
        assert!(readable_positive_path(root, "../private").is_err());
    }
    #[cfg(windows)]
    #[test]
    fn readable_positive_is_immutable_while_original_parent_handle_is_held() {
        use std::os::windows::fs::OpenOptionsExt;
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("c28-positive-{}-{unique}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("workspace")).unwrap();
        let path = readable_positive_path(&root, NONCE).unwrap();
        assert!(verify_readable_positive(&root, NONCE).is_err());
        let mut original = OpenOptions::new().read(true).write(true).create_new(true).share_mode(1).open(&path).unwrap();
        original.write_all(b"C28-TRUSTED-POSITIVE\n").unwrap(); original.sync_all().unwrap();
        verify_readable_positive(&root, NONCE).unwrap();
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(std::fs::remove_file(&path).is_err());
        assert!(verify_readable_positive(&root, "11112233445566778899aabbccddeeff").is_err());
        drop(original);
        std::fs::write(&path, b"Z28-TRUSTED-POSITIVE\n").unwrap();
        assert!(verify_readable_positive(&root, NONCE).is_err());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root.join("workspace")).unwrap(); std::fs::remove_dir(root).unwrap();
    }
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
            source: crate::basic_security::IdentitySource::Whoami(crate::preflight::Artifact { path: PathBuf::from(crate::basic_security::SYSTEM_QUERY),
                sha256: crate::preflight::digest(crate::basic_security::SYSTEM_QUERY_SHA256).unwrap() }),
            expected_sid: "S-1-5-21-1-2-3-1001".into(), port: 23456,
        };
        let bytes = expected_security_stdout(NONCE, &spec);
        assert!(bytes.starts_with(&expected_stdout(NONCE)));
        assert!(bytes.ends_with(format!("C28-NET {NONCE} 127.0.0.1 23456 denied=10013\n").as_bytes()));
        let mut other = spec.clone(); other.port += 1;
        assert_ne!(bytes, expected_security_stdout(NONCE, &other));
        other = spec.clone(); other.expected_sid = "S-1-5-21-1-2-3-1002".into();
        assert_ne!(bytes, expected_security_stdout(NONCE, &other));
        other = spec;
        if let crate::basic_security::IdentitySource::Whoami(query)=&mut other.source {query.sha256[0] ^= 1;}else{panic!("original whoami fixture");}
        assert_ne!(bytes, expected_security_stdout(NONCE, &other));
    }
    #[test]
    fn malformed_basic_arguments_reject_before_file_effects() {
        assert!(run_child_if_requested(&["--witness-basic".into(), NONCE.into()]).is_err());
        assert!(run_child_if_requested(&["--witness-basic".into(), "wrong".into(),
            "relative/workspace/basic.bin".into(), "relative/outside.bin".into()]).is_err());
        assert_eq!(run_child_if_requested(&["--phase".into(), "preflight".into()]).unwrap(), None);
    }
    #[test]
    fn primary_mode_rejects_mixed_shape_bad_sid_and_noncanonical_port_before_effects() {
        let original=vec!["--witness-basic-v8-primary".into(),NONCE.into(),
            r"C:\memory\workspace\allow.bin".into(),r"C:\memory\basic-outside\deny.bin".into(),
            "S-1-5-21-1-2-3-1001".into(),"23456".into()];
        let parsed=security_from_args(&original).unwrap().unwrap();
        assert!(parsed.source.is_native_v8());assert!(parsed.source.legacy_query().is_none());
        let mut mixed=original.clone();mixed.push("unused-whoami-path".into());mixed.push("unused-image-hash".into());
        assert!(security_from_args(&mixed).is_err());
        mixed=original.clone();mixed[0]="--witness-basic".into();assert!(security_from_args(&mixed).is_err());
        mixed=original.clone();mixed[4]="S-1-5-18".into();assert!(security_from_args(&mixed).is_err());
        mixed=original;mixed[5]="023456".into();assert!(security_from_args(&mixed).is_err());
        assert!(run_child_if_requested(&["--witness-basic-v8-primary".into(),NONCE.into()]).is_err());
    }
}
