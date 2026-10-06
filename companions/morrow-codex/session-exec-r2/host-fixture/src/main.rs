//! Qualification-only process owning the genuine Core/SDK native authority.
//! It is not a production plugin transport or a package review UI.
#![deny(unsafe_code)]
use morrow_agent_session_exec_v1_r2::authority::{Admission, Capabilities, SessionExecHost};
use morrow_core::{
    dispatch::HostRuntime,
    store::{EventBudget, Store},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("database path required")?;
    let sessions = args.collect::<Vec<_>>();
    if sessions.is_empty() || sessions.len() > 16 {
        return Err("reviewed finite scope required".into());
    }
    let mut runtime = HostRuntime::new(Store::open(
        std::path::Path::new(&path),
        EventBudget::default(),
    )?)?;
    let connection = runtime.connect()?;
    let host = SessionExecHost::new(&mut runtime).map_err(|error| format!("{error:?}"))?;
    let generation = host
        .generation(&runtime)
        .map_err(|error| format!("{error:?}"))?;
    let declared = Capabilities {
        session_read: true,
        session_write: true,
        retire: true,
        ..Default::default()
    };
    // Fixture operator authorization is established at process startup, outside
    // the guest codec; declarations alone never create this admission.
    let control = host
        .admit(
            &runtime,
            &connection,
            declared,
            declared,
            sessions.clone(),
            "codex-fixture-session".into(),
            100,
            0,
        )
        .map_err(|error| format!("{error:?}"))?;
    let mut endpoints = BTreeMap::<u64, Admission>::from([(0, control)]);
    let mut next = 1u64;
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    for line in std::io::stdin().lock().lines() {
        let value: Value = serde_json::from_str(&line?)?;
        let result: Result<Value, String> = (|| {
            let operation = value["operation"].as_str().ok_or("operation required")?;
            match operation {
                "hello" => Ok(json!({ "generation": generation, "control": 0 })),
                "admit_writer" => {
                    let session = value["session"].as_str().ok_or("session required")?;
                    if !sessions.iter().any(|item| item == session) {
                        return Err("scope denied".into());
                    }
                    let approved = Capabilities {
                        session_read: true,
                        session_write: true,
                        ..Default::default()
                    };
                    let admission = host
                        .admit(
                            &runtime,
                            &connection,
                            declared,
                            approved,
                            vec![session.into()],
                            "codex-fixture-session".into(),
                            100,
                            1,
                        )
                        .map_err(|error| format!("{error:?}"))?;
                    let endpoint = next;
                    next = next.checked_add(1).ok_or("endpoint exhaustion")?;
                    endpoints.insert(endpoint, admission);
                    Ok(json!({"endpoint": endpoint, "generation": generation}))
                }
                "exchange" => {
                    let endpoint = value["endpoint"].as_u64().ok_or("endpoint required")?;
                    let admission = endpoints
                        .get(&endpoint)
                        .ok_or("original endpoint not found")?;
                    let frame: Vec<u8> = serde_json::from_value(value["frame"].clone())
                        .map_err(|error| error.to_string())?;
                    let reply = host
                        .dispatch(&mut runtime, &connection, admission, &frame, || 1)
                        .map_err(|error| format!("{error:?}"))?;
                    Ok(json!({"frame": reply}))
                }
                "revoke" => {
                    let endpoint = value["endpoint"].as_u64().ok_or("endpoint required")?;
                    let admission = endpoints
                        .get(&endpoint)
                        .ok_or("original endpoint not found")?;
                    host.revoke(admission)
                        .map_err(|error| format!("{error:?}"))?;
                    Ok(Value::Null)
                }
                "retire" => {
                    let session = value["session"].as_str().ok_or("session required")?;
                    let control = endpoints.get(&0).ok_or("control not found")?;
                    let review = host
                        .review_session_retirement(&runtime, &connection, control, session, 1)
                        .map_err(|error| format!("{error:?}"))?;
                    host.retire_session(
                        &mut runtime,
                        &connection,
                        control,
                        session,
                        review.info.revision,
                        review.record_sha256,
                        || 1,
                    )
                    .map_err(|error| format!("{error:?}"))?;
                    Ok(Value::Null)
                }
                _ => Err("unknown operation".into()),
            }
        })();
        serde_json::to_writer(
            &mut output,
            &match result {
                Ok(value) => json!({"ok": value}),
                Err(error) => json!({"error": error}),
            },
        )?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}
