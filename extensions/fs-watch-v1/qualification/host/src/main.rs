//! Runs the real default Morrow raw-task Runner. No native watch is dispatched.
use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner, TaskRun};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn execute(runner: &Runner, input: &[u8], cancel: Cancellation) -> TaskRun {
    let mut unexpected = 0;
    let run = runner.run_task(
        input,
        &mut |_| {
            unexpected += 1;
            Err(())
        },
        cancel,
    );
    assert_eq!(unexpected, 0, "observation guest called a host service");
    assert_eq!(
        run.report.host_calls, 0,
        "no core service callback expected"
    );
    run
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 17 {
        return Err("language, module, reviewed hash, and thirteen input paths required".into());
    }
    let language = &args[1];
    assert!(matches!(language.as_str(), "rust" | "c" | "cpp"));
    let module = fs::read(&args[2])?;
    assert_eq!(
        format!("{:x}", Sha256::digest(&module)),
        args[3],
        "independent module pin changed"
    );
    assert!(matches!(
        Runner::new(&module, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    let runner =
        Runner::new_task(&module, Limits::default()).map_err(|e| format!("prepare: {e:?}"))?;
    let mut scenarios = 1;
    let mut inputs = Vec::new();
    for (index, path) in args[4..].iter().enumerate() {
        let input = fs::read(path)?;
        assert!((16..=65536).contains(&input.len()));
        let run = execute(&runner, &input, Cancellation::default());
        assert_eq!(run.report.outcome, Ok(0), "{language} valid {index}");
        assert_eq!(
            run.completion.as_deref(),
            Some(input.as_slice()),
            "original bytes changed"
        );
        assert!(run.report.fuel_remaining < Limits::default().fuel);
        println!(
            "WASM_VALID language={language} input={} bytes={} fuel_used={}",
            index + 1,
            input.len(),
            Limits::default().fuel - run.report.fuel_remaining
        );
        scenarios += 1;
        let mut bad = input.clone();
        bad.pop();
        let refused = execute(&runner, &bad, Cancellation::default());
        assert_eq!(
            refused.report.outcome,
            Err(Fault::TaskProtocol),
            "{language} malformed {index}"
        );
        assert!(refused.completion.is_none());
        scenarios += 1;
        inputs.push(input);
    }
    let schema = [
        0x33, 0x12, 0xe3, 0x34, 0xf5, 0xd2, 0x98, 0xd3, 0xc5, 0x6d, 0xf9, 0xc0, 0x6e, 0xd3, 0xac,
        0x94, 0x0b, 0xbf, 0x8f, 0xc9, 0xa0, 0x58, 0xd1, 0x13, 0xac, 0x30, 0xb0, 0xd6, 0x85, 0x4f,
        0x34, 0x6f,
    ];
    let mut wrong = inputs[0].clone();
    let positions: Vec<_> = wrong
        .windows(32)
        .enumerate()
        .filter_map(|(n, w)| (w == schema).then_some(n))
        .collect();
    assert_eq!(positions.len(), 1);
    wrong[positions[0]] ^= 1;
    for (name, input) in [
        ("schema", wrong),
        ("oversized", vec![0; 65537]),
        ("empty", Vec::new()),
    ] {
        let refused = execute(&runner, &input, Cancellation::default());
        assert_eq!(
            refused.report.outcome,
            Err(if name == "empty" {
                Fault::Limits
            } else {
                Fault::TaskProtocol
            }),
            "{language} {name}"
        );
        assert!(refused.completion.is_none());
        scenarios += 1;
    }
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let stopped = execute(&runner, &inputs[0], cancelled);
    assert_eq!(stopped.report.outcome, Err(Fault::Cancelled));
    assert!(stopped.completion.is_none());
    scenarios += 1;
    let limited = Runner::new_task(
        &module,
        Limits {
            fuel: 1,
            ..Limits::default()
        },
    )
    .map_err(|e| format!("fuel prepare: {e:?}"))?;
    let exhausted = execute(&limited, &inputs[0], Cancellation::default());
    assert_eq!(exhausted.report.outcome, Err(Fault::Limits));
    assert!(exhausted.completion.is_none());
    scenarios += 1;
    assert_eq!(scenarios, 32);
    assert!(Path::new(&args[2]).is_absolute());
    println!(
        "WATCH_WASM_PASS language={language} scenarios={scenarios} original_native_inputs=6 synthetic_inputs=7 byte_roundtrips=13 malformed_refusals=13 schema_refusals=1 oversized_refusals=1 empty_refusals=1 cancellations=1 fuel_refusals=1 wrong_profile_refusals=1 native_watch_calls=0"
    );
    Ok(())
}
