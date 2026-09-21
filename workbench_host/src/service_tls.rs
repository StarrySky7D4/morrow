//! Explicit, per-run TLS identity selection by the trusted application.
//! Files are bounded and revalidated at start. This is not a filesystem sandbox,
//! certificate trust validator, persisted key store, or automatic rotation.
use crate::Result;
use crate::tls_validity::TlsValidity;
use morrow_network_node::server::TlsIdentity;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::Read,
    path::{Component, Path, PathBuf},
};
use zeroize::Zeroizing;

const MAX_PEM_BYTES: u64 = 65_536;
const FILE_ERROR: &str = "invalid selected TLS file";
const IDENTITY_ERROR: &str = "invalid selected TLS identity";

/// Contains paths and the exact certificate PEM digest; never private key bytes.
/// Intentionally has no Debug implementation.
pub struct TlsSelection {
    certificate: PathBuf,
    private_key: PathBuf,
    certificate_sha256: [u8; 32],
    validity: Option<TlsValidity>,
}

impl TlsSelection {
    /// Restore a caller's frozen choice, not a live identity. `load` is still
    /// mandatory at admission and compares these bytes with the selected file.
    pub fn from_expected(
        certificate: PathBuf,
        private_key: PathBuf,
        certificate_sha256: [u8; 32],
    ) -> Result<Self> {
        if certificate_sha256 == [0; 32] {
            return Err(IDENTITY_ERROR.into());
        }
        Ok(Self {
            certificate,
            private_key,
            certificate_sha256,
            validity: None,
        })
    }
    pub fn inspect(certificate: &Path, private_key: &Path) -> Result<Self> {
        let cert = read_bounded(certificate)?;
        let key = read_bounded(private_key)?;
        TlsIdentity::from_pem(&cert, &key).map_err(|_| IDENTITY_ERROR)?;
        Ok(Self {
            certificate: certificate.to_path_buf(),
            private_key: private_key.to_path_buf(),
            certificate_sha256: Sha256::digest(&*cert).into(),
            validity: Some(TlsValidity::from_pem(&cert)?),
        })
    }

    pub fn certificate_path(&self) -> &Path {
        &self.certificate
    }

    pub fn private_key_path(&self) -> &Path {
        &self.private_key
    }

    pub fn certificate_sha256(&self) -> [u8; 32] {
        self.certificate_sha256
    }

    pub fn validity(&self) -> Option<TlsValidity> {
        self.validity
    }

    /// Recheck selected files immediately before constructing the active identity.
    /// An active server owns its in-memory identity and does not watch these paths.
    pub fn load(&self) -> Result<TlsIdentity> {
        self.load_with_validity().map(|(identity, _)| identity)
    }

    pub fn load_with_validity(&self) -> Result<(TlsIdentity, TlsValidity)> {
        let cert = read_bounded(&self.certificate)?;
        let key = read_bounded(&self.private_key)?;
        if <[u8; 32]>::from(Sha256::digest(&*cert)) != self.certificate_sha256 {
            return Err("selected TLS certificate changed".into());
        }
        let identity = TlsIdentity::from_pem(&cert, &key).map_err(|_| IDENTITY_ERROR)?;
        Ok((identity, TlsValidity::from_pem(&cert)?))
    }
}

fn reparse(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

fn checked_path(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(FILE_ERROR.into());
    }
    for component in path.components() {
        match component {
            Component::ParentDir | Component::CurDir => return Err(FILE_ERROR.into()),
            #[cfg(windows)]
            Component::Prefix(prefix)
                if !matches!(
                    prefix.kind(),
                    std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
                ) =>
            {
                return Err(FILE_ERROR.into());
            }
            #[cfg(windows)]
            Component::Normal(name) if name.to_string_lossy().contains(':') => {
                return Err(FILE_ERROR.into());
            }
            _ => {}
        }
    }
    // Check the selected spelling before resolving anything: canonicalizing
    // first would erase a symlink/reparse point that this policy rejects.
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| FILE_ERROR)?;
        if metadata.file_type().is_symlink() || reparse(&metadata) {
            return Err(FILE_ERROR.into());
        }
    }
    Ok(())
}

fn regular_bounded(metadata: &Metadata) -> bool {
    metadata.is_file() && !reparse(metadata) && (1..=MAX_PEM_BYTES).contains(&metadata.len())
}

