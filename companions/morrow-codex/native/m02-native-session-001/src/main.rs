//! Native host-owned session consumer; stdout is binary in session mode.
fn main() -> std::process::ExitCode {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().and_then(|v| v.to_str()) == Some("--morrow-native-session-v2") {
        let options = match morrow_native_session_client::client::Options::parse(&arguments[1..]) {
            Ok(options) => options,
            Err(reason) => {
                eprintln!("{reason}");
                return std::process::ExitCode::from(2);
            }
        };
        return match morrow_native_session_client::client::run(options) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(end) => {
                eprintln!("session ended: {}", end.reason);
                std::process::ExitCode::from(end.exit)
            }
        };
    }
    if arguments.len() == 1 {
        match arguments[0].to_str() {
            Some("--help") => {
                println!(
                    "morrow-session-client: host-owned native session client\nOffline: --help, --version, --check\nHost launch: --morrow-native-session-v2 [--queries 1..64] [--interval-ms 0..10000] [--io-timeout-ms 100..5000]\nOnly host-owned read-only session status is supported. No launch option grants authority."
                );
                return std::process::ExitCode::SUCCESS;
            }
            Some("--version") => {
                println!(
                    "morrow-session-client {} capnp-v2-r1",
                    env!("CARGO_PKG_VERSION")
                );
                return std::process::ExitCode::SUCCESS;
            }
            Some("--check") => {
                println!(
                    "static-check-only; host-session=unavailable; authorization=absent; execution=unsupported; network=unsupported; persistence=unsupported"
                );
                return std::process::ExitCode::SUCCESS;
            }
            _ => {}
        }
    }
    eprintln!("host session unavailable: no verified launch contract; operation refused");
    std::process::ExitCode::from(2)
}
