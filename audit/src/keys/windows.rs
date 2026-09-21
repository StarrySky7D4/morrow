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
fn crypt(input: &[u8], encrypt: bool, entropy: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    crypt_bounded(input, encrypt, entropy, 64 * 1024)
}
fn crypt_bounded(
    input: &[u8],
    encrypt: bool,
    entropy: &[u8],
    limit: usize,
) -> Result<Zeroizing<Vec<u8>>> {
    if input.is_empty() || input.len() > limit || entropy.is_empty() || entropy.len() > 256 {
        return Err(KeyError::Format);
    }
    let data = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(input.len()).map_err(|_| KeyError::Format)?,
        pbData: input.as_ptr().cast_mut(),
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr().cast_mut(),
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
    if output.0.pbData.is_null() || output.0.cbData as usize > limit {
        return Err(KeyError::Protection);
    }
    // SAFETY: successful DPAPI allocation remains owned until after this copy.
    Ok(Zeroizing::new(
        unsafe { std::slice::from_raw_parts(output.0.pbData, output.0.cbData as usize) }.to_vec(),
    ))
}
pub(super) fn protect(input: &[u8]) -> Result<Vec<u8>> {
    Ok(crypt(input, true, ENTROPY)?.to_vec())
}
pub(super) fn unprotect(input: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    crypt(input, false, ENTROPY)
}

const HTTP_ENTROPY: &[u8] = b"Morrow/http/credential/current-user/v1";
pub(super) fn protect_http(input: &[u8]) -> Result<Vec<u8>> {
    Ok(crypt(input, true, HTTP_ENTROPY)?.to_vec())
}
pub(super) fn unprotect_http(input: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    crypt(input, false, HTTP_ENTROPY)
}
const TLS_ENTROPY: &[u8] = b"Morrow/tls/identity/current-user/v1";
pub(super) fn protect_tls(input: &[u8]) -> Result<Vec<u8>> {
    Ok(crypt_bounded(
        input,
        true,
        TLS_ENTROPY,
        morrow_core::tls_identity::MAX_CIPHERTEXT_BYTES,
    )?
    .to_vec())
}
pub(super) fn unprotect_tls(input: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    crypt_bounded(
        input,
        false,
        TLS_ENTROPY,
        morrow_core::tls_identity::MAX_CIPHERTEXT_BYTES,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_legacy_entropy_still_decrypts_keys_and_http_domain_is_separate() {
        let plain = Zeroizing::new(b"synthetic-domain-check".to_vec());
        let legacy = crypt(&plain, true, b"Morrow/audit/key/current-user/v1").unwrap();
        assert!(unprotect(&legacy).unwrap().as_slice() == plain.as_slice());
        assert!(unprotect_http(&legacy).is_err());
        let current = protect(&plain).unwrap();
        assert!(
            crypt(&current, false, b"Morrow/audit/key/current-user/v1")
                .unwrap()
                .as_slice()
                == plain.as_slice()
        );
        let http = protect_http(&plain).unwrap();
        assert!(unprotect_http(&http).unwrap().as_slice() == plain.as_slice());
        assert!(unprotect(&http).is_err());
        let tls = protect_tls(&plain).unwrap();
        assert!(unprotect_tls(&tls).unwrap().as_slice() == plain.as_slice());
        assert!(unprotect(&tls).is_err());
        assert!(unprotect_http(&tls).is_err());
        assert!(unprotect_tls(&http).is_err());
        assert!(unprotect_tls(&legacy).is_err());
    }
    #[test]
    fn dpapi_input_and_entropy_bounds_fail_before_os_call() {
        assert!(crypt(&[], true, ENTROPY).is_err());
        assert!(crypt(&vec![1; 65537], true, ENTROPY).is_err());
        assert!(crypt(b"x", true, &[]).is_err());
        assert!(crypt(b"x", true, &vec![1; 257]).is_err());
    }
}
