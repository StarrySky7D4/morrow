#[cfg(target_os = "windows")]
mod a1_allow_table;
#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    // Must precede every provisioning/log/payload/account/default setup path.
    if let Some(status)=codex_windows_sandbox::asuser_a1_dispatch(&a1_allow_table::TABLE) {
        std::process::exit(status);
    }
    if let Some(status)=codex_windows_sandbox::station_broker_a0_dispatch() {
        std::process::exit(status);
    }
    codex_windows_sandbox::setup_helper_main()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    panic!("codex-windows-sandbox-setup is Windows-only");
}
