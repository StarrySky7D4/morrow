//! Retain the selected file and every ancestor while renaming within its original directory.
use super::*;

pub(super) struct RenameFile {
    pub(super) file: File,
    pub(super) destination: RelativeFilePath,
    // Keep each verified directory alive without write/delete sharing until the native call returns.
    pub(super) parents: Vec<File>,
    // Refund only after all retained handles have closed.
    pub(super) lease: IoResourceLease,
}

pub(super) fn resources(path: &Path) -> Result<u32> {
    // A drive prefix and root together require one handle, each normal component one more.
    u32::try_from(
        path.components()
            .count()
            .checked_sub(1)
            .ok_or(Error::InvalidSelection)?,
    )
    .map_err(|_| Error::Limit)
}

impl RenameFile {
    pub(super) fn select(
        path: &Path,
        destination: &RelativeFilePath,
        lease: IoResourceLease,
    ) -> Result<Self> {
        validate_path(path)?;
        let root = path.ancestors().last().ok_or(Error::InvalidSelection)?;
        let handle = OpenOptions::new()
            .access_mode(0x0010_00a0)
            .share_mode(1)
            .custom_flags(0x0220_0000)
            .open(root)?;
        creation_selection::directory(&handle)?;
        let mut parents = vec![handle];
        let relative = path
            .strip_prefix(root)
            .map_err(|_| Error::InvalidSelection)?;
        let mut parts = relative.components().peekable();
        while let Some(Component::Normal(part)) = parts.next() {
            let leaf = part.to_str().ok_or(Error::InvalidSelection)?;
            let parent = parents.last().ok_or(Error::InvalidSelection)?;
            if parts.peek().is_none() {
                let file = native_windows::open_rename_source(parent, leaf)?;
                Stamp::read(file.metadata()?)?;
                return Ok(Self {
                    file,
                    destination: destination.clone(),
                    lease,
                    parents,
                });
            }
            let child = native_windows::open_child_directory(parent, leaf)?;
            creation_selection::directory(&child)?;
            parents.push(child);
        }
        Err(Error::InvalidSelection)
    }
}
