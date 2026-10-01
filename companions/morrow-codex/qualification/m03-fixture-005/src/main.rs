fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args == ["--help"] {
        println!(
            "M03 real Core consumer source: native host wire adapter is not implemented. No network run is available; no default transport fallback."
        );
        return;
    }
    eprintln!("m03-stream: host-owned wire adapter not ready; no request started");
    std::process::exit(2);
}
