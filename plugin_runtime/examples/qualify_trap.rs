use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner};
fn main() {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    assert!(!paths.is_empty());
    for path in paths {
        let runner = Runner::new(&std::fs::read(&path).unwrap(), Limits::default())
            .expect("fixed guest imports");
        let result = runner.run(
            &mut |_| panic!("trap fixture must not submit"),
            Cancellation::default(),
        );
        assert_eq!(result.outcome, Err(Fault::Trap));
        assert_eq!(result.host_calls, 0);
        println!("PASS: {path}: C++ failure traps locally, no host submission");
    }
}
