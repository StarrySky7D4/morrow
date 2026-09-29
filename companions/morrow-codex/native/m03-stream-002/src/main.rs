//! Controlled batch executable. Stdout is exclusively v3 Capnp in native mode.
use codex_core::ResponseEvent;
use morrow_codex_m03_stream::{
    driver::RequestKey,
    request_task::{self, FixtureRequest},
};
use morrow_codex_native_http_client::workers::ControlReader;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const OUTPUT_ROOT: &str = r"C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex\out";
struct Options {
    base: String,
    evidence: PathBuf,
    max_chunk: usize,
}
fn options() -> Result<Options, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--morrow-native-http-v3") {
        return Err("requires host-owned native v3 launch".into());
    }
    let mut base = None;
    let mut evidence = None;
    let mut max_chunk = None;
    let mut index = 1;
    while index < args.len() {
        let value = args.get(index + 1).ok_or("missing option value")?;
        match args[index].as_str() {
            "--fixture-base" if base.is_none() => base = Some(value.clone()),
            "--evidence-dir" if evidence.is_none() => evidence = Some(PathBuf::from(value)),
            "--max-chunk" if max_chunk.is_none() => {
                max_chunk = Some(value.parse::<usize>().map_err(|_| "chunk integer")?)
            }
            _ => return Err("unknown/duplicate native option".into()),
        }
        index += 2;
    }
    let base = base.ok_or("fixture base required")?;
    morrow_codex_m03_stream::limits::fixture_base(&base).map_err(str::to_owned)?;
    let evidence = evidence.ok_or("existing batch evidence directory required")?;
    let max_chunk = max_chunk.ok_or("max chunk required")?;
    if max_chunk == 0 || max_chunk > 8192 {
        return Err("max chunk outside fixed bounds".into());
    }
    let root = Path::new(OUTPUT_ROOT)
        .canonicalize()
        .map_err(|_| "output root")?;
    let canonical = evidence.canonicalize().map_err(|_| "evidence directory")?;
    if canonical == root || !canonical.starts_with(&root) || !canonical.is_dir() {
        return Err("evidence must be a batch directory under plugin out".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        for path in evidence.ancestors() {
            if std::fs::symlink_metadata(path)
                .map_err(|_| "evidence ancestor")?
                .file_attributes()
                & 0x400
                != 0
            {
                return Err("reparse evidence path".into());
            }
        }
    }
    Ok(Options {
        base,
        evidence: canonical,
        max_chunk,
    })
}
fn artifact() -> Result<[u8; 32], String> {
    let mut input = File::open(std::env::current_exe().map_err(|_| "current exe")?)
        .map_err(|_| "open actual artifact")?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = input.read(&mut buffer).map_err(|_| "read artifact")?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest.finalize().into())
}
fn main() -> std::process::ExitCode {
    if std::env::args().skip(1).collect::<Vec<_>>() == ["--help"] {
        println!(
            "morrow-codex-native-http-client: controlled native v3 fixture\n--morrow-native-http-v3 --fixture-base http://127.0.0.1:PORT/v1 --evidence-dir BATCH_DIR --max-chunk 1024\nNo argument grants HTTP; host proposal approval is mandatory."
        );
        return std::process::ExitCode::SUCCESS;
    }
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "native fixture ended: {}",
                error.chars().take(1024).collect::<String>()
            );
            std::process::ExitCode::from(2)
        }
    }
}
fn run() -> Result<(), String> {
    let options = options()?;
    let mut events = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(options.evidence.join("core-events.jsonl"))
        .map_err(|_| "create new event evidence")?;
    let mut result = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(options.evidence.join("result.json"))
        .map_err(|_| "create new result evidence")?;
    let actual_artifact = artifact()?;
    let session = ControlReader::start()
        .admit(actual_artifact, options.max_chunk, RequestKey(1))
        .map_err(|e| e.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Tokio runtime")?;
    let started = Instant::now();
    runtime.block_on(async {
        let mut task=request_task::start(session.clone(),FixtureRequest{base_url:options.base,
            original_native_deadline:tokio::time::Instant::from_std(session.deadline()),max_parser_chunk:options.max_chunk,local_request_key:RequestKey(1)}).map_err(|e|e.to_string())?;
        let mut ordinal=0u64;
        while let Some(event)=task.next_event().await {
            ordinal+=1;
            let value=match event {
                Ok(ResponseEvent::OutputTextDelta(text))=>json!({"kind":"actual_core_output_text_delta","ordinal":ordinal,"bytes":text.len(),"sha256":format!("{:x}",Sha256::digest(text.as_bytes()))}),
                Ok(ResponseEvent::Completed{end_turn,..})=>json!({"kind":"actual_core_completed","ordinal":ordinal,"end_turn":end_turn}),
                Ok(_)=>json!({"kind":"actual_core_other_event","ordinal":ordinal}),
                Err(error)=>json!({"kind":"core_consumer_error","ordinal":ordinal,"error":error}),
            };
            let line=json!({"elapsed_us":started.elapsed().as_micros(),"event":value});
            serde_json::to_writer(&mut events,&line).map_err(|_|"event serialization")?;
            events.write_all(b"\n").and_then(|_|events.flush()).map_err(|_|"event evidence write")?;
            // A trusted harness may use the flushed real first-delta record as
            // the fixture EOF barrier. This process never writes fixture control.
        }
        let receipt=task.wait_cleanup().await.map_err(|e|e.to_string())?;
        let close=match session.close() {
            Ok(())=>match tokio::time::timeout(Duration::from_millis(500),session.wait_close()).await {
                Ok(result)=>result,Err(_)=>Err(morrow_codex_m03_stream::driver::BridgeError::CleanupUnconfirmed)
            },
            Err(error)=>Err(error),
        };
        let final_control=session.final_control_status();
        let (frames,frames_overflow)=session.observations();
        let (io,io_overflow,io_joined)=session.io_observations();
        let local_cleanup_ok=receipt.cleanup.as_ref().is_ok_and(|r|r.request_closed&&r.data_channel_closed&&r.data_connect_reaped&&r.data_read_reaped&&r.data_write_reaped&&(!r.network_worker_started||r.network_worker_exited))&&io_joined;
        let model_complete=matches!(receipt.core_terminal,request_task::CoreTerminal::Completed{..});
        let transport_complete=matches!(receipt.transport_drain,Some(Ok(())));
        let report=json!({"status":"native_fixture_observation","identity":session.identity(),"task":receipt,"frames":frames,"frames_overflow":frames_overflow,
            "io":io,"io_overflow":io_overflow,"local_data_thread_joined":io_joined,"product_success_claimed":false,
            "close_result":close,"final_control_result":final_control,"result_snapshot":"after_close_observation",
            "whole_session_release":"external_operator_only"});
        serde_json::to_writer_pretty(&mut result,&report).map_err(|_|"result serialization")?;
        result.write_all(b"\n").and_then(|_|result.flush()).map_err(|_|"result evidence write")?;
        if close.is_err()||final_control.is_err(){return Err("native terminal control failure; see final close result".into())}
        if !local_cleanup_ok||!model_complete||!transport_complete{return Err("request incomplete; inspect independent task/evidence/cleanup dimensions".into())}
        Ok(())
    })
}
