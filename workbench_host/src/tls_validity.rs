//! Bounded parsing of the supplied chain's intersected time window. This is
//! neither chain/signature validation nor a trust-anchor decision.
use crate::Result;
use rustls_pki_types::{CertificateDer, pem::PemObject};
use x509_parser::{certificate::X509Certificate, prelude::FromDer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TlsValidity {
    /// Inclusive Unix seconds, intersected over all supplied certificates.
    pub not_before: i64,
    pub not_after: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pem(before: i64, after: i64) -> String {
        let mut params = rcgen::CertificateParams::new(vec!["localhost".into()]).unwrap();
        params.not_before = time::OffsetDateTime::from_unix_timestamp(before).unwrap();
        params.not_after = time::OffsetDateTime::from_unix_timestamp(after).unwrap();
        params
            .self_signed(&rcgen::KeyPair::generate().unwrap())
            .unwrap()
            .pem()
    }
    #[test]
    fn intersects_every_certificate_and_preserves_inclusive_seconds() {
        let chain = format!("{}{}", pem(-1000, 1000), pem(10, 800));
        let validity = TlsValidity::from_pem(chain.as_bytes()).unwrap();
        assert_eq!(
            validity,
            TlsValidity {
                not_before: 10,
                not_after: 800
            }
        );
        assert_eq!(validity.authority_window().unwrap(), (10_000, 801_000));
        assert_eq!(
            TlsValidity::from_pem(pem(-1000, 0).as_bytes())
                .unwrap()
                .authority_window()
                .unwrap(),
            (0, 1000)
        );
        assert_eq!(
            TlsValidity::from_pem(pem(10, 10).as_bytes())
                .unwrap()
                .authority_window()
                .unwrap(),
            (10_000, 11_000)
        );
    }
    #[test]
    fn rejects_empty_disjoint_inverted_oversized_and_excess_chains() {
        for bytes in [
            vec![],
            b"not a certificate".to_vec(),
            vec![0; 65_537],
            pem(100, 99).into_bytes(),
            format!("{}{}", pem(0, 10), pem(11, 20)).into_bytes(),
            pem(0, 100).repeat(17).into_bytes(),
        ] {
            assert!(TlsValidity::from_pem(&bytes).is_err());
        }
        assert!(TlsValidity::from_pem(pem(0, 100).repeat(16).as_bytes()).is_ok());
    }
}
impl TlsValidity {
    pub fn from_pem(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() || bytes.len() > 65_536 {
            return Err("invalid TLS certificate validity".into());
        }
        let mut window = Self {
            not_before: i64::MIN,
            not_after: i64::MAX,
        };
        let mut count = 0;
        for der in CertificateDer::pem_slice_iter(bytes) {
            if count == 16 {
                return Err("invalid TLS certificate validity".into());
            }
            let der = der.map_err(|_| "invalid TLS certificate validity")?;
            let (remaining, cert) = X509Certificate::from_der(der.as_ref())
                .map_err(|_| "invalid TLS certificate validity")?;
            let validity = cert.validity();
            let before = validity.not_before.timestamp();
            let after = validity.not_after.timestamp();
            if !remaining.is_empty()
                || before > after
                || before < -62_135_596_800
                || after > 253_402_300_799
            {
                return Err("invalid TLS certificate validity".into());
            }
            window.not_before = window.not_before.max(before);
            window.not_after = window.not_after.min(after);
            count += 1;
        }
        if count == 0 || window.not_before > window.not_after {
            return Err("invalid TLS certificate validity".into());
        }
        Ok(window)
    }

    /// Same whole-second inclusive comparison as TLS clients. The host's
    /// authorization window is half-open milliseconds, so include notAfter's
    /// second by expiring at the next second. Pre-epoch expiry cannot run now.
    pub fn authority_window(self) -> Result<(u64, u64)> {
        let before = u64::try_from(self.not_before.max(0))?
            .checked_mul(1000)
            .ok_or("invalid TLS certificate validity")?;
        let after = u64::try_from(
            self.not_after
                .checked_add(1)
                .ok_or("invalid TLS certificate validity")?,
        )?
        .checked_mul(1000)
        .ok_or("invalid TLS certificate validity")?;
        if before >= after {
            return Err("invalid TLS certificate validity".into());
        }
        Ok((before, after))
    }
}
