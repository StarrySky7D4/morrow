//! The only unsafe boundary: OS-owned DPAPI buffers and bounded caller-owned RNG buffers.
use super::{KeyError, Result};
use windows_sys::Win32::{Foundation::LocalFree, Security::Cryptography::*};
use zeroize::{Zeroize, Zeroizing};
const ENTROPY: &[u8] = b"Morrow/audit/key/current-user/v1";
pub(super) fn random(bytes: &mut [u8]) -> Result<()> {
    let len = u32::try_from(bytes.len()).map_err(|_| KeyError::Random)?;
    // SAFETY: writable live slice of len bytes; null provider is required for the system RNG.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            len,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        Err(KeyError::Random)
    } else {
        Ok(())
    }
}
struct OwnedBlob(CRYPT_INTEGER_BLOB);
impl Drop for OwnedBlob {
    fn drop(&mut self) {
        if !self.0.pbData.is_null() {
            // SAFETY: successful DPAPI call allocated cbData bytes with LocalAlloc.
            unsafe {
                std::slice::from_raw_parts_mut(self.0.pbData, self.0.cbData as usize).zeroize();
                LocalFree(self.0.pbData.cast());
            }
        }
    }
}
fn crypt(input: &[u8], encrypt: bool) -> Result<Zeroizing<Vec<u8>>> {
    let data = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(input.len()).map_err(|_| KeyError::Format)?,
        pbData: input.as_ptr().cast_mut(),
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: input/entropy live for the call and are input-only per DPAPI contract.
    // Null reserved/prompt/description outputs are optional; no interactive prompt.
    let success = unsafe {
        if encrypt {
            CryptProtectData(
                &data,
                std::ptr::null(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &data,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if success == 0 {
        return Err(KeyError::Protection);
    }
    let output = OwnedBlob(output);
    if output.0.pbData.is_null() || output.0.cbData > 64 * 1024 {
        return Err(KeyError::Protection);
    }
    // SAFETY: successful DPAPI allocation remains owned until after this copy.
    Ok(Zeroizing::new(
        unsafe { std::slice::from_raw_parts(output.0.pbData, output.0.cbData as usize) }.to_vec(),
    ))
}
pub(super) fn protect(input: &[u8]) -> Result<Vec<u8>> {
    Ok(crypt(input, true)?.to_vec())
}
pub(super) fn unprotect(input: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    crypt(input, false)
}
