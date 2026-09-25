//! Shared selection and approval rules. This does not revoke running instances.
use super::{Package, capability, io::IoCapability, proto::Capability};
use crate::{Error, Result, envelope, identity, lifecycle::GrantKind};
use prost::Message;
use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
pub mod sqlite;
mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.plugin.registry.v1.rs"));
}
const MAGIC: &[u8; 8] = b"MORROWG1";
const MAX_RAW: usize = 512 * 1024;
pub const MAX_CONTAINER: usize = MAX_RAW + MAX_RAW / 255 + 128;
pub const MAX_SELECTIONS: usize = 1024;
pub const MAX_DEPENDENCY_LOCKS: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub package_id: String,
    pub digest: [u8; 32],
    pub enabled: bool,
    /// Host-approved ceiling; individual object grants must still be issued separately.
    pub approved: BTreeSet<GrantKind>,
    /// Independent IO ceiling. Resource scopes and live grants are never restored here.
    pub approved_io: BTreeSet<IoCapability>,
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
/// Trusted persistence boundary, never implemented by a guest. An implementation
/// owns an exclusive lease until drop, validates immutable package bytes on load,
/// and publishes a whole synced snapshot atomically. An uncertain commit must
/// fail closed; the owner must reopen before attempting another decision.
pub trait RegistryStorage: Send {
    fn read(&self) -> Result<Option<Vec<u8>>>;
    fn publish(&mut self, bytes: &[u8]) -> Result<()>;
    fn load_package(&self, digest: [u8; 32]) -> Result<Package>;
    fn install_package(&self, package: &Package) -> Result<()>;
}

