//! Current-user platform protection. No load-or-create path and no plaintext key export.
use crate::{SigningKey, TrustedLog};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};
use zeroize::{Zeroize, Zeroizing};
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod windows;
#[cfg(target_os = "windows")]
use windows as platform;
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod windows_publication;
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(all(target_os = "linux", feature = "linux-test-keyring"))]
pub use linux::test_keyring;
mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.keys.v1.rs"));
}
const MAGIC: &[u8; 8] = b"MORROWK1";
#[cfg(target_os = "windows")]
const PROVIDER: &str = "windows-current-user-dpapi-v1";
#[cfg(target_os = "linux")]
const PROVIDER: &str = "linux-current-user-secret-service-v1";
const MAX_FILE: usize = 64 * 1024;
impl Drop for proto::SecretKey {
    fn drop(&mut self) {
        self.seed.zeroize();
    }
}
#[derive(Debug)]
pub enum KeyError {
    Io(std::io::Error),
    Format,
    Protection,
    Random,
    AlreadyExists,
    PublishUnknown,
}
impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "protected audit key: {self:?}")
    }
}
impl std::error::Error for KeyError {}
impl From<std::io::Error> for KeyError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
pub type Result<T> = std::result::Result<T, KeyError>;

// Fixed failure labels only. No paths, error formatting, key material or provider output.
// Observing a failure must return the original Result even if stderr cannot be written.
#[derive(Clone, Copy)]
enum KeyDiagnosticStage {
    Initialize,
    ExistingLoad,
    DestinationCheck,
    Random,
    Protect,
    TemporaryIo,
    Publish,
    PublishedSync,
    ExistingRead,
    ReloadRead,
    ContainerFormat,
    Unprotect,
    MaterialValidate,
}
impl KeyDiagnosticStage {
    fn code(self) -> &'static [u8] {
        match self {
            Self::Initialize => b"initialize",
            Self::ExistingLoad => b"existing_load",
            Self::DestinationCheck => b"destination_check",
            Self::Random => b"random",
            Self::Protect => b"protect",
            Self::TemporaryIo => b"temporary_io",
            Self::Publish => b"publish",
            Self::PublishedSync => b"published_sync",
            Self::ExistingRead => b"existing_read",
            Self::ReloadRead => b"reload_read",
            Self::ContainerFormat => b"container_format",
            Self::Unprotect => b"unprotect",
            Self::MaterialValidate => b"material_validate",
        }
    }
}
fn diagnostic_kind(error: &KeyError) -> &'static [u8] {
    match error {
        KeyError::Io(_) => b"io",
        KeyError::Format => b"format",
        KeyError::Protection => b"protection",
        KeyError::Random => b"random",
        KeyError::AlreadyExists => b"already_exists",
        KeyError::PublishUnknown => b"publish_unknown",
    }
}
fn observe_result_with<T, W: Write>(
    stage: KeyDiagnosticStage,
    result: Result<T>,
    writer: &mut W,
) -> Result<T> {
    if let Err(error) = &result {
        let _ = (|| -> std::io::Result<()> {
            writer.write_all(b"MORROW_AUDIT_KEY_DIAGNOSTIC stage=")?;
            writer.write_all(stage.code())?;
            writer.write_all(b" kind=")?;
            writer.write_all(diagnostic_kind(error))?;
            writer.write_all(b"\n")
        })();
    }
    result
}
fn observe_result<T>(stage: KeyDiagnosticStage, result: Result<T>) -> Result<T> {
    if result.is_ok() {
        return result;
    }
    let stderr = std::io::stderr();
    let mut writer = stderr.lock();
    observe_result_with(stage, result, &mut writer)
}
/// Private seed cannot be formatted, exported or passed to a guest through this API.
pub struct Key {
    signing: SigningKey,
    trust: TrustedLog,
}
impl Key {
    pub fn trust(&self) -> TrustedLog {
        self.trust.clone()
    }
    pub(crate) fn sign(&self, value: &crate::proto::Segment) -> crate::Result<Vec<u8>> {
        crate::sign(value, &self.trust, &self.signing)
    }
    /// Explicit provisioning only. Refuse all existing destinations, even damaged ones.
    pub fn create(path: &Path) -> Result<Self> {
        #[cfg(target_os = "windows")]
        observe_result(KeyDiagnosticStage::DestinationCheck, windows_publication::admit())?;
        #[cfg(target_os = "windows")]
        if observe_result(KeyDiagnosticStage::DestinationCheck, path.try_exists().map_err(KeyError::from))? {
            return observe_result(KeyDiagnosticStage::DestinationCheck, Err(KeyError::AlreadyExists));
        }
        #[cfg(target_os = "linux")]
        match std::fs::symlink_metadata(path) {
            Ok(_) => return Err(KeyError::AlreadyExists),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(KeyError::Io(e)),
        }
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        #[cfg(target_os = "linux")]
        let directory = morrow_core::linux_storage::PrivateDirectory::open(parent, false)?;
        let mut random = Zeroizing::new([0u8; 48]);
        observe_result(KeyDiagnosticStage::Random, platform::random(&mut *random))?;
        let id = format!(
            "audit-{}",
            random[32..]
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>()
        );
        #[cfg(target_os = "windows")]
        let temporary_id = id.clone();
        let mut seed = Zeroizing::new([0u8; 32]);
        seed.copy_from_slice(&random[..32]);
        let public = SigningKey::from_bytes(&seed).verifying_key();
        let material = proto::SecretKey {
            version: 1,
            log_id: id,
            seed: random[..32].to_vec(),
            public_key: public.as_bytes().to_vec(),
        };
        let plain = Zeroizing::new(material.encode_to_vec());
        let packed = Zeroizing::new(lz4_flex::block::compress_prepend_size(&plain));
        let ciphertext = observe_result(KeyDiagnosticStage::Protect, platform::protect(&packed))?;
        let container = proto::ProtectedKey {
            version: 1,
            provider: PROVIDER.into(),
            ciphertext,
        }
        .encode_to_vec();
        let bytes = frame(&container);
        #[cfg(target_os = "windows")]
        {
            let target = path.file_name().and_then(|v| v.to_str()).ok_or(KeyError::Format);
            let mut publication = observe_result(KeyDiagnosticStage::TemporaryIo,
                target.and_then(|target| windows_publication::Publication::create(parent, target, &temporary_id)))?;
            observe_result(KeyDiagnosticStage::TemporaryIo, publication.stage(&bytes))?;
            boundary("key-before-publish");
            observe_result(KeyDiagnosticStage::Publish, publication.publish())?;
            boundary("key-after-publish");
            // Validate the actual bytes through the original exclusive publication handle.
            // The Key is returned only after both original handles checked-close.
            return publication.sync_read_close(Self::from_protected);
        }
        #[cfg(target_os = "linux")]
        {
        let mut file = observe_result(KeyDiagnosticStage::TemporaryIo, tempfile::NamedTempFile::new_in(parent).map_err(KeyError::from))?;
        observe_result(KeyDiagnosticStage::TemporaryIo, file.write_all(&bytes).map_err(KeyError::from))?;
        observe_result(KeyDiagnosticStage::TemporaryIo, file.as_file().sync_all().map_err(KeyError::from))?;
        boundary("key-before-publish");
        #[cfg(target_os = "linux")]
        directory.verify()?;
        let published = observe_result(KeyDiagnosticStage::Publish, file.persist_noclobber(path).map_err(|e| {
            if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                KeyError::AlreadyExists
            } else {
                KeyError::Io(e.error)
            }
        }))?;
        boundary("key-after-publish");
        #[cfg(target_os = "linux")]
        directory.verify()?;
        observe_result(KeyDiagnosticStage::PublishedSync, published.sync_all().map_err(|_| KeyError::PublishUnknown))?;
        #[cfg(target_os = "linux")]
        directory.sync().map_err(|_| KeyError::PublishUnknown)?;
        drop(published);
        }
        #[cfg(target_os = "linux")]
        {
        // Verify the actual published file. If this fails, never replace it on retry.
        Self::load_at(path, KeyDiagnosticStage::ReloadRead)
        }
    }
    pub fn load(path: &Path) -> Result<Self> {
        Self::load_at(path, KeyDiagnosticStage::ExistingRead)
    }
    fn load_at(path: &Path, stage: KeyDiagnosticStage) -> Result<Self> {
        let bytes = observe_result(stage, read_protected(path))?;
        Self::from_protected(&bytes)
    }
    pub(crate) fn create_initialized(path: &Path) -> Result<Self> {
        observe_result(KeyDiagnosticStage::Initialize, Self::create(path))
    }
    pub(crate) fn load_existing(path: &Path) -> Result<Self> {
        observe_result(KeyDiagnosticStage::ExistingLoad, Self::load(path))
    }
    /// Read once; the validated ciphertext is exactly what recovery publishes.
    pub(crate) fn validated_copy(path: &Path) -> Result<(Self, Vec<u8>)> {
        let bytes = read_protected(path)?;
        Ok((Self::from_protected(&bytes)?, bytes))
    }
    pub(crate) fn from_protected(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_FILE || bytes.len() < 44 || !bytes.starts_with(MAGIC) {
            return observe_result(KeyDiagnosticStage::ContainerFormat, Err(KeyError::Format));
        }
        if Sha256::digest(&bytes[40..]).as_slice() != &bytes[8..40] {
            return observe_result(KeyDiagnosticStage::ContainerFormat, Err(KeyError::Format));
        }
        let raw = observe_result(KeyDiagnosticStage::ContainerFormat, unpack(&bytes[40..], MAX_FILE))?;
        let protected =
            observe_result(KeyDiagnosticStage::ContainerFormat, proto::ProtectedKey::decode(raw.as_slice()).map_err(|_| KeyError::Format))?;
        if protected.version != 1
            || protected.provider != PROVIDER
            || protected.ciphertext.len() > 8192
        {
            return observe_result(KeyDiagnosticStage::ContainerFormat, Err(KeyError::Format));
        }
        let packed = observe_result(KeyDiagnosticStage::Unprotect, platform::unprotect(&protected.ciphertext))?;
        let plain = Zeroizing::new(observe_result(KeyDiagnosticStage::MaterialValidate, unpack(&packed, 4096))?);
        let material = observe_result(KeyDiagnosticStage::MaterialValidate, proto::SecretKey::decode(plain.as_slice()).map_err(|_| KeyError::Format))?;
        if material.version != 1
            || material.seed.len() != 32
            || material.log_id.len() != 38
            || !material.log_id.starts_with("audit-")
            || !material.log_id[6..].bytes().all(|v| v.is_ascii_hexdigit())
        {
            return observe_result(KeyDiagnosticStage::MaterialValidate, Err(KeyError::Format));
        }
        let mut seed = Zeroizing::new([0u8; 32]);
        seed.copy_from_slice(&material.seed);
        let signing = SigningKey::from_bytes(&seed);
        if signing.verifying_key().as_bytes().as_slice() != material.public_key {
            return observe_result(KeyDiagnosticStage::MaterialValidate, Err(KeyError::Format));
        }
        let trust = TrustedLog {
            id: material.log_id.clone(),
            key: signing.verifying_key(),
        };
        Ok(Self { signing, trust })
    }
}
pub(crate) fn read_protected(path: &Path) -> Result<Vec<u8>> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        // Open the final leaf itself. This is not ancestor confinement.
        let mut file = std::fs::OpenOptions::new().read(true).share_mode(1)
            .custom_flags(0x00200000).open(path)?; // FILE_FLAG_OPEN_REPARSE_POINT
        let before = file.metadata()?;
        if !before.is_file() || before.file_attributes() & 0x400 != 0
            || before.len() > MAX_FILE as u64 {
            return Err(KeyError::Format);
        }
        let mut bytes = Vec::new();
        (&mut file).take(MAX_FILE as u64 + 1).read_to_end(&mut bytes)?;
        let after = file.metadata()?;
        if bytes.len() > MAX_FILE || bytes.len() as u64 != before.len()
            || !after.is_file() || after.file_attributes() & 0x400 != 0
            || before.len() != after.len()
            || before.creation_time() != after.creation_time()
            || before.last_write_time() != after.last_write_time()
            || before.file_attributes() != after.file_attributes() {
            return Err(KeyError::Format);
        }
        Ok(bytes)
    }
    #[cfg(target_os = "linux")]
    {
    let file = {
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        morrow_core::linux_storage::PrivateDirectory::open(parent, false)?
            .open_read(Path::new(path.file_name().ok_or(KeyError::Format)?))?
    };
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > MAX_FILE as u64 {
        return Err(KeyError::Format);
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_FILE {
        return Err(KeyError::Format);
    }
    Ok(bytes)
    }
}
fn unpack(bytes: &[u8], limit: usize) -> Result<Vec<u8>> {
    if bytes.len() < 4 || u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize > limit {
        return Err(KeyError::Format);
    }
    lz4_flex::block::decompress_size_prepended(bytes).map_err(|_| KeyError::Format)
}
fn boundary(_name: &str) {
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_AUDIT_CRASH_AT").as_deref() == Ok(_name) {
        std::process::exit(86);
    }
}

