#[cfg(target_os = "windows")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use morrow_audit::{keys::Key, sealer::Sealer};
    use morrow_core::store::{EventBudget, Store};
    use std::path::Path;
    let mut args = std::env::args().skip(1);
    let first = args
        .next()
        .ok_or("usage: morrow-audit-seal --initialize DATABASE | --open-bound DATABASE | --new-key NEW_FILE | KEY_FILE EXISTING_CORE_DB")?;
    if first == "--initialize" || first == "--open-bound" {
        use morrow_audit::session::{OpenMode, Session};
        let database = args.next().ok_or("database path")?;
        if args.next().is_some() {
            return Err("unexpected argument".into());
        }
        let mode = if first == "--initialize" {
            OpenMode::Initialize
        } else {
            OpenMode::Existing
        };
        let mut session = Session::open(Path::new(&database), EventBudget::default(), mode)?;
        let trust = session.trust();
        let result = session.flush(16)?;
        println!(
            "Bound log={}; public_key={}; sealed={} events; more_pending={}",
            trust.id,
            trust
                .key
                .as_bytes()
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>(),
            result.events,
            result.more_pending
        );
    } else if first == "--new-key" {
        let path = args.next().ok_or("new key file")?;
        if args.next().is_some() {
            return Err("unexpected argument".into());
        }
        let key = Key::create(Path::new(&path))?;
        let trust = key.trust();
        println!(
            "Created protected key; log={}; public_key={}",
            trust.id,
            trust
                .key
                .as_bytes()
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>()
        );
    } else {
        let database = args.next().ok_or("existing core database")?;
        if args.next().is_some() {
            return Err("unexpected argument".into());
        }
        let sealer = Sealer::new(Key::load(Path::new(&first))?)?;
        let mut store = Store::open_audited(
            Path::new(&database),
            EventBudget::default(),
            false,
            sealer.trust(),
        )?;
        let result = sealer.flush(&mut store, 16)?;
        println!(
            "Sealed {} segments / {} events; more_pending={}",
            result.segments, result.events, result.more_pending
        );
    }
    Ok(())
}
#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("Protected key provider unavailable on this platform");
    std::process::exit(1);
}