/// Must remain owned by one trusted manager. Live grants are never restored.
pub struct Registry {
    storage: Box<dyn RegistryStorage>,
    uncertain: std::cell::Cell<bool>,
    revision: u64,
    selections: BTreeMap<String, Selection>,
    dependencies: DependencyLocks,
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
    /// The adapter must hold its exclusive ownership lease before this call.
    pub fn from_storage(storage: Box<dyn RegistryStorage>) -> Result<Self> {
        let mut result = Self {
            storage,
            uncertain: std::cell::Cell::new(false),
            revision: 0,
            selections: BTreeMap::new(),
            dependencies: BTreeMap::new(),
        };
        if let Some(bytes) = result.storage.read()? {
            if bytes.len() > MAX_CONTAINER {
                return Err(Error::Limit);
            }
            let raw = envelope::unpack(MAGIC, &bytes, MAX_RAW)?;
            let state = proto::Registry::decode(raw.as_slice())
                .map_err(|_| Error::Invalid("registry protobuf"))?;
            // Refuse unknown/noncanonical data rather than silently rewriting and losing it.
            if !matches!(state.schema_version, 1 | 2) || state.encode_to_vec() != raw {
                return Err(Error::UnsupportedVersion);
            }
            let legacy = state.schema_version == 1;
            if legacy
                && state
                    .selections
                    .iter()
                    .any(|s| !s.approved_io_capabilities.is_empty())
            {
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
                if entry.approved_io_capabilities.len() > 10 {
                    return Err(Error::Limit);
                }
                let mut approved_io = BTreeSet::new();
                let mut previous_io = None;
                for raw in entry.approved_io_capabilities {
                    let kind = IoCapability::from_number(raw)?;
                    if previous_io.is_some_and(|previous| previous >= raw) {
                        return Err(Error::Invalid("IO approval ordering"));
                    }
                    previous_io = Some(raw);
                    approved_io.insert(kind);
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
                        approved_io,
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
            if legacy {
                // Full old-state validation precedes publication. Do not use commit's
                // semantic no-op shortcut: migration must durably advance the revision.
                result.publish(result.selections.clone(), result.dependencies.clone())?;
            }
        }
        Ok(result)
    }
    /// Immutable installation is separate from selection and approval.
    pub fn install_package(&self, archive: &[u8]) -> Result<[u8; 32]> {
        self.ensure_ready()?;
        let package = Package::decode(archive)?;
        self.observe(self.storage.install_package(&package))?;
        Ok(package.digest())
    }
    /// Native-compatible persisted envelope, for trusted export and verification.
    pub fn persisted_snapshot(&self) -> Result<Option<Vec<u8>>> {
        self.ensure_ready()?;
        self.storage.read()
    }
    pub fn installed_package(&self, digest: [u8; 32]) -> Result<Package> {
        self.ensure_ready()?;
        self.storage.load_package(digest)
    }
    /// An uncertain publication requires reopening; even a semantic no-op may
    /// otherwise falsely confirm the old in-memory state after a durable commit.
    pub fn ensure_ready(&self) -> Result<()> {
        if self.uncertain.get() {
            Err(Error::CommitUnknown)
        } else {
            Ok(())
        }
    }
    fn observe<T>(&self, result: Result<T>) -> Result<T> {
        if matches!(result, Err(Error::CommitUnknown)) {
            self.uncertain.set(true);
        }
        result
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
        let package = self.storage.load_package(selection.digest)?;
        if package.manifest().package_id != selection.package_id
            || !selection.approved.is_subset(package.capabilities())
            || !selection.approved_io.is_subset(package.io_capabilities())
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
        self.ensure_ready()?;
        let mut root_package = None;
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
            if current == id {
                // Reuse only within this traversal. Each new resolution still reads and
                // validates catalog bytes; no cross-call cache or file metadata shortcut.
                root_package = Some(package);
            }
        }
        let selection = self.selections.get(id).ok_or(Error::NotFound)?;
        Ok((root_package.ok_or(Error::NotFound)?, selection.clone()))
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
        self.ensure_ready()?;
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
        self.publish(next, dependencies)
    }
    fn publish(
        &mut self,
        next: BTreeMap<String, Selection>,
        dependencies: DependencyLocks,
    ) -> Result<()> {
        if next.len() > MAX_SELECTIONS || dependencies.len() > MAX_DEPENDENCY_LOCKS {
            return Err(Error::Limit);
        }
        let revision = self.revision.checked_add(1).ok_or(Error::Limit)?;
        let state = proto::Registry {
            schema_version: 2,
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
                    approved_io_capabilities: s
                        .approved_io
                        .iter()
                        .copied()
                        .map(IoCapability::number)
                        .collect(),
                })
                .collect(),
        };
        let bytes = envelope::pack(MAGIC, &state.encode_to_vec(), MAX_RAW)?;
        let publication = self.storage.publish(&bytes);
        self.observe(publication)?;
        self.selections = next;
        self.dependencies = dependencies;
        self.revision = revision;
        Ok(())
    }
    /// First selection is disabled with no approvals. Upgrade requires a newer SemVer;
    /// retained approvals are intersected with the new manifest, never expanded.
    pub fn select(&mut self, digest: [u8; 32], expected_revision: u64) -> Result<()> {
        self.check_revision(expected_revision)?;
        let package = self.storage.load_package(digest)?;
        let id = &package.manifest().package_id;
        let mut selection = Selection {
            package_id: id.clone(),
            digest,
            enabled: false,
            approved: BTreeSet::new(),
            approved_io: BTreeSet::new(),
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
            selection.approved_io = previous
                .approved_io
                .intersection(package.io_capabilities())
                .copied()
                .collect();
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
    /// Read-only approval preflight, including disabled selections. Managers use this
    /// before revoking live instances so malformed decisions have no runtime effect.
    pub fn validate_io_approval(
        &self,
        id: &str,
        expected_digest: [u8; 32],
        approved: &BTreeSet<IoCapability>,
        expected_revision: u64,
    ) -> Result<()> {
        self.check_revision(expected_revision)?;
        let selection = self.selections.get(id).ok_or(Error::NotFound)?;
        if selection.digest != expected_digest {
            return Err(Error::RevisionConflict);
        }
        // Revoking every IO category must remain possible if the selected package
        // is missing or damaged. Identity and revision are still checked above;
        // any nonempty decision needs the exact validated declaration below.
        if approved.is_empty() {
            return Ok(());
        }
        let package = self.load(selection)?;
        if !approved.is_subset(package.io_capabilities()) {
            return Err(Error::Invalid("IO approval exceeds declaration"));
        }
        Ok(())
    }
    /// IO approval uses an independent namespace and the same atomic selection snapshot.
    pub fn approve_io(
        &mut self,
        id: &str,
        expected_digest: [u8; 32],
        approved: BTreeSet<IoCapability>,
        expected_revision: u64,
    ) -> Result<()> {
        self.validate_io_approval(id, expected_digest, &approved, expected_revision)?;
        let mut next = self.selections.clone();
        next.get_mut(id).expect("existing selection").approved_io = approved;
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::plugin_package::catalog::Catalog;
    use std::fs;
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
        future.schema_version = 3;
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
            approved_io_capabilities: vec![],
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
