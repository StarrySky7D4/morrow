//! Synthetic evidence only. This test key must never sign a user database.
use morrow_audit::*;
use morrow_core::{
    content::CardRecord,
    store::{EventBudget, Store},
};
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let destination =
        std::path::PathBuf::from(std::env::args().nth(1).ok_or("new evidence directory")?);
    std::fs::create_dir(&destination)?; // refuse an existing directory
    let key = SigningKey::from_bytes(&[7; 32]);
    let trust = TrustedLog {
        id: "synthetic-audit".into(),
        key: key.verifying_key(),
    };
    let mut store = Store::open_audited(
        &destination.join("synthetic.db"),
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
                "Synthetic evidence",
                vec![i],
            )?,
        )?;
    }
    let events = store.pending(0, 10)?;
    let one = sign(
        &from_pending(&trust, 1, [0; 32], &events[..2])?,
        &trust,
        &key,
    )?;
    let two = sign(
        &from_pending(&trust, 2, verify(&one, &trust)?.digest(), &events[2..])?,
        &trust,
        &key,
    )?;
    let archive_path = destination.join("archive.db");
    let mut archive = archive::Archive::open(&archive_path, trust.clone(), true)?;
    assert!(matches!(archive.append(&one)?, archive::Append::Stored(_)));
    assert!(matches!(
        archive.append(&one)?,
        archive::Append::AlreadyStored(_)
    ));
    assert!(matches!(archive.append(&two)?, archive::Append::Stored(_)));
    drop(archive);
    let mut truncated =
        archive::Archive::open(&destination.join("truncated.db"), trust.clone(), true)?;
    truncated.append(&one)?;
    drop(truncated);
    let archive = archive::Archive::open_read_only(&archive_path, trust.clone())?;
    let head = archive.check(None)?.ok_or("missing archive head")?;
    assert_eq!((head.index, head.last_sequence), (2, 3));
    for index in 1..=2 {
        let original = archive.segment(index)?.ok_or("missing archived segment")?;
        assert!(store.seal_pending(original.container())?);
        std::fs::write(
            destination.join(format!("{index:04}.maudit")),
            original.container(),
        )?;
    }
    std::fs::write(destination.join("independent-checkpoint.maudit"), &two)?;
    println!(
        "Archived and reopened 2 segments / 3 events; original bytes exported from the archive."
    );
    let public = trust
        .key
        .as_bytes()
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    std::fs::write(destination.join("test-public-key.txt"), &public)?;
    assert!(!store.seal_pending(&one)?);
    store.integrity_check()?;
    drop(store);
    let store = Store::open_read_only_audited(&destination.join("synthetic.db"), trust)?;
    assert!(store.pending(0, 10)?.is_empty());
    assert_eq!(store.sealed_segment(1)?.ok_or("missing original")?, one);
    println!(
        "Synthetic only; key={public}; log=synthetic-audit. Core reopened: 3 events sealed, pending queue empty, original operations retained."
    );
    Ok(())
}
