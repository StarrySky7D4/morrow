//! Synthetic test-only PEM pair; never used as an application identity default.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::path::PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("fixture directory required")?,
    );
    std::fs::create_dir_all(&dir)?;
    let certified =
        rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()])?;
    std::fs::write(dir.join("certificate.pem"), certified.cert.pem())?;
    std::fs::write(
        dir.join("private-key.pem"),
        certified.key_pair.serialize_pem(),
    )?;
    Ok(())
}