// Whole compressed-file checksum detects damaged encoding as well as payload;
// authenticity of the secret still comes from the platform provider and derived public key.
fn frame(container: &[u8]) -> Vec<u8> {
    let packed = lz4_flex::block::compress_prepend_size(container);
    [
        MAGIC.as_slice(),
        Sha256::digest(&packed).as_slice(),
        &packed,
    ]
    .concat()
}
// Only the dedicated credential codec may select this separate DPAPI domain.
pub(crate) fn protect_http_credential(input: &[u8]) -> Result<Vec<u8>> {
    platform::protect_http(input)
}
pub(crate) fn unprotect_http_credential(input: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    platform::unprotect_http(input)
}
pub(crate) fn protect_tls_identity(input: &[u8]) -> Result<Vec<u8>> {
    platform::protect_tls(input)
}
pub(crate) fn unprotect_tls_identity(input: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    platform::unprotect_tls(input)
}

#[cfg(all(test, any(target_os = "windows", all(target_os = "linux", feature = "linux-test-keyring"))))]
mod tests {
    use super::*;
    #[test]
    fn authenticated_payload_and_derived_key_are_checked_beyond_file_checksum() {
        #[cfg(target_os = "windows")]
        let d = tempfile::tempdir().unwrap();
        #[cfg(target_os = "linux")]
        let _fixture = test_keyring::activate().unwrap();
        #[cfg(target_os = "linux")]
        let d = {
            use std::os::unix::fs::PermissionsExt;
            tempfile::Builder::new().permissions(std::fs::Permissions::from_mode(0o700)).tempdir().unwrap()
        };
        let p = d.path().join("key");
        Key::create(&p).unwrap();
        let original = std::fs::read(&p).unwrap();
        let raw = unpack(&original[40..], MAX_FILE).unwrap();
        let mut protected = proto::ProtectedKey::decode(raw.as_slice()).unwrap();
        protected.ciphertext[0] ^= 1;
        std::fs::write(&p, frame(&protected.encode_to_vec())).unwrap();
        assert!(Key::load(&p).is_err());
        let mut protected = proto::ProtectedKey::decode(raw.as_slice()).unwrap();
        let decrypted = platform::unprotect(&protected.ciphertext).unwrap();
        let plain = Zeroizing::new(unpack(&decrypted, 4096).unwrap());
        let mut secret = proto::SecretKey::decode(plain.as_slice()).unwrap();
        secret.seed[0] ^= 1; // stale embedded public key, despite valid OS protection
        let raw = Zeroizing::new(secret.encode_to_vec());
        let packed = Zeroizing::new(lz4_flex::block::compress_prepend_size(&raw));
        protected.ciphertext = platform::protect(&packed).unwrap();
        std::fs::write(&p, frame(&protected.encode_to_vec())).unwrap();
        assert!(matches!(Key::load(&p), Err(KeyError::Format)));
    }
}


