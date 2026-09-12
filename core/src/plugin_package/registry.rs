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
pub const MAX_DEPENDENCY_LOCKS: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub package_id: String,
    pub digest: [u8; 32],
    pub enabled: bool,
    /// Host-approved ceiling; individual object grants must still be issued separately.
    pub approved: BTreeSet<GrantKind>,
}
/// Persistent exact-version approval; this is not a live instance binding or object grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockedDependency {
    pub caller_id: String,
    pub caller_digest: [u8; 32],
    pub slot: String,
    pub provider_id: String,
    pub provider_digest: [u8; 32],
}
type DependencyLocks = BTreeMap<(String, String), LockedDependency>;
/// Must remain owned by one trusted manager. The persistent lock file is not a PID marker.
pub struct Registry {
    root: PathBuf,
    catalog: Catalog,
    _lease: File,
    revision: u64,
    selections: BTreeMap<String, Selection>,
    dependencies: DependencyLocks,
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
            dependencies: BTreeMap::new(),
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
            if state.revision == 0
                || state.selections.len() > MAX_SELECTIONS
                || state.dependency_locks.len() > MAX_DEPENDENCY_LOCKS
            {
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
            let mut previous = None;
            for entry in state.dependency_locks {
                identity(&entry.caller_id)?;
                identity(&entry.slot)?;
                identity(&entry.provider_id)?;
                let key = (entry.caller_id.clone(), entry.slot.clone());
                if previous.as_ref().is_some_and(|p| p >= &key) {
                    return Err(Error::Invalid("dependency lock ordering"));
                }
                previous = Some(key.clone());
                result.dependencies.insert(
                    key,
                    LockedDependency {
                        caller_id: entry.caller_id,
                        caller_digest: entry
                            .caller_digest
                            .try_into()
                            .map_err(|_| Error::Integrity)?,
                        slot: entry.slot,
                        provider_id: entry.provider_id,
                        provider_digest: entry
                            .provider_digest
                            .try_into()
                            .map_err(|_| Error::Integrity)?,
                    },
                );
            }
            result.revision = state.revision;
            // Fail closed if any persisted selection no longer names a valid installed package.
            for selection in result.selections.values() {
                result.load(selection)?;
            }
            result.validate_dependencies(&result.dependencies)?;
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
    pub fn dependency(&self, caller: &str, slot: &str) -> Option<&LockedDependency> {
        self.dependencies.get(&(caller.into(), slot.into()))
    }
    // Validate declarations and exact current selections without requiring enablement or
    // completeness. This permits opening and approving a partially configured registry.
    fn load_dependency(&self, lock: &LockedDependency) -> Result<(Package, Package, Selection)> {
        let caller = self
            .selections
            .get(&lock.caller_id)
            .ok_or(Error::NotFound)?;
        let provider = self
            .selections
            .get(&lock.provider_id)
            .ok_or(Error::NotFound)?;
        if caller.digest != lock.caller_digest || provider.digest != lock.provider_digest {
            return Err(Error::RevisionConflict);
        }
        let caller_package = self.load(caller)?;
        let provider_package = self.load(provider)?;
        caller_package.check_dependency(&lock.slot, &provider_package)?;
        Ok((caller_package, provider_package, provider.clone()))
    }
    fn validate_dependencies(&self, locks: &DependencyLocks) -> Result<()> {
        if locks.len() > MAX_DEPENDENCY_LOCKS {
            return Err(Error::Limit);
        }
        let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut incoming: BTreeMap<String, usize> = BTreeMap::new();
        for lock in locks.values() {
            let (caller, _, _) = self.load_dependency(lock)?;
            if caller.dependency(&lock.slot)?.optional {
                continue;
            }
            incoming.entry(lock.caller_id.clone()).or_default();
            if edges
                .entry(lock.caller_id.clone())
                .or_default()
                .insert(lock.provider_id.clone())
            {
                *incoming.entry(lock.provider_id.clone()).or_default() += 1;
            }
        }
        // Iterative topological check: bounded by selections/locks, not native stack depth.
        let mut ready: Vec<_> = incoming
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| id.clone())
            .collect();
        let mut visited = 0;
        while let Some(id) = ready.pop() {
            visited += 1;
            if let Some(providers) = edges.get(&id) {
                for provider in providers {
                    let count = incoming.get_mut(provider).expect("edge target");
                    *count -= 1;
                    if *count == 0 {
                        ready.push(provider.clone());
                    }
                }
            }
        }
        if visited != incoming.len() {
            return Err(Error::Invalid("required dependency cycle"));
        }
        Ok(())
    }
    /// Returns a fresh validated package and a ceiling snapshot, not an execution permit.
    /// Every required edge must resolve; optional edges never gate starting the caller.
    pub fn resolve_enabled(&self, id: &str) -> Result<(Package, Selection)> {
        let mut active = BTreeSet::new();
        let mut complete = BTreeSet::new();
        let mut pending = vec![(id.to_owned(), false)];
        while let Some((current, exiting)) = pending.pop() {
            if exiting {
                active.remove(&current);
                complete.insert(current);
                continue;
            }
            if complete.contains(&current) {
                continue;
            }
            if !active.insert(current.clone()) {
                return Err(Error::Invalid("required dependency cycle"));
            }
            let selection = self.selections.get(&current).ok_or(Error::NotFound)?;
            if !selection.enabled {
                return Err(Error::Invalid("plugin disabled"));
            }
            let package = self.load(selection)?;
            pending.push((current.clone(), true));
            for requirement in &package.manifest().dependencies {
                if requirement.optional {
                    continue;
                }
                let lock = self
                    .dependency(&current, &requirement.slot)
                    .ok_or(Error::NotFound)?;
                self.load_dependency(lock)?;
                pending.push((lock.provider_id.clone(), false));
            }
        }
        let selection = self.selections.get(id).ok_or(Error::NotFound)?;
        Ok((self.load(selection)?, selection.clone()))
    }
    /// Explicit slot resolution also requires a usable caller and provider required closure.
    pub fn resolve_dependency(
        &self,
        caller: &str,
        slot: &str,
    ) -> Result<(LockedDependency, Package, Selection)> {
        self.resolve_enabled(caller)?;
        let lock = self.dependency(caller, slot).ok_or(Error::NotFound)?;
        self.load_dependency(lock)?;
        let (package, selection) = self.resolve_enabled(&lock.provider_id)?;
        Ok((lock.clone(), package, selection))
    }
    /// Sorted transitive required consumers, excluding the changed package itself.
    /// Independent of enabled state. Corrupt declarations conservatively count as required,
    /// so a manager never misses revocation merely because a catalog read failed.
    pub fn required_dependents(&self, id: &str) -> Vec<String> {
        let mut reverse: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for lock in self.dependencies.values() {
            let optional = self
                .selections
                .get(&lock.caller_id)
                .and_then(|s| self.load(s).ok())
                .and_then(|p| p.dependency(&lock.slot).ok().map(|d| d.optional))
                .unwrap_or(false);
            if !optional {
                reverse
                    .entry(lock.provider_id.clone())
                    .or_default()
                    .insert(lock.caller_id.clone());
            }
        }
        let mut reached = BTreeSet::from([id.to_owned()]);
        let mut pending = vec![id.to_owned()];
        while let Some(provider) = pending.pop() {
            if let Some(callers) = reverse.get(&provider) {
                for caller in callers {
                    if reached.insert(caller.clone()) {
                        pending.push(caller.clone());
                    }
                }
            }
        }
        reached.remove(id);
        reached.into_iter().collect()
    }
    /// Trusted explicit approval. Manager must first stop the caller and affected consumers.
    pub fn approve_dependency(
        &mut self,
        caller_id: &str,
        caller_digest: [u8; 32],
        slot: &str,
        provider_id: &str,
        provider_digest: [u8; 32],
        expected_revision: u64,
    ) -> Result<()> {
        self.check_revision(expected_revision)?;
        identity(caller_id)?;
        identity(slot)?;
        identity(provider_id)?;
        let mut next = self.dependencies.clone();
        next.insert(
            (caller_id.into(), slot.into()),
            LockedDependency {
                caller_id: caller_id.into(),
                caller_digest,
                slot: slot.into(),
                provider_id: provider_id.into(),
                provider_digest,
            },
        );
        self.validate_dependencies(&next)?;
        self.commit(self.selections.clone(), next)
    }
    pub fn remove_dependency(
        &mut self,
        caller: &str,
        slot: &str,
        expected_revision: u64,
    ) -> Result<()> {
        self.check_revision(expected_revision)?;
        let mut next = self.dependencies.clone();
        if next.remove(&(caller.into(), slot.into())).is_none() {
            return Err(Error::NotFound);
        }
        self.commit(self.selections.clone(), next)
    }
    fn check_revision(&self, expected: u64) -> Result<()> {
        if expected != self.revision {
            return Err(Error::RevisionConflict);
        }
        Ok(())
    }
    fn commit(
        &mut self,
        next: BTreeMap<String, Selection>,
        dependencies: DependencyLocks,
    ) -> Result<()> {
        if next == self.selections && dependencies == self.dependencies {
            return Ok(());
        }
        if next.len() > MAX_SELECTIONS || dependencies.len() > MAX_DEPENDENCY_LOCKS {
            return Err(Error::Limit);
        }
        let revision = self.revision.checked_add(1).ok_or(Error::Limit)?;
        let state = proto::Registry {
            schema_version: 1,
            revision,
            dependency_locks: dependencies
                .values()
                .map(|lock| proto::LockedDependency {
                    caller_id: lock.caller_id.clone(),
                    caller_digest: lock.caller_digest.to_vec(),
                    slot: lock.slot.clone(),
                    provider_id: lock.provider_id.clone(),
                    provider_digest: lock.provider_digest.to_vec(),
                })
                .collect(),
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
        self.dependencies = dependencies;
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
        let mut dependencies = self.dependencies.clone();
        dependencies.retain(|_, lock| lock.caller_id != *id && lock.provider_id != *id);
        self.commit(next, dependencies)
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
        self.commit(next, self.dependencies.clone())
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
        self.commit(next, self.dependencies.clone())
    }
    /// Forget selection and approval only; immutable packages and user content remain untouched.
    pub fn remove(&mut self, id: &str, expected_revision: u64) -> Result<()> {
        self.check_revision(expected_revision)?;
        let mut next = self.selections.clone();
        if next.remove(id).is_none() {
            return Err(Error::NotFound);
        }
        let mut dependencies = self.dependencies.clone();
        dependencies.retain(|_, lock| lock.caller_id != id && lock.provider_id != id);
        self.commit(next, dependencies)
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
            dependency_locks: vec![],
            selections: vec![],
        };
        let mut unknown = state.encode_to_vec();
        unknown.extend_from_slice(&[0x28, 1]); // Unknown field 5.
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
            dependency_locks: vec![],
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
            dependency_locks: vec![],
            selections: vec![unknown],
        };
        assert_eq!(reject(&state.encode_to_vec()), Error::UnsupportedVersion);
    }
}
