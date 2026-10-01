//! Real same-user data connector for qualification only; no Core/HTTP.
use morrow_native_pipe_win::Pipe;
use std::io::{BufRead, Write};
fn main() {
    let locator = std::io::stdin().lock().lines().next().unwrap().unwrap();
    let _pipe = Pipe::open_client(&locator).expect("single actual connect");
    println!("{}", std::process::id());
    std::io::stdout().flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
}
