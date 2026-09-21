//! Synthetic test-only PEM pair; never used as an application identity default.
use rcgen::{CertificateParams, KeyPair};
use std::{env, fs, path::PathBuf};
use time::OffsetDateTime;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.len() == 2 || args.len() > 3 {
        return Err("usage: <dir> [<not_before> <not_after>]".into());
    }
    let dir = PathBuf::from(&args[0]);
    let bounds = if args.len() == 3 {
        let before: i64 = args[1].parse()?;
        let after: i64 = args[2].parse()?;
        if before > after {
            return Err("notBefore must be <= notAfter".into());
        }
        Some((before, after))
    } else {
        None
    };
    let mut params = CertificateParams::new(vec!["localhost".into(), "127.0.0.1".into()])?;
    if let Some((before, after)) = bounds {
        params.not_before = OffsetDateTime::from_unix_timestamp(before)?;
        params.not_after = OffsetDateTime::from_unix_timestamp(after)?;
    }
    let key = KeyPair::generate()?;
    let cert = params.self_signed(&key)?;
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("certificate.pem"), cert.pem())?;
    fs::write(dir.join("private-key.pem"), key.serialize_pem())?;
    Ok(())
}
