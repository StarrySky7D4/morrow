//! Explicit fresh synthetic data only; no user database and no fixed private key.
#[cfg(target_os = "windows")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use morrow_audit::keys::Key;
    use morrow_core::{
        content::CardRecord,
        store::{EventBudget, Store},
    };
    let root = std::path::PathBuf::from(std::env::args().nth(1).ok_or("fresh evidence directory")?);
    std::fs::create_dir(&root)?;
    let key = Key::create(&root.join("protected.morrowkey"))?;
    let trust = key.trust();
    let mut store = Store::open_audited(
        &root.join("synthetic.db"),
        EventBudget::default(),
        true,
        trust.clone(),
    )?;
    for i in 0..3 {
        store.create_local(
            &format!("create-{i}"),
            &CardRecord::new(
                &format!("card-{i}"),
                "text",
                1,
                "Protected-key synthetic",
                vec![i],
            )?,
        )?;
    }
    drop(store);
    std::fs::write(
        root.join("public-key.txt"),
        trust
            .key
            .as_bytes()
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>(),
    )?;
    std::fs::write(root.join("log-id.txt"), trust.id)?;
    println!(
        "Prepared fresh synthetic database and random protected key. Private material was not printed."
    );
    Ok(())
}
#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("Windows qualification only");
    std::process::exit(1);
}