#[cfg(test)]
mod diagnostic_tests {
    use super::*;

    struct RefusingWriter { calls: usize }
    impl Write for RefusingWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            Err(std::io::Error::from_raw_os_error(1234))
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }

    #[test]
    fn success_is_returned_without_diagnostic_io() {
        let mut writer = RefusingWriter { calls: 0 };
        let value = Box::new(17);
        let address = &*value as *const i32;
        let returned = observe_result_with(KeyDiagnosticStage::Initialize, Ok(value), &mut writer).unwrap();
        assert_eq!(&*returned as *const i32, address);
        assert_eq!(writer.calls, 0);
    }

    #[test]
    fn diagnostic_failure_keeps_original_io_error_object() {
        let original = std::io::Error::new(std::io::ErrorKind::Other, String::from("private-error-canary"));
        let address = original.get_ref().unwrap() as *const _ as *const ();
        let mut writer = RefusingWriter { calls: 0 };
        let returned: Result<()> = observe_result_with(
            KeyDiagnosticStage::TemporaryIo, Err(KeyError::Io(original)), &mut writer,
        );
        match returned {
            Err(KeyError::Io(error)) => {
                assert_eq!(error.get_ref().unwrap() as *const _ as *const (), address);
                assert_eq!(error.kind(), std::io::ErrorKind::Other);
            }
            _ => panic!("original error was replaced"),
        }
        assert_eq!(writer.calls, 1); // No retry after a diagnostic write failure.
    }

    #[test]
    fn all_error_kinds_emit_only_fixed_labels_and_preserve_variants() {
        let errors = [
            (KeyError::Io(std::io::Error::new(std::io::ErrorKind::Other, "private-error-canary")), b"io".as_slice()),
            (KeyError::Format, b"format".as_slice()),
            (KeyError::Protection, b"protection".as_slice()),
            (KeyError::Random, b"random".as_slice()),
            (KeyError::AlreadyExists, b"already_exists".as_slice()),
            (KeyError::PublishUnknown, b"publish_unknown".as_slice()),
        ];
        for (error, label) in errors {
            let before = std::mem::discriminant(&error);
            let mut bytes = Vec::new();
            let returned: Result<()> = observe_result_with(KeyDiagnosticStage::Initialize, Err(error), &mut bytes);
            assert_eq!(std::mem::discriminant(&returned.unwrap_err()), before);
            let mut expected = b"MORROW_AUDIT_KEY_DIAGNOSTIC stage=initialize kind=".to_vec();
            expected.extend_from_slice(label);
            expected.push(b'\n');
            assert_eq!(bytes, expected);
            assert!(!bytes.windows(b"private-error-canary".len()).any(|w| w == b"private-error-canary"));
        }
    }

    #[test]
    fn stage_labels_are_exact_unique_and_bounded() {
        let stages = [
            (KeyDiagnosticStage::Initialize, b"initialize".as_slice()),
            (KeyDiagnosticStage::ExistingLoad, b"existing_load".as_slice()),
            (KeyDiagnosticStage::DestinationCheck, b"destination_check".as_slice()),
            (KeyDiagnosticStage::Random, b"random".as_slice()),
            (KeyDiagnosticStage::Protect, b"protect".as_slice()),
            (KeyDiagnosticStage::TemporaryIo, b"temporary_io".as_slice()),
            (KeyDiagnosticStage::Publish, b"publish".as_slice()),
            (KeyDiagnosticStage::PublishedSync, b"published_sync".as_slice()),
            (KeyDiagnosticStage::ExistingRead, b"existing_read".as_slice()),
            (KeyDiagnosticStage::ReloadRead, b"reload_read".as_slice()),
            (KeyDiagnosticStage::ContainerFormat, b"container_format".as_slice()),
            (KeyDiagnosticStage::Unprotect, b"unprotect".as_slice()),
            (KeyDiagnosticStage::MaterialValidate, b"material_validate".as_slice()),
        ];
        let mut seen = std::collections::BTreeSet::new();
        for (stage, label) in stages {
            assert_eq!(stage.code(), label);
            assert!(seen.insert(label));
            assert!(label.len() <= 20 && label.iter().all(|b| b.is_ascii_lowercase() || *b == b'_'));
            let mut bytes = Vec::new();
            let returned: Result<()> = observe_result_with(stage, Err(KeyError::Protection), &mut bytes);
            assert!(matches!(returned, Err(KeyError::Protection)));
            assert!(bytes.len() <= 96 && bytes.last() == Some(&b'\n'));
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_reader_tests {
    use super::*;
    use std::{fs, path::PathBuf, sync::atomic::{AtomicU64, Ordering}};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn fixture() -> PathBuf {
        // The test runner selects a fresh owned TEMP/TMP. Retain synthetic files.
        let root = std::env::temp_dir().join(format!("morrow-key-reader-{}-{}",
            std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&root).unwrap();
        root
    }
    fn write_new(path: &Path, bytes: &[u8]) {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }
    #[test]
    fn ordinary_bytes_and_exact_cap_are_read_without_provider_calls() {
        let root = fixture();
        let path = root.join("ordinary.bin");
        write_new(&path, b"synthetic-reader");
        assert_eq!(read_protected(&path).unwrap(), b"synthetic-reader");
        assert_eq!(fs::read(&path).unwrap(), b"synthetic-reader");
        let cap = root.join("at-cap.bin");
        let bytes = vec![0x5a; MAX_FILE];
        write_new(&cap, &bytes);
        assert_eq!(read_protected(&cap).unwrap(), bytes);
    }
    #[test]
    fn directory_input_is_rejected_without_file_effects() {
        let root = fixture();
        let path = root.join("directory");
        fs::create_dir(&path).unwrap();
        assert!(read_protected(&path).is_err());
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
    }
    #[test]
    fn over_cap_regular_file_is_rejected_and_preserved() {
        let root = fixture();
        let path = root.join("over-cap.bin");
        let file = fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
        file.set_len(MAX_FILE as u64 + 1).unwrap();
        file.sync_all().unwrap();
        drop(file);
        assert!(matches!(read_protected(&path), Err(KeyError::Format)));
        assert_eq!(fs::metadata(&path).unwrap().len(), MAX_FILE as u64 + 1);
    }
    #[test]
    fn actual_leaf_symlink_is_rejected_without_reading_target_as_key() {
        use std::os::windows::fs::{symlink_file, MetadataExt};
        let root = fixture();
        let target = root.join("target.bin");
        let link = root.join("leaf-link.bin");
        write_new(&target, b"synthetic-reader");
        // A permission/setup failure is a real test failure, never a skipped PASS.
        symlink_file(&target, &link).expect("real owned file symlink required; unavailable means NOT_COVERED");
        assert_ne!(fs::symlink_metadata(&link).unwrap().file_attributes() & 0x400, 0);
        assert_eq!(fs::read(&link).unwrap(), b"synthetic-reader");
        assert!(matches!(read_protected(&link), Err(KeyError::Format)));
        assert_eq!(fs::read(&target).unwrap(), b"synthetic-reader");
        assert_ne!(fs::symlink_metadata(&link).unwrap().file_attributes() & 0x400, 0);
    }
}
