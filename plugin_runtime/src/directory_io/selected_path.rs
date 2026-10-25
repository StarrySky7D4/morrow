//! Trusted native-selection spelling, resolved from an actual volume root.
//! This records no picker-time object identity and grants no guest path access.
use super::{DirectoryPathError, DirectoryRelativePath};
use std::{os::windows::ffi::OsStrExt, path::Path};

/// Only an absolute local drive path is admitted. UNC, device, verbatim,
/// relative, normalized-away and reserved components are rejected. Every raw
/// UTF-16 component is retained; no lossy UTF-8 conversion is involved.
#[derive(Clone, Debug)]
pub struct DirectoryAbsolutePath {
    drive: u8,
    relative: Option<DirectoryRelativePath>,
}

impl DirectoryAbsolutePath {
    pub fn new(path: &Path) -> Result<Self, DirectoryPathError> {
        // Bound before allocating a copy of caller-controlled spelling.
        let units = path.as_os_str().encode_wide();
        if units.clone().count() > 4096 {
            return Err(DirectoryPathError::Limit);
        }
        let raw: Vec<_> = units.collect();
        if raw.len() < 3 || !matches!(raw[0], 65..=90 | 97..=122) || raw[1..3] != [58, 92] {
            return Err(DirectoryPathError::InvalidComponent);
        }
        let relative = if raw.len() == 3 {
            None
        } else {
            let parts: Vec<_> = raw[3..].split(|unit| *unit == 92).collect();
            if parts.len() > super::selection_path::MAX_DIRECTORY_SEGMENTS {
                return Err(DirectoryPathError::Limit);
            }
            let mut components = Vec::with_capacity(parts.len());
            for part in parts {
                super::selection_path::validate_component(part)?;
                components.push(part.to_vec());
            }
            Some(DirectoryRelativePath::new(components)?)
        };
        Ok(Self {
            drive: raw[0] as u8,
            relative,
        })
    }

    pub(crate) fn retained_input_bytes(&self) -> usize {
        self.relative
            .as_ref()
            .map_or(0, DirectoryRelativePath::retained_input_bytes)
    }

    pub(super) fn into_parts(self) -> (u8, Option<DirectoryRelativePath>) {
        (self.drive, self.relative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};

    #[test]
    fn preserves_raw_units_and_never_normalizes_a_selected_path() {
        let path = Path::new(r"d:\root\child");
        let selected = DirectoryAbsolutePath::new(path).unwrap();
        assert_eq!(selected.drive, b'd');
        assert_eq!(
            selected.relative.unwrap().as_components(),
            &[vec![114, 111, 111, 116], vec![99, 104, 105, 108, 100]]
        );
        let raw = OsString::from_wide(&[67, 58, 92, 0xd800]);
        let selected = DirectoryAbsolutePath::new(Path::new(&raw)).unwrap();
        assert_eq!(selected.relative.unwrap().as_components(), &[vec![0xd800]]);
        let root = DirectoryAbsolutePath::new(Path::new(r"C:\")).unwrap();
        assert!(root.relative.is_none());
    }

    #[test]
    fn rejects_ambiguous_roots_and_every_unsafe_component_without_os_access() {
        for path in [
            r"\\server\share",
            r"\\?\C:\root",
            r"\\.\C:\root",
            r"C:root",
            r"\root",
            "C:/root",
            r"C:\root\..\child",
            r"C:\root\.\child",
            r"C:\root\\child",
            r"C:\root\",
            r"C:\root\con",
            "C:\\root\\child ",
            r"C:\root\child.",
            r"C:\root\file:stream",
            "C:\\root\0child",
        ] {
            assert!(
                DirectoryAbsolutePath::new(Path::new(path)).is_err(),
                "{path:?}"
            );
        }
        assert!(
            DirectoryAbsolutePath::new(Path::new(&format!("C:\\{}", "a".repeat(256)))).is_err()
        );
        assert!(
            DirectoryAbsolutePath::new(Path::new(&format!("C:\\{}", vec!["x"; 33].join("\\"))))
                .is_err()
        );
    }
}
