//! Synthetic protocol fixture, no Core/backend/HTTP. Never a plugin substitute.
use morrow_native_session_stream::wire as w;
use std::io::{Read, Write};
fn read_record(r: &mut impl Read) -> Result<(w::Frame,Vec<u8>), String> {
    let mut p = [0; 4];
    r.read_exact(&mut p).map_err(|e| e.to_string())?;
    let n = w::payload_length(&p).map_err(str::to_owned)?;
    let mut b = p.to_vec();
    b.resize(n + 4, 0);
    r.read_exact(&mut b[4..]).map_err(|e| e.to_string())?;
    let frame=w::Frame::decode(&b).map_err(str::to_owned)?;
    Ok((frame,b))
}
fn read(r:&mut impl Read)->Result<w::Frame,String>{read_record(r).map(|(frame,_)|frame)}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len()==2 && args[1]=="--stdio-holder" {
        std::thread::sleep(std::time::Duration::from_secs(30));return Ok(());
    }
    if args.len() != 4 || args[1] != "--morrow-native-http-v3" {
        return Err("fixture args".into());
    }
    let mode = &args[2];
    let path = &args[3];
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    let initial = read(&mut input)?;
    if initial.kind != w::Kind::Challenge || initial.child_pid != std::process::id() {
        return Err("challenge identity".into());
    }
    let mut hello = initial.clone();
    hello.kind = w::Kind::Hello;
    hello.sequence = 1;
    output
        .write_all(&hello.encode().map_err(str::to_owned)?)
        .map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())?;
    loop {
        let f = read(&mut input)?;
        if f.kind == w::Kind::Welcome {
            if f.code != 0 || f.sequence != 1 {
                return Err("Welcome requires success code0 and Hello sequence1".into());
            }
            break;
        }
        if f.kind == w::Kind::Stop {
            return Err("pre-Welcome Stop".into());
        }
    }
    if mode=="descendant-stdio" || mode=="descendant-hold" {
        let child=std::process::Command::new(std::env::current_exe().map_err(|e|e.to_string())?)
            .arg("--stdio-holder").stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::inherit()).stderr(std::process::Stdio::inherit())
            .spawn().map_err(|e|e.to_string())?;
        std::fs::write(path,serde_json::json!({"direct_pid":std::process::id(),"descendant_pid":child.id(),"inherited_stdio":true}).to_string()).map_err(|e|e.to_string())?;
        if mode=="descendant-hold" {loop {if read(&mut input)?.kind==w::Kind::Stop{break;}}}
        return Ok(());
    }
    // Real child/control-only expiry probes; no Core/model/HTTP call.
    let expiry_mode = mode.starts_with("expiry-");
    let mut initial_stop = false;
    if expiry_mode {
        loop {
            let (f,raw)=read_record(&mut input)?;
            if f.kind==w::Kind::Stop {
                if !f.same_admission(&initial) || f.revocation_generation!=2
                    || f.remaining_ms!=0 || f.request_budget!=initial.request_budget {
                    return Err("expiry Stop identity/budget mismatch".into());
                }
                if f.code!=20 { return Err("expiry probe requires first Stop20".into()); }
                if mode=="expiry-no-close" {
                    // A kill-negative fixture cannot promise a final file after Job kill.
                    // Persist the actual guest observation before deliberately withholding Close.
                    let receipt=serde_json::json!({"mode":mode,"phase":"stop-observed-no-close",
                        "child_pid":std::process::id(),"session":initial.session,"epoch":initial.instance_epoch,
                        "stop_received":true,"stop_code":f.code,"no_close_submitted":true,
                        "http_not_invoked":true,"ack_received":false,"terminal_protocol_complete":false,
                        "stop_raw_hex":w::hex(&raw)});
                    let mut file=std::fs::OpenOptions::new().create_new(true).write(true)
                        .open(format!("{path}.stop-receipt.json")).map_err(|e|e.to_string())?;
                    serde_json::to_writer(&mut file,&receipt).map_err(|e|e.to_string())?;
                    file.sync_all().map_err(|e|e.to_string())?;
                }
                initial_stop=true;break;
            }
        }
    }
    let mut close = initial.clone();
    close.kind = w::Kind::Close;
    close.sequence = if mode == "bad-sequence" || mode == "expiry-bad-sequence" { 3 } else { 2 };
    if mode=="expiry-bad-identity" { close.child_pid+=1; }
    if mode=="expiry-bad-budget" { close.request_budget-=1; }
    if mode=="expiry-positive-credit" {
        close.kind=w::Kind::HttpCredit;
        close.payload=w::Payload::Credit(w::Credit{consumed_offset:0,window_bytes:1,max_chunk_bytes:1024,parser_yielded_bytes:0,drain_discarded_bytes:0,cancel_discarded_bytes:0,error_consumed_bytes:0});
    }
    let mut bytes = close.encode().map_err(str::to_owned)?;
    match mode.as_str() {
        "expiry-no-close"=>bytes.clear(),
        "expiry-partial"=>bytes.truncate(12),
        "expiry-malformed"=>bytes=vec![0;4],
        "expiry-replay"=>{let again=bytes.clone();bytes.extend_from_slice(&again);},
        "expiry-tail"=>bytes.push(0xff),
        "expiry-stderr-limit"=>{std::io::stderr().write_all(&vec![b'x';65537]).map_err(|e|e.to_string())?;},
        _=>{},
    }
    if mode == "post-close-bytes" {
        bytes.push(0xff);
    }
    output.write_all(&bytes).map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())?;
    let mut frames = vec![];
    let mut ack = false;
    let mut stop = initial_stop;
    loop {
        let mut prefix = [0; 4];
        match input.read(&mut prefix[..1]) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
        input
            .read_exact(&mut prefix[1..])
            .map_err(|e| e.to_string())?;
        let n = w::payload_length(&prefix).map_err(str::to_owned)?;
        let mut bytes = prefix.to_vec();
        bytes.resize(n + 4, 0);
        input
            .read_exact(&mut bytes[4..])
            .map_err(|e| e.to_string())?;
        let f = w::Frame::decode(&bytes).map_err(str::to_owned)?;
        if f.kind == w::Kind::State && f.sequence == 2 && f.code == 0 {
            ack = true;
            if let w::Payload::Progress(p) = &f.payload {
                if p.owner_released || p.child_exited {
                    return Err("premature session release".into());
                }
            } else {
                return Err("ACK payload".into());
            }
        }
        if f.kind == w::Kind::Stop {
            stop = true;
        }
        frames.push(w::hex(&bytes));
    }
    let result = serde_json::json!({"mode":mode,"ack_received":ack,"stop_received":stop,"stdin_eof_after_frames":true,"frames":frames,"http_not_invoked":true});
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut f, &result).map_err(|e| e.to_string())?;
    if mode=="expiry-normal" && (!ack || !stop) {
        return Err("expiry lacks complete terminal ACK after actual Stop20".into());
    }
    if mode == "normal" && (!ack || stop) {
        return Err("normal Close lacks exclusive complete ACK".into());
    }
    if mode == "bad-sequence" && (ack || !stop) {
        return Err("invalid Close accepted".into());
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(2)
    }
}
