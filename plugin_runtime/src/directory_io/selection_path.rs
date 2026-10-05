//! Trusted-host relative directory selection spelling. This is not a guest path,
//! filesystem grant, picker proof or validation of ancestors above an anchor.
//! Raw UTF-16 units are retained exactly; isolated surrogate units are allowed.
use std::fmt;

pub const MAX_DIRECTORY_SEGMENTS: usize = 32;
pub const MAX_DIRECTORY_UTF16_BYTES: usize = 8192;
const MAX_COMPONENT_UNITS: usize = 255;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryPathError {
    InvalidComponent,
    Limit,
}
impl fmt::Display for DirectoryPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidComponent => f.write_str("invalid relative directory component"),
            Self::Limit => f.write_str("relative directory path limit"),
        }
    }
}
impl std::error::Error for DirectoryPathError {}

/// A bounded exact spelling, never an approval or durable filesystem identity.
#[derive(Clone, PartialEq, Eq)]
pub struct DirectoryRelativePath {
    components: Vec<Vec<u16>>,
    utf16_bytes: usize,
}
impl fmt::Debug for DirectoryRelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DirectoryRelativePath")
            .field("component_count", &self.components.len())
            .field("utf16_bytes", &self.utf16_bytes)
            .finish_non_exhaustive()
    }
}
impl DirectoryRelativePath {
    /// Takes already-owned trusted-host input. Does not resolve, decode,
    /// normalize, case-fold, allocate new component storage or inspect the OS.
    pub fn new(components: Vec<Vec<u16>>) -> Result<Self, DirectoryPathError> {
        if components.is_empty() {
            return Err(DirectoryPathError::InvalidComponent);
        }
        if components.len() > MAX_DIRECTORY_SEGMENTS
            || components.capacity() > MAX_DIRECTORY_SEGMENTS
        {
            return Err(DirectoryPathError::Limit);
        }
        let mut utf16_bytes = 0usize;
        let mut capacity_bytes = 0usize;
        for component in &components {
            validate_component(component)?;
            if component.capacity() > MAX_COMPONENT_UNITS {
                return Err(DirectoryPathError::Limit);
            }
            capacity_bytes = capacity_bytes
                .checked_add(
                    component
                        .capacity()
                        .checked_mul(2)
                        .ok_or(DirectoryPathError::Limit)?,
                )
                .ok_or(DirectoryPathError::Limit)?;
            if capacity_bytes > MAX_DIRECTORY_UTF16_BYTES {
                return Err(DirectoryPathError::Limit);
            }
            utf16_bytes = utf16_bytes
                .checked_add(
                    component
                        .len()
                        .checked_mul(2)
                        .ok_or(DirectoryPathError::Limit)?,
                )
                .ok_or(DirectoryPathError::Limit)?;
            if utf16_bytes > MAX_DIRECTORY_UTF16_BYTES {
                return Err(DirectoryPathError::Limit);
            }
        }
        components
            .capacity()
            .checked_mul(std::mem::size_of::<Vec<u16>>())
            .and_then(|outer| outer.checked_add(capacity_bytes))
            .ok_or(DirectoryPathError::Limit)?;
        Ok(Self {
            components,
            utf16_bytes,
        })
    }

    pub fn as_components(&self) -> &[Vec<u16>] {
        &self.components
    }
    pub fn component_count(&self) -> usize {
        self.components.len()
    }
    /// Sum of component UTF-16 storage bytes, excluding queue/container allowance.
    pub fn utf16_bytes(&self) -> usize {
        self.utf16_bytes
    }
    /// Actual retained allocation allowance, including spare capacity and the
    /// component-vector container. Excludes this fixed struct/queue framing.
    pub(crate) fn retained_input_bytes(&self) -> usize {
        // Clone may reduce spare capacity. Read the currently retained vectors,
        // rather than cache the allocation allowance of the original object.
        let names = self.components.iter().try_fold(0usize, |sum, component| {
            sum.checked_add(component.capacity().checked_mul(2)?)
        });
        self.components
            .capacity()
            .checked_mul(std::mem::size_of::<Vec<u16>>())
            .and_then(|outer| outer.checked_add(names?))
            .expect("validated directory path capacity")
    }
}

/// Shared by the safe path owner and the private native open boundary. This
/// protects every direct native call too, without trusting prior construction.
pub(crate) fn validate_component(component: &[u16]) -> Result<(), DirectoryPathError> {
    if component.len() > MAX_COMPONENT_UNITS {
        return Err(DirectoryPathError::Limit);
    }
    if component.is_empty()
        || component == [46]
        || component == [46, 46]
        || matches!(component.last(), Some(32 | 46))
        || component.iter().any(|unit| {
            matches!(*unit, 0..=31 | 127..=159 | 47 | 92 | 58 | 60 | 62 | 34 | 124 | 63 | 42)
        })
        || windows_device(component)
    {
        return Err(DirectoryPathError::InvalidComponent);
    }
    Ok(())
}

fn windows_device(component: &[u16]) -> bool {
    // Windows device stems are reserved before the first dot, including ASCII
    // spaces immediately before it. Fold ASCII only for this lexical rejection;
    // never alter the spelling sent to NtCreateFile.
    let mut stem = component.split(|unit| *unit == 46).next().unwrap_or(&[]);
    while stem.last() == Some(&32) {
        stem = &stem[..stem.len() - 1];
    }
    let upper = |unit: u16| {
        if (97..=122).contains(&unit) {
            unit - 32
        } else {
            unit
        }
    };
    let matches_ascii = |value: &[u8]| {
        stem.len() == value.len()
            && stem
                .iter()
                .zip(value)
                .all(|(unit, byte)| upper(*unit) == u16::from(*byte))
    };
    if [
        b"CON".as_slice(),
        b"PRN",
        b"AUX",
        b"NUL",
        b"CONIN$",
        b"CONOUT$",
        b"CLOCK$",
    ]
    .iter()
    .any(|name| matches_ascii(name))
    {
        return true;
    }
    stem.len() == 4
        && ((upper(stem[0]) == 67 && upper(stem[1]) == 79 && upper(stem[2]) == 77)
            || (upper(stem[0]) == 76 && upper(stem[1]) == 80 && upper(stem[2]) == 84))
        && matches!(stem[3], 49..=57 | 0x00b9 | 0x00b2 | 0x00b3)
}

#[cfg(test)]
#[path = "selection_path_tests.rs"]
mod tests;
