//! File-only checks. Synthetic PE headers are never loaded or executed. The
//! preserved-image test reads three immutable artifacts, never stages copies.
use super::*;
use std::{fs, io::Write, sync::atomic::{AtomicU64, Ordering}};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "morrow-preflight-006-{}-{}",
            std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn image(&self, name: &str, marker: u8) -> Artifact {
        let mut bytes = vec![0u8; 512];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
        bytes[88..90].copy_from_slice(&0x20bu16.to_le_bytes());
        bytes[511] = marker;
        let path = self.0.join(name);
        let mut file = File::options().write(true).create_new(true).open(&path).unwrap();
        file.write_all(&bytes).unwrap();
        file.sync_all().unwrap();
        Artifact { path, sha256: Sha256::digest(&bytes).into() }
    }
    fn config(&self) -> Config {
        Config {
            synthetic_root: self.0.join("fresh-owned-root"),
            runner: self.image("codex-command-runner.exe", 1),
            setup: self.image("codex-windows-sandbox-setup.exe", 2),
            helper: self.image("harness.exe", 3),
        }
    }
    fn materialize(&self, config: &Config) -> Artifact {
        let bin = config.synthetic_root.join("codex-home").join(".sandbox-bin");
        fs::create_dir_all(&bin).unwrap();
        let path = bin.join("codex-command-runner-0.0.0-512-100.exe");
        fs::copy(&config.runner.path, &path).unwrap();
        Artifact { path, sha256: config.runner.sha256 }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // Only the unique directory that this fixture created is removed.
        assert!(self.0.file_name().unwrap().to_string_lossy().starts_with("morrow-preflight-006-"));
        assert_eq!(self.0.parent().unwrap().canonicalize().unwrap(), std::env::temp_dir().canonicalize().unwrap());
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn minimal_preflight_has_zero_actions_and_preserves_staged_sibling_policy() {
    let fixture = Fixture::new();
    let mut config = fixture.config();
    let observation = preflight(&config).unwrap();
    assert_eq!(observation.system_actions, 0);
    assert!(!observation.production_qualified && !observation.protected_session_qualified && !observation.sdk_complete);
    assert_eq!(observation.artifacts.len(), 3);
    assert!(!config.synthetic_root.exists());
    let outside = fixture.0.join("other");
    fs::create_dir(&outside).unwrap();
    let moved = outside.join("harness.exe");
    fs::copy(&config.helper.path, &moved).unwrap();
    config.helper.path = moved;
    assert!(observe_images(&config).is_err());
}

#[test]
fn oversized_regular_image_is_rejected_before_hash_and_wrong_sha_is_rejected() {
    let fixture = Fixture::new();
    let pin = fixture.image("bounded.exe", 9);
    let mut wrong = pin.clone();
    wrong.sha256 = [1; 32];
    assert!(observe_artifact(&wrong).err().unwrap().to_string().contains("SHA-256 mismatch"));
    File::options().write(true).open(&pin.path).unwrap().set_len(MAX_PE_BYTES + 1).unwrap();
    // The old digest would fail if hashing were reached. Require the size error.
    assert!(observe_artifact(&pin).err().unwrap().to_string().contains("PE bound"));
    assert_eq!(MAX_PE_BYTES, 128 * 1024 * 1024);
}

#[test]
fn preserved_build008_runner_and_setup_are_readonly_observed_at_fixed_identity() {
    let images = [
        ("MORROW_PREFLIGHT_BUILD008", "98ab9b4906d55a75475144a224dc53080b727f045d13f97c71d84f447ad3ba2d", 104018432u64),
        ("MORROW_PREFLIGHT_ORIGINAL_RUNNER", "89fcc9d29d2939bb52f777406957d2ce1ffbd311972037a4764e8be4eb061f54", 26988032u64),
        ("MORROW_PREFLIGHT_ORIGINAL_SETUP", "1fcd3a511746f98e7f8741a3709dfa305d1d9b6fef2a78eaaa384d4701bc4234", 40924672u64),
    ];
    for (variable, sha, bytes) in images {
        // Environment supplies locations only: neither expected SHA nor length
        // is caller-controlled and no missing-input skip can count as a pass.
        let pin = Artifact { path: PathBuf::from(std::env::var_os(variable).expect("root must provide the immutable preserved image path")), sha256: digest(sha).unwrap() };
        let observation = observe_artifact(&pin).unwrap();
        assert_eq!(observation.bytes, bytes);
        assert_eq!(observation.sha256, sha);
        assert_eq!(observation.machine, "AMD64_PE32_PLUS");
    }
    assert!(104018432u64 > 64 * 1024 * 1024 && 104018432u64 < MAX_PE_BYTES);
}

#[test]
fn materialized_policy_accepts_owned_resolved_path_and_rejects_source_or_other_root() {
    let fixture = Fixture::new();
    let config = fixture.config();
    let actual = fixture.materialize(&config);
    let observed = observe_materialized_images(&config, &actual).unwrap();
    assert_eq!(observed.len(), 4);
    assert_ne!(observed["materialized_runner"].canonical_path, observed["runner"].canonical_path);
    assert!(observe_materialized_images(&config, &config.runner).is_err());
    let mut wrong_root = config.clone();
    wrong_root.synthetic_root = fixture.0.clone();
    assert!(observe_materialized_images(&wrong_root, &actual).is_err());
    let nested = actual.path.parent().unwrap().join("nested");
    fs::create_dir(&nested).unwrap();
    let nested_pin = Artifact { path: nested.join("runner.exe"), sha256: actual.sha256 };
    fs::copy(&actual.path, &nested_pin.path).unwrap();
    assert!(observe_materialized_images(&config, &nested_pin).is_err());
}

#[test]
fn materialized_policy_keeps_stage_siblings_and_full_source_digest_mandatory() {
    let fixture = Fixture::new();
    let mut config = fixture.config();
    let actual = fixture.materialize(&config);
    let alien = fixture.image("different-source.exe", 8);
    config.runner = alien;
    assert!(observe_materialized_images(&config, &actual).err().unwrap().to_string().contains("source full SHA"));
    let outside = fixture.0.join("separate-stage");
    fs::create_dir(&outside).unwrap();
    let moved = outside.join("setup.exe");
    fs::copy(&config.setup.path, &moved).unwrap();
    config.setup.path = moved;
    assert!(observe_materialized_images(&config, &actual).err().unwrap().to_string().contains("same approved artifact directory"));
}
