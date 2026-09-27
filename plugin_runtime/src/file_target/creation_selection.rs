//! Bound creation destination. Selection/validation never opens or changes the leaf.
use super::*;

/// An opaque, session-local destination reference, not durable filesystem authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedCreateTarget {
    pub reference: [u8; 32],
}

pub(super) struct CreateEntry {
    // Drop files before refunding reservations. Every ancestor remains pinned.
    pub(super) handles: Vec<File>,
    leases: Vec<IoResourceLease>,
    pub(super) relative: RelativeFilePath,
    pub(super) scope: SelectionScope,
    pub(super) package_sha256: [u8; 32],
}
impl CreateEntry {
    pub(super) fn lease(&self) -> &IoResourceLease {
        &self.leases[0] // construction always reserves and opens the root
    }
    pub(super) fn reclaimable(&self, now: u64) -> bool {
        self.lease().reclaimable(now)
    }
}
fn directory(file: &File) -> Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
        return Err(Error::InvalidSelection);
    }
    Ok(())
}
fn root_path(path: &Path) -> Result<()> {
    let raw = path.to_str().ok_or(Error::InvalidSelection)?;
    // Also admit an exact drive root. validate_path handles all other original
    // spellings, including device/UNC/ADS/dot rejection; it never canonicalizes.
    if (raw.len() == 3 || (raw.starts_with(r"\\?\") && raw.len() == 7))
        && matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
        && matches!(path.components().nth(1), Some(Component::RootDir))
    {
        return Ok(());
    }
    let raw = raw.strip_suffix(['\\', '/']).unwrap_or(raw);
    validate_path(Path::new(raw))
}
impl TargetBroker {
    /// Compatibility wrapper for callers without a shared cancellation gate.
    #[allow(clippy::too_many_arguments)]
    pub fn select_create(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        root: &Path,
        relative: &RelativeFilePath,
        scope: SelectionScope,
        clock: impl FnMut() -> u64,
    ) -> Result<SelectedCreateTarget> {
        let mut control = LocalControl(clock);
        self.select_create_controlled(
            manager,
            host,
            instance,
            binding,
            root,
            relative,
            scope,
            &mut control,
        )
    }

    /// Select a directory-bound creation destination under a shared live gate.
    /// The trusted picker supplies the root; only the relative spelling is guest
    /// input. Parent handles are retained, while the leaf is never opened here.
    #[allow(clippy::too_many_arguments)]
    pub fn select_create_controlled(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        root: &Path,
        relative: &RelativeFilePath,
        scope: SelectionScope,
        control: &mut impl TargetControl,
    ) -> Result<SelectedCreateTarget> {
        if binding.is_mutation_history() {
            return Err(Error::Admission(io_binding::Error::Denied));
        }
        let capability = Disposition::Create.capability();
        control.with(|now, cancelled| {
            binding.preflight_capability(manager, host, instance, capability, now)?;
            running(cancelled)
        })?;
        if !native_windows::available() {
            return Err(Error::RestartRequired);
        }
        if scope.disposition != Disposition::Create
            || scope.subject.is_empty()
            || scope.subject.len() > 256
            || scope
                .subject
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
            || scope.approval_sha256 == [0; 32]
        {
            return Err(Error::InvalidSelection);
        }
        root_path(root)?;
        let segments: Vec<_> = relative.as_str().split('/').collect();
        // root plus the parents of the final leaf = the number of segments.
        let now = controlled(control, Ok)?;
        self.reap(now);
        if self.retained_slots().saturating_add(segments.len()) > MAX_TARGETS {
            return Err(Error::Limit);
        }
        let next = self.next.checked_add(1).ok_or(Error::Limit)?;
        let mut hash = Sha256::new();
        hash.update(b"morrow.selected-create-target.v1\0");
        hash.update(self.secret);
        hash.update(self.identity.to_le_bytes());
        hash.update(next.to_le_bytes());
        let reference = hash.finalize().into();
        // Reserve the entire chain before OS access. Partial admission failure
        // drops every reservation and opens no directory.
        let mut leases = Vec::with_capacity(segments.len());
        for _ in &segments {
            leases.push(controlled(control, |now| {
                Ok(binding.admit_resource(manager, host, instance, &[capability], now)?)
            })?);
        }
        let mut handles = Vec::with_capacity(segments.len());
        controlled(control, |now| {
            leases[0].check(manager, host, instance, now)?;
            Ok(())
        })?;
        // OS opens and metadata checks run outside the control's clock lock.
        // FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE; read sharing only.
        // No DELETE sharing pins ancestry; no WRITE sharing prevents ordinary
        // reparse mutation while retained. This does not lock child contents.
        let file = OpenOptions::new()
            .access_mode(0x0010_00a0)
            .share_mode(1)
            .custom_flags(0x0220_0000) // BACKUP_SEMANTICS | OPEN_REPARSE_POINT
            .open(root)?;
        directory(&file)?;
        handles.push(file);
        controlled(control, |now| {
            leases[0].check(manager, host, instance, now)?;
            Ok(())
        })?;
        for (index, segment) in segments[..segments.len() - 1].iter().enumerate() {
            controlled(control, |now| {
                leases[index + 1].check(manager, host, instance, now)?;
                Ok(())
            })?;
            let file = native_windows::open_child_directory(&handles[index], segment)?;
            directory(&file)?;
            handles.push(file);
            controlled(control, |now| {
                leases[index + 1].check(manager, host, instance, now)?;
                Ok(())
            })?;
        }
        self.creates.insert(
            reference,
            CreateEntry {
                handles,
                leases,
                relative: relative.clone(),
                scope,
                package_sha256: instance.package().package().digest(),
            },
        );
        self.next = next;
        Ok(SelectedCreateTarget { reference })
    }

    pub(super) fn validate_create_controlled(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        control: &mut impl TargetControl,
    ) -> Result<()> {
        let request = request.request();
        let entry = self
            .creates
            .get(&request.target.reference)
            .ok_or(Error::Missing)?;
        // Keep the original-owner preflight ahead of cancellation so a foreign
        // caller cannot learn the live cancellation state of another selection.
        control.with(|now, cancelled| {
            entry.lease().preflight(manager, host, instance, now)?;
            running(cancelled)
        })?;
        if request.subject != entry.scope.subject
            || request.approval_sha256 != entry.scope.approval_sha256
            || request.package_sha256 != entry.package_sha256
            || request.disposition != Disposition::Create
            || request.target.relative_path.as_ref() != Some(&entry.relative)
            || request.expected_identity.is_some()
        {
            return Err(Error::Mismatch);
        }
        controlled(control, |now| {
            entry.lease().check(manager, host, instance, now)?;
            Ok(())
        })?;
        for file in &entry.handles {
            directory(file)?;
            controlled(control, |now| {
                entry.lease().check(manager, host, instance, now)?;
                Ok(())
            })?;
        }
        Ok(())
    }
}
