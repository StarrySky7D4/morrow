#[cfg(target_os = "windows")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use morrow_audit::session::{OpenMode, Session};
    use morrow_core::{content::CardRecord, store::EventBudget};
    let root = std::path::PathBuf::from(std::env::args().nth(1).ok_or("fresh evidence directory")?);
    std::fs::create_dir(&root)?;
    let mut session = Session::open(
        &root.join("synthetic.db"),
        EventBudget::default(),
        OpenMode::Initialize,
    )?;
    for i in 0..3 {
        session.runtime().store_local_mut().create_local(
            &format!("create-{i}"),
            &CardRecord::new(&format!("card-{i}"), "text", 1, "Bound synthetic", vec![i])?,
        )?;
    }
    let trust = session.trust();
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
    println!("Prepared synthetic database with durable identity binding and 3 pending events.");
    Ok(())
}
#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("Windows qualification only");
    std::process::exit(1);
}
