//! Native, cooperative selection registry. This does not revoke running instances.
use super::{Package, capability, catalog::Catalog, proto::Capability};
use crate::{Error, Result, envelope, identity, lifecycle::GrantKind};
use prost::Message;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.plugin.registry.v1.rs"));
}
const MAGIC: &[u8; 8] = b"MORROWG1";
const MAX_RAW: usize = 512 * 1024;
const MAX_CONTAINER: usize = MAX_RAW + MAX_RAW / 255 + 128;
pub const MAX_SELECTIONS: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub package_id: String,
    pub digest: [u8; 32],
    pub enabled: bool,
    /// Host-approved ceiling; individual object grants must still be issued separately.
    pub approved: BTreeSet<GrantKind>,
}
/// Must remain owned by one trusted manager. The persistent lock file is not a PID marker.
pub struct Registry {
    root: PathBuf,
    catalog: Catalog,
    _lease: File,
    revision: u64,
    selections: BTreeMap<String, Selection>,
}
fn regular_or_absent(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => Ok(true),
        Ok(_) => Err(Error::Invalid("registry file type")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(Error::Io),
    }
}
fn cap_number(kind: GrantKind) -> i32 {
    (match kind {
        GrantKind::Rename => Capability::RenameCard,
        GrantKind::ReadSummary => Capability::ReadSummary,
        GrantKind::QueryOperation => Capability::QueryOperation,
        GrantKind::ReadAttachment => Capability::ReadAttachment,
        GrantKind::CreateContent => Capability::CreateContent,
        GrantKind::EditContent => Capability::EditContent,
        GrantKind::ReadContent => Capability::ReadContent,
    }) as i32
}
impl Registry {
    /// Registry and catalog roots belong to the trusted host, not to package metadata.
    pub fn open(root: &Path, catalog: Catalog) -> Result<Self> {
        if let Ok(meta) = fs::symlink_metadata(root)
            && !meta.file_type().is_dir()
        {
            return Err(Error::Invalid("registry directory type"));
        }
        fs::create_dir_all(root).map_err(|_| Error::Io)?;
        let root = root.canonicalize().map_err(|_| Error::Io)?;
        let lock = root.join("registry.lock");
        regular_or_absent(&lock)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3); // Keep the locked file from being deleted/replaced while live.
        }
        let lease = options.open(lock).map_err(|_| Error::Io)?;
        lease.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => Error::StorageBusy,
            std::fs::TryLockError::Error(_) => Error::Io,
        })?;
        let mut result = Self {
            root,
            catalog,
            _lease: lease,
            revision: 0,
            selections: BTreeMap::new(),
        };
        let path = result.path();
        if regular_or_absent(&path)? {
            let mut bytes = Vec::new();
            File::open(path)
                .map_err(|_| Error::Io)?
                .take(MAX_CONTAINER as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| Error::Io)?;
            let raw = envelope::unpack(MAGIC, &bytes, MAX_RAW)?;
            let state = proto::Registry::decode(raw.as_slice())
                .map_err(|_| Error::Invalid("registry protobuf"))?;
            // Refuse unknown/noncanonical data rather than silently rewriting and losing it.
            if state.schema_version != 1 || state.encode_to_vec() != raw {
                return Err(Error::UnsupportedVersion);
            }
            if state.revision == 0 || state.selections.len() > MAX_SELECTIONS {
                return Err(Error::Limit);
            }
            let mut previous = None;
            for entry in state.selections {
                identity(&entry.package_id)?;
                if previous
                    .as_ref()
                    .is_some_and(|p: &String| p >= &entry.package_id)
                {
                    return Err(Error::Invalid("registry ordering"));
                }
                previous = Some(entry.package_id.clone());
                let digest: [u8; 32] = entry.digest.try_into().map_err(|_| Error::Integrity)?;
                if entry.approved_capabilities.len() > 7 {
                    return Err(Error::Limit);
                }
                let mut approved = BTreeSet::new();
                for cap in entry.approved_capabilities {
                    if !approved.insert(capability(cap)?) {
                        return Err(Error::Invalid("duplicate approval"));
                    }
                }
                result.selections.insert(
                    entry.package_id.clone(),
                    Selection {
                        package_id: entry.package_id,
                        digest,
                        enabled: entry.enabled,
                        approved,
                    },
                );
            }
            result.revision = state.revision;
            // Fail closed if any persisted selection no longer names a valid installed package.
            for selection in result.selections.values() {
                result.load(selection)?;
            }
        }
        Ok(result)
    }
    fn path(&self) -> PathBuf {
        self.root.join("selection.morrow")
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn selections(&self) -> impl Iterator<Item = &Selection> {
        self.selections.values()
    }
    pub fn selection(&self, id: &str) -> Option<&Selection> {
        self.selections.get(id)
    }
    fn load(&self, selection: &Selection) -> Result<Package> {
        let package = self.catalog.load(selection.digest)?;
        if package.manifest().package_id != selection.package_id
            || !selection.approved.is_subset(package.capabilities())
        {
            return Err(Error::Integrity);
        }
        Ok(package)
    }
    /// Returns a fresh validated package and a ceiling snapshot, not an execution permit.
    pub fn resolve_enabled(&self, id: &str) -> Result<(Package, Selection)> {
        let selection = self.selections.get(id).ok_or(Error::NotFound)?;
        if !selection.enabled {
            return Err(Error::Invalid("plugin disabled"));
        }
        Ok((self.load(selection)?, selection.clone()))
    }
    fn check_revision(&self, expected: u64) -> Result<()> {
        if expected != self.revision {
            return Err(Error::RevisionConflict);
        }
        Ok(())
    }
    fn commit(&mut self, next: BTreeMap<String, Selection>) -> Result<()> {
        if next == self.selections {
            return Ok(());
        }
        if next.len() > MAX_SELECTIONS {
            return Err(Error::Limit);
        }
        let revision = self.revision.checked_add(1).ok_or(Error::Limit)?;
        let state = proto::Registry {
            schema_version: 1,
            revision,
            selections: next
                .values()
                .map(|s| proto::Selection {
                    package_id: s.package_id.clone(),
                    digest: s.digest.to_vec(),
                    enabled: s.enabled,
                    approved_capabilities: s.approved.iter().copied().map(cap_number).collect(),
                })
                .collect(),
        };
        let bytes = envelope::pack(MAGIC, &state.encode_to_vec(), MAX_RAW)?;
        regular_or_absent(&self.path())?;
        let mut staged = tempfile::NamedTempFile::new_in(&self.root).map_err(|_| Error::Io)?;
        staged.write_all(&bytes).map_err(|_| Error::Io)?;
        staged.as_file().sync_all().map_err(|_| Error::Io)?;
        staged.persist(self.path()).map_err(|_| Error::Io)?;
        self.selections = next;
        self.revision = revision;
        Ok(())
    }
    /// First selection is disabled with no approvals. Upgrade requires a newer SemVer;
    /// retained approvals are intersected with the new manifest, never expanded.
    pub fn select(&mut self, digest: [u8; 32], expected_revision: u64) -> Result<()> {
        self.check_revision(expected_revision)?;
        let package = self.catalog.load(digest)?;
        let id = &package.manifest().package_id;
        let mut selection = Selection {
            package_id: id.clone(),
            digest,
            enabled: false,
            approved: BTreeSet::new(),
        };
        if let Some(previous) = self.selections.get(id) {
            let old = self.load(previous)?;
            if previous.digest == digest {
                return Ok(());
            }
            let old_version = semver::Version::parse(&old.manifest().package_version)
                .map_err(|_| Error::Invalid("package version"))?;
            let new_version = semver::Version::parse(&package.manifest().package_version)
                .map_err(|_| Error::Invalid("package version"))?;
            if new_version.cmp_precedence(&old_version) != std::cmp::Ordering::Greater {
                return Err(Error::RevisionConflict);
            }
            // A new immutable version requires a fresh explicit enable decision.
            selection.enabled = false;
            selection.approved = previous
                .approved
                .intersection(package.capabilities())
                .copied()
                .collect();
        }
        let mut next = self.selections.clone();
        next.insert(id.clone(), selection);
        self.commit(next)
    }
    /// Explicit trusted approval decision, not an implicit consequence of selecting a version.
    pub fn approve(
        &mut self,
        id: &str,
        expected_digest: [u8; 32],
        approved: BTreeSet<GrantKind>,
        expected_revision: u64,
    ) -> Result<()> {
        self.check_revision(expected_revision)?;
        let selection = self.selections.get(id).ok_or(Error::NotFound)?;
        if selection.digest != expected_digest {
            return Err(Error::RevisionConflict);
        }
        let package = self.load(selection)?;
        if !approved.is_subset(package.capabilities()) {
            return Err(Error::Invalid("approval exceeds declaration"));
        }
        let mut next = self.selections.clone();
        next.get_mut(id).expect("existing selection").approved = approved;
        self.commit(next)
    }
    /// Controls future trusted resolution only. Caller must separately stop existing instances.
    pub fn set_enabled(
        &mut self,
        id: &str,
        expected_digest: [u8; 32],
        enabled: bool,
        expected_revision: u64,
    ) -> Result<()> {
        self.check_revision(expected_revision)?;
        let selection = self.selections.get(id).ok_or(Error::NotFound)?;
        if selection.digest != expected_digest {
            return Err(Error::RevisionConflict);
        }
        if enabled {
            self.load(selection)?;
        }
        let mut next = self.selections.clone();
        next.get_mut(id).expect("existing selection").enabled = enabled;
        self.commit(next)
    }
    /// Forget selection and approval only; immutable packages and user content remain untouched.
    pub fn remove(&mut self, id: &str, expected_revision: u64) -> Result<()> {
        self.check_revision(expected_revision)?;
        let mut next = self.selections.clone();
        if next.remove(id).is_none() {
            return Err(Error::NotFound);
        }
        self.commit(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reject(raw: &[u8]) -> Error {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("registry");
        fs::create_dir(&root).unwrap();
        let bytes = envelope::pack(MAGIC, raw, MAX_RAW).unwrap();
        fs::write(root.join("selection.morrow"), &bytes).unwrap();
        let result = Registry::open(&root, Catalog::open(&dir.path().join("packages")).unwrap());
        assert_eq!(fs::read(root.join("selection.morrow")).unwrap(), bytes);
        match result {
            Err(e) => e,
            Ok(_) => panic!("accepted invalid registry"),
        }
    }
    #[test]
    fn unknown_semantics_and_noncanonical_data_never_get_rewritten() {
        let state = proto::Registry {
            schema_version: 1,
            revision: 1,
            selections: vec![],
        };
        let mut unknown = state.encode_to_vec();
        unknown.extend_from_slice(&[0x20, 1]); // Unknown field 4.
        assert_eq!(reject(&unknown), Error::UnsupportedVersion);
        let mut duplicate = state.encode_to_vec();
        duplicate.extend_from_slice(&[8, 1]); // Repeated scalar field.
        assert_eq!(reject(&duplicate), Error::UnsupportedVersion);
        let mut future = state.clone();
        future.schema_version = 2;
        assert_eq!(reject(&future.encode_to_vec()), Error::UnsupportedVersion);
        let mut zero = state;
        zero.revision = 0;
        assert_eq!(reject(&zero.encode_to_vec()), Error::Limit);
    }
    #[test]
    fn duplicate_selection_and_unknown_approval_are_rejected() {
        let entry = proto::Selection {
            package_id: "org.example.test".into(),
            digest: vec![1; 32],
            enabled: false,
            approved_capabilities: vec![],
        };
        let duplicate = proto::Registry {
            schema_version: 1,
            revision: 1,
            selections: vec![entry.clone(), entry.clone()],
        };
        assert_eq!(
            reject(&duplicate.encode_to_vec()),
            Error::Invalid("registry ordering")
        );
        let mut unknown = entry;
        unknown.approved_capabilities = vec![99];
        let state = proto::Registry {
            schema_version: 1,
            revision: 1,
            selections: vec![unknown],
        };
        assert_eq!(reject(&state.encode_to_vec()), Error::UnsupportedVersion);
    }
}