fn read_bounded(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    checked_path(path)?;
    if !regular_bounded(&fs::symlink_metadata(path).map_err(|_| FILE_ERROR)?) {
        return Err(FILE_ERROR.into());
    }
    let mut options = File::options();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Inspect a replaced leaf reparse point instead of following it.
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path).map_err(|_| FILE_ERROR)?;
    if !regular_bounded(&file.metadata().map_err(|_| FILE_ERROR)?) {
        return Err(FILE_ERROR.into());
    }
    // Fixed capacity avoids growth leaving an unwiped allocation behind.
    let mut bytes = Zeroizing::new(Vec::with_capacity((MAX_PEM_BYTES + 1) as usize));
    file.take(MAX_PEM_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| FILE_ERROR)?;
    if bytes.is_empty() || bytes.len() > MAX_PEM_BYTES as usize {
        return Err(FILE_ERROR.into());
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{CertifiedKey, generate_simple_self_signed};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn pair() -> (String, String) {
        let CertifiedKey { cert, key_pair } =
            generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        (cert.pem(), key_pair.serialize_pem())
    }

    fn write(
        dir: &TempDir,
        cert_pem: &str,
        key_pem: &str,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let cert_path = dir.path().join("tls.crt");
        let key_path = dir.path().join("tls.key");
        fs::write(&cert_path, cert_pem).unwrap();
        fs::write(&key_path, key_pem).unwrap();
        (cert_path, key_path)
    }

    #[test]
    fn valid_pair_hash_matches_and_rotation() {
        let dir = TempDir::new().unwrap();
        let (cert_pem, key_pem) = pair();
        let (cert_path, key_path) = write(&dir, &cert_pem, &key_pem);
        let sel: TlsSelection = TlsSelection::inspect(&cert_path, &key_path).unwrap();
        let expected: [u8; 32] = Sha256::digest(cert_pem.as_bytes()).into();
        assert_eq!(sel.certificate_sha256(), expected);
        assert!(sel.load().is_ok());

        // Rotate: overwrite same two files with a different valid pair.
        let (cert_pem2, key_pem2) = pair();
        let (cert_path, key_path) = write(&dir, &cert_pem2, &key_pem2);
        // Old selection now fails to load.
        assert!(sel.load().is_err());
        // Fresh inspect succeeds.
        let sel2: TlsSelection = TlsSelection::inspect(&cert_path, &key_path).unwrap();
        let expected2: [u8; 32] = Sha256::digest(cert_pem2.as_bytes()).into();
        assert_eq!(sel2.certificate_sha256(), expected2);
        assert!(sel2.load().is_ok());
    }

    #[test]
    fn mismatched_key_and_cert_rejected_at_inspect() {
        let dir = TempDir::new().unwrap();
        let (cert_pem, _) = pair();
        let (_, key_pem) = pair();
        let (cert_path, key_path) = write(&dir, &cert_pem, &key_pem);
        assert!(TlsSelection::inspect(&cert_path, &key_path).is_err());
    }

    #[test]
    fn read_bounded_rejects_bad_inputs() {
        let dir = TempDir::new().unwrap();

        let empty = dir.path().join("empty.pem");
        fs::write(&empty, b"").unwrap();
        assert!(read_bounded(&empty).is_err());

        let big = dir.path().join("big.pem");
        fs::write(&big, vec![b'a'; 65537]).unwrap();
        assert!(read_bounded(&big).is_err());

        assert!(read_bounded(dir.path()).is_err());

        let relative: &Path = Path::new("relative/nope.pem");
        assert!(read_bounded(relative).is_err());
    }

    #[test]
    fn key_replacement_and_removal_are_rechecked_without_path_or_pem_errors() {
        let dir = TempDir::new().unwrap();
        let (certificate, key) = pair();
        let (cert_path, key_path) = write(&dir, &certificate, &key);
        let selection = TlsSelection::inspect(&cert_path, &key_path).unwrap();
        let (_, other_key) = pair();
        fs::write(&key_path, &other_key).unwrap();
        assert_eq!(selection.load().err().unwrap().to_string(), IDENTITY_ERROR);
        fs::write(&key_path, "sensitive-invalid-private-key").unwrap();
        assert_eq!(selection.load().err().unwrap().to_string(), IDENTITY_ERROR);
        fs::remove_file(&key_path).unwrap();
        assert_eq!(selection.load().err().unwrap().to_string(), FILE_ERROR);
        fs::write(&key_path, key).unwrap();
        assert!(selection.load().is_ok());
        assert_eq!(selection.certificate_path(), cert_path);
        assert_eq!(selection.private_key_path(), key_path);
    }

    #[test]
    fn exact_read_limit_is_accepted_but_invalid_pem_is_not_an_identity() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("limit.pem");
        fs::write(&path, vec![b'x'; MAX_PEM_BYTES as usize]).unwrap();
        assert_eq!(read_bounded(&path).unwrap().len(), MAX_PEM_BYTES as usize);
        assert_eq!(
            TlsSelection::inspect(&path, &path)
                .err()
                .unwrap()
                .to_string(),
            IDENTITY_ERROR
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_device_unc_and_alternate_stream_spellings_are_rejected() {
        for path in [
            r"\\server\share\cert.pem",
            r"\\?\UNC\server\share\cert.pem",
            r"\\.\PhysicalDrive0",
            r"C:\cert.pem:secret",
            r"C:\a\..\cert.pem",
        ] {
            assert!(checked_path(Path::new(path)).is_err());
        }
    }

    #[cfg(any(windows, unix))]
    #[test]
    fn leaf_and_parent_symlinks_are_rejected_before_canonicalization() {
        let dir = TempDir::new().unwrap();
        let actual = dir.path().join("real");
        fs::create_dir(&actual).unwrap();
        let file = actual.join("cert.pem");
        fs::write(&file, "synthetic").unwrap();
        let leaf = dir.path().join("linked.pem");
        let parent = dir.path().join("linked-dir");
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(&file, &leaf).unwrap();
            std::os::windows::fs::symlink_dir(&actual, &parent).unwrap();
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&file, &leaf).unwrap();
            std::os::unix::fs::symlink(&actual, &parent).unwrap();
        }
        assert!(read_bounded(&leaf).is_err());
        assert!(read_bounded(&parent.join("cert.pem")).is_err());
        assert_eq!(&*read_bounded(&file).unwrap(), b"synthetic");
    }
}
