use morrow_agent_catalog_admin_v1::{MAX_PATH_BYTES, Status};
use morrow_agent_session_process_v1_host::MAX_ARCHIVE_BYTES;
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::Read,
    path::{Component, Path},
};

fn plain(metadata: &Metadata, directory: bool) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    if directory {
        metadata.file_type().is_dir()
    } else {
        metadata.file_type().is_file()
    }
}

fn io_error(error: std::io::Error) -> Status {
    match error.kind() {
        std::io::ErrorKind::NotFound => Status::NotFound,
        std::io::ErrorKind::PermissionDenied => Status::Denied,
        _ => Status::Storage,
    }
}

/// A trusted, local file selection. A returned digest identifies the bytes read;
/// it does not claim an OS sandbox or freeze a directory object across calls.
pub(super) fn read(path: &str) -> Result<Vec<u8>, Status> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES || path.chars().any(char::is_control) {
        return Err(Status::Invalid);
    }
    let selected = Path::new(path);
    if !selected.is_absolute() {
        return Err(Status::Invalid);
    }
    #[cfg(windows)]
    {
        use std::path::Prefix;
        // Deny UNC/device/network namespaces and drive-relative paths. The UI
        // supplies the ordinary absolute path selected by its native picker.
        if !matches!(selected.components().next(), Some(Component::Prefix(value))
            if matches!(value.kind(), Prefix::Disk(_)))
        {
            return Err(Status::Denied);
        }
    }
    if path
        .split(['/', '\\'])
        .any(|part| matches!(part, "." | ".."))
        || selected.components().any(|part| {
            matches!(part, Component::ParentDir | Component::CurDir)
                || matches!(part, Component::Normal(value) if value.to_string_lossy().contains(':'))
        })
    {
        return Err(Status::Invalid);
    }
    let path = selected;
    #[cfg(not(windows))]
    if path.to_string_lossy().contains('\\') || path.to_string_lossy().starts_with("//") {
        return Err(Status::Denied);
    }
    for ancestor in path.ancestors().skip(1) {
        if !plain(&fs::symlink_metadata(ancestor).map_err(io_error)?, true) {
            return Err(Status::Denied);
        }
    }
    let before = fs::symlink_metadata(path).map_err(io_error)?;
    if !plain(&before, false) {
        return Err(Status::Denied);
    }
    if before.len() > MAX_ARCHIVE_BYTES as u64 {
        return Err(Status::Limit);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Inspect the final reparse point itself; disallow concurrent writes or
        // deletion for the read handle. No native unsafe boundary is introduced.
        options.share_mode(1).custom_flags(0x00200000);
    }
    let mut file: File = options.open(path).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !plain(&metadata, false) {
        return Err(Status::Denied);
    }
    if metadata.len() > MAX_ARCHIVE_BYTES as u64 {
        return Err(Status::Limit);
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_ARCHIVE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > MAX_ARCHIVE_BYTES || bytes.len() as u64 != metadata.len() {
        return Err(Status::Limit);
    }
    Ok(bytes)
}
