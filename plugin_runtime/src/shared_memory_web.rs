//! Worker-owned immutable bytes. Guests receive bounded copies into their own
//! linear memory; no native mapping handle or shared writable memory is exposed.
use std::io;
pub const MAX_REGION_BYTES: usize = 16 * 1024 * 1024;
#[derive(Debug)]
pub struct FrozenRegion(Box<[u8]>);
impl FrozenRegion {
    pub fn copy_from(bytes: &[u8]) -> io::Result<Self> {
        if !(1..=MAX_REGION_BYTES).contains(&bytes.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "shared region must contain 1..=16 MiB",
            ));
        }
        let mut copy = Vec::new();
        copy.try_reserve_exact(bytes.len())
            .map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "shared region allocation"))?;
        copy.extend_from_slice(bytes);
        Ok(Self(copy.into_boxed_slice()))
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
