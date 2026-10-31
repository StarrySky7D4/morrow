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

    /// Display-only spelling for the original native confirmation. No parsing,
    /// normalization, filesystem access or authority is added by this copy.
    pub(super) fn picker_spelling(&self) -> Vec<u16> {
        let components = self.relative.as_ref().map(|p| p.as_components());
        let count = components.map_or(3, |parts| {
            3 + parts.iter().map(Vec::len).sum::<usize>() + parts.len() - 1
        });
        let mut spelling = Vec::with_capacity(count + 1);
        spelling.extend_from_slice(&[self.drive as u16, 58, 92]);
        if let Some(parts) = components {
            for (n, part) in parts.iter().enumerate() {
                if n != 0 {
                    spelling.push(92);
                }
                spelling.extend_from_slice(part);
            }
        }
        spelling.push(0);
        spelling
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
    fn native_confirmation_prefill_preserves_exact_utf16_and_one_terminator() {
        for raw in [
            vec![100, 58, 92],
            vec![100, 58, 92, 0xd800, 92, 0x76ee, 0x5f55, 0xd83d, 0xde00],
        ] {
            let os = OsString::from_wide(&raw);
            let selected = DirectoryAbsolutePath::new(Path::new(&os)).unwrap();
            let spelling = selected.picker_spelling();
            assert_eq!(&spelling[..spelling.len() - 1], raw);
            assert_eq!(spelling.last(), Some(&0));
            assert_eq!(spelling.iter().filter(|v| **v == 0).count(), 1);
        }
    }

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
