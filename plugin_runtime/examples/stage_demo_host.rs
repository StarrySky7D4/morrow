//! Stage demo only: a private process/session, fixed binary pipe, no user database.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::catalog,
    store::{EventBudget, Store},
    task::{Invocation, Transform},
    ui::{Document, Kind, Session},
};
use morrow_plugin_runtime::{Limits, package::PreparedPackage};
use std::io::{Read, Write};
fn send(kind: u8, bytes: &[u8]) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    out.write_all(&[kind])?;
    out.write_all(&(bytes.len() as u32).to_le_bytes())?;
    out.write_all(bytes)?;
    out.flush()
}
fn frame(revision: u64, doc: &Document) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = revision.to_le_bytes().to_vec();
    bytes.extend(doc.encode()?);
    send(0, &bytes)?;
    Ok(())
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let package = std::env::args().nth(1).ok_or("Missing bundled plugin")?;
    let prepared = PreparedPackage::new(
        catalog::read_file(std::path::Path::new(&package))?,
        Limits::default(),
    )
    .map_err(|e| format!("Package preparation failed: {e:?}"))?;
    if !prepared.package().capabilities().is_empty() {
        return Err("Demo package requests content capabilities".into());
    }
    let temporary = tempfile::tempdir()?;
    let mut host = HostRuntime::new(Store::open(
        &temporary.path().join("demo.db"),
        EventBudget::default(),
    )?)?;
    let connection = prepared.connect(&mut host)?;
    let mut session = Session::new("demo-view", u64::MAX)?;
    let mut counter = 0u64;
    let mut produce = |handler: &str,
                       input_type: &str,
                       input: Vec<u8>|
     -> Result<Document, Box<dyn std::error::Error>> {
        counter = counter.checked_add(1).ok_or("Task limit")?;
        let task = Invocation::new_transform(
            &format!("demo-{counter}"),
            Transform {
                handler: handler.into(),
                input_type: input_type.into(),
                output_type: "morrow.ui.document.v1".into(),
                input,
            },
        )?;
        let r = prepared.run_task(&mut host, &connection, &task, || 0, Default::default());
        if r.execution.outcome != Ok(0) || r.execution.host_calls != 0 {
            return Err(format!("Plugin execution failed: {:?}", r.execution.outcome).into());
        }
        if let Some(f) = r.failure {
            return Err(format!("Plugin rejected input: {}", f.message).into());
        }
        if r.response.is_some() {
            return Err("Unexpected content response".into());
        }
        let output = r.output.ok_or("Missing plugin output")?;
        if output.type_id != "morrow.ui.document.v1" {
            return Err("Unexpected UI type".into());
        }
        let document = Document::decode(&output.bytes)?;
        // These sample handlers implement text edits only. Expose other controls as
        // disabled previews, applying the same policy to every returned snapshot.
        let mut nodes = document.nodes().to_vec();
        for n in &mut nodes {
            if matches!(n.kind, Kind::Button | Kind::Toggle) {
                n.enabled = false;
            }
        }
        Ok(Document::new(nodes)?)
    };
    let first = produce("ui.form", "text.utf8", "写下新的灵感".as_bytes().to_vec())?;
    let revision = session.replace(0, first.clone())?;
    frame(revision, &first)?;
    let mut input = std::io::stdin().lock();
    loop {
        let mut header = [0u8; 5];
        match input.read_exact(&mut header) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        let length = u32::from_le_bytes(header[1..].try_into()?) as usize;
        if header[0] == 0 && length == 0 {
            break;
        }
        if header[0] != 1 || length == 0 || length > 65536 {
            return Err("Invalid demo frame".into());
        }
        let mut bytes = vec![0; length];
        input.read_exact(&mut bytes)?;
        session.accept(&bytes)?;
        let next = produce("ui.edit", "morrow.ui.event.v1", bytes)?;
        let revision = session.replace(session.revision(), next.clone())?;
        frame(revision, &next)?;
    }
    session.close();
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        let message = format!("{e}");
        let _ = send(
            1,
            message
                .as_bytes()
                .get(..message.len().min(4096))
                .unwrap_or_default(),
        );
        std::process::exit(1);
    }
}
