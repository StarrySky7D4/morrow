#[cfg(not(target_arch = "wasm32"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use morrow_core::{
        content::CardRecord,
        envelope,
        lifecycle::HostPolicy,
        runtime::RenameRequest,
        store::{EventBudget, Store},
        transaction::Lookup,
    };
    use std::{
        io::{Read, Write},
        path::Path,
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    let budget = EventBudget::default();
    match args.as_slice() {
        [command, db] if command == "init" => {
            Store::open(Path::new(db), budget)?.integrity_check()?;
            println!("Ready: experimental transaction database");
        }
        [command, db, op, id, title] if command == "create-local" => {
            let mut store = Store::open_existing(Path::new(db), budget)?;
            let card = CardRecord::new(id, "morrow.text", 1, title, vec![])?;
            let receipt = store.create_local(op, &card)?;
            println!("LocallyCommitted revision={} event={}", receipt.revision, receipt.event_id);
        }
        [command, db, op, path] if command == "import-container-local" => {
            let mut bytes = Vec::new();
            std::fs::File::open(path)?.take(envelope::MAX_CONTAINER_BYTES as u64 + 1).read_to_end(&mut bytes)?;
            let card = envelope::decode(&bytes)?;
            let mut store = Store::open_existing(Path::new(db), budget)?;
            let receipt = store.create_local(op, &card)?;
            println!("LocallyCommitted revision={} event={}", receipt.revision, receipt.event_id);
        }
        [command, db, op, id, revision, title] if command == "rename-local" => {
            let mut store = Store::open_existing(Path::new(db), budget)?;
            let mut host = HostPolicy::new()?;
            let instance = host.activate()?; host.ready(instance)?;
            let start = std::time::Instant::now();
            let clock = || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            let grant = host.grant_rename(instance, id, 30_000, clock())?;
            let request = RenameRequest { operation_id: op.clone(), card_id: id.clone(), expected_revision: revision.parse()?, title: title.clone() };
            let permit = host.begin(instance, grant, &request, clock())?;
            let receipt = host.commit_rename(permit, &mut store, clock)?;
            println!("LocallyCommitted revision={} event={}", receipt.revision, receipt.event_id);
        }
        [command, db, op] if command == "query" => {
            let store = Store::open_existing(Path::new(db), budget)?;
            match store.lookup(op)? {
                Lookup::Committed(receipt) => println!("LocallyCommitted revision={} event={}", receipt.revision, receipt.event_id),
                Lookup::Absent => println!("Absent at query snapshot; does not resolve a concurrent in-flight operation"),
            }
        }
        [command, db, id, destination] if command == "export" => {
            let store = Store::open_existing(Path::new(db), budget)?;
            let card = store.card(id)?.ok_or("card not found")?;
            let bytes = envelope::encode(&card)?;
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(destination)?;
            file.write_all(&bytes)?; file.sync_all()?;
            println!("Exported card container");
        }
        [command, db] if command == "check" => {
            Store::open_existing(Path::new(db), budget)?.integrity_check()?;
            println!("PASS: SQLite integrity, content digest and atomic operation/outbox associations");
        }
        _ => return Err("usage: morrow-core-store init <db> | create-local <db> <op> <card> <title> | import-container-local <db> <op> <container> | rename-local <db> <op> <card> <revision> <title> | query <db> <op> | export <db> <card> <new-file> | check <db>".into()),
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    #[cfg(not(target_arch = "wasm32"))]
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::process::ExitCode::FAILURE
    }
}
