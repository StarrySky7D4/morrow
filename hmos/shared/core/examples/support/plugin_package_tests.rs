use super::*;
fn setup(command: &str) -> (tempfile::TempDir, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let module = dir.path().join("module.wasm");
    // An offline container fixture, intentionally not a claim of executable guest ABI.
    std::fs::write(&module, b"\0asm\x01\0\0\0").unwrap();
    let output = dir.path().join(if command == "pack-v2-catalog" {
        "catalog"
    } else {
        "package.mplugin"
    });
    let args = vec![
        command.into(),
        module.to_str().unwrap().into(),
        output.to_str().unwrap().into(),
        "org.example.fixture".into(),
        "1.2.3-test.1".into(),
    ];
    (dir, args)
}
fn add(args: &mut Vec<String>, values: &[&str]) {
    args.extend(values.iter().map(|v| (*v).into()));
}
fn execute(args: &[String]) -> String {
    let mut out = vec![];
    run(args, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}
#[test]
fn seven_capabilities_and_task_defaults_are_roundtripped() {
    let (_dir, mut args) = setup("pack-v2");
    for cap in [
        "rename",
        "summary",
        "operation",
        "attachment",
        "create-content",
        "edit-content",
        "read-content",
    ] {
        add(&mut args, &["--capability", cap]);
    }
    add(&mut args, &["--name", "Developer fixture"]);
    let output = execute(&args);
    let p = catalog::read_file(Path::new(&args[2])).unwrap();
    assert_eq!(p.capabilities().len(), 7);
    assert_eq!(p.manifest().guest_abi_version, 2);
    assert_eq!(p.manifest().display_name, "Developer fixture");
    assert!(p.manifest().transform_handlers.is_empty());
    assert!(p.manifest().required_features.is_empty());
    let published = output
        .lines()
        .next()
        .unwrap()
        .strip_prefix("published ")
        .unwrap();
    assert!(Path::new(published).is_absolute());
    assert_eq!(
        Path::new(published),
        Path::new(&args[2]).canonicalize().unwrap()
    );
    assert!(output.contains("authorization"));
}
#[test]
fn handlers_capabilities_dependencies_and_budgets_keep_exact_range() {
    let (_dir, mut args) = setup("pack-v2");
    add(
        &mut args,
        &[
            "--name",
            "Combined",
            "--capability",
            "read-content",
            "--handler",
            "bytes.one",
            "bytes",
            "bytes",
            "65536",
            "65536",
            "--handler",
            "bytes.two",
            "bytes",
            "text.utf8",
            "0",
            "32",
            "--dependency",
            "required-slot",
            "bytes.reverse",
            "bytes",
            "bytes",
            ">=1.0, <2.0",
            "required",
            "--dependency",
            "optional-slot",
            "bytes.other",
            "bytes",
            "text.utf8",
            "^1.2.3",
            "optional",
            "--dependency-calls",
            "--fuel",
            "100000000",
            "--memory-bytes",
            "67108864",
            "--host-calls",
            "1024",
        ],
    );
    execute(&args);
    let p = catalog::read_file(Path::new(&args[2])).unwrap();
    let m = p.manifest();
    assert_eq!(p.capabilities().len(), 1);
    assert_eq!(m.transform_handlers.len(), 2);
    assert_eq!(m.dependencies.len(), 2);
    assert_eq!(m.dependencies[0].provider_version, ">=1.0, <2.0");
    assert!(!m.dependencies[0].optional);
    assert!(m.dependencies[1].optional);
    assert_eq!(
        m.required_features,
        vec![
            "transform-handlers-v1",
            "dependencies-v1",
            "dependency-calls-v1"
        ]
    );
    assert_eq!(
        m.dependency_schema_sha256,
        morrow_core::dependency_call::schema_digest()
    );
    let b = m.budget.as_ref().unwrap();
    assert_eq!(
        (b.fuel, b.memory_bytes, b.host_calls),
        (100000000, 67108864, 1024)
    );
    let text = execute(&["inspect".into(), args[2].clone()]);
    for expected in [
        "name=\"Combined\"",
        "dependency=required-slot",
        "version=\">=1.0, <2.0\"",
        "optional",
        "fuel=100000000",
        "dependency-schema-sha256=",
    ] {
        assert!(text.contains(expected), "{text}");
    }
}
#[test]
fn task_dependency_without_dynamic_call_feature_is_valid() {
    let (_dir, mut args) = setup("pack-v2");
    add(
        &mut args,
        &[
            "--dependency",
            "slot",
            "bytes.reverse",
            "bytes",
            "bytes",
            "*",
            "required",
        ],
    );
    execute(&args);
    let p = catalog::read_file(Path::new(&args[2])).unwrap();
    assert_eq!(p.manifest().required_features, vec!["dependencies-v1"]);
    assert!(p.manifest().dependency_schema_sha256.is_empty());
}
#[test]
fn malformed_options_leave_no_output_or_catalog() {
    let cases: [(&[&str], &str); 16] = [
        (&["--unknown"], "unknown option"),
        (&["--name"], "requires VALUE"),
        (&["--name", "a", "--name", "b"], "duplicate single-value"),
        (&["--fuel", "1", "--fuel", "2"], "duplicate single-value"),
        (
            &["--dependency-calls", "--dependency-calls"],
            "duplicate single-value",
        ),
        (&["--capability", "rename-card"], "unknown capability"),
        (&["--fuel", "-1"], "unsigned integer"),
        (&["--fuel", "18446744073709551616"], "unsigned integer"),
        (&["--fuel", "0"], "expects 1..=100000000"),
        (&["--memory-bytes", "65537"], "multiple of 65536"),
        (&["--host-calls", "1025"], "expects 0..=1024"),
        (
            &["--handler", "x", "bytes", "bytes", "65537", "0"],
            "MAX_INPUT",
        ),
        (
            &["--handler", "x", "bytes", "bytes", "0"],
            "requires MAX_OUTPUT",
        ),
        (
            &["--dependency", "slot", "h", "bytes", "bytes", "^1", "maybe"],
            "required|optional",
        ),
        (
            &["--dependency", "slot", "h", "bytes", "bytes", "^1"],
            "requires required|optional",
        ),
        (&["--dependency-calls"], "requires at least one --handler"),
    ];
    for command in ["pack-v2", "pack-v2-catalog"] {
        for (extra, expected) in cases {
            let (dir, mut args) = setup(command);
            add(&mut args, extra);
            let error = run(&args, &mut vec![]).unwrap_err().to_string();
            assert!(error.contains(expected), "{extra:?}: {error}");
            assert!(!Path::new(&args[2]).exists());
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
}
#[test]
fn package_validation_failures_never_publish() {
    for extra in [
        vec![
            "--dependency",
            "slot",
            "h",
            "bytes",
            "bytes",
            "not-semver",
            "required",
        ],
        vec!["--capability", "rename", "--capability", "rename"],
        vec![
            "--handler",
            "h",
            "bytes",
            "bytes",
            "0",
            "0",
            "--handler",
            "h",
            "bytes",
            "bytes",
            "0",
            "0",
        ],
        vec!["--name", ""],
    ] {
        let (dir, mut args) = setup("pack-v2-catalog");
        add(&mut args, &extra);
        assert!(run(&args, &mut vec![]).is_err());
        assert!(!Path::new(&args[2]).exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    let (_dir, args) = setup("pack-v2");
    std::fs::write(&args[1], b"not wasm").unwrap();
    assert!(
        run(&args, &mut vec![])
            .unwrap_err()
            .to_string()
            .contains("wasm module header")
    );
    assert!(!Path::new(&args[2]).exists());
}
#[test]
fn catalog_retries_are_idempotent_and_corrupt_existing_archive_is_not_overwritten() {
    let (_dir, args) = setup("pack-v2-catalog");
    let first = execute(&args);
    assert_eq!(execute(&args), first);
    let path = PathBuf::from(
        first
            .lines()
            .next()
            .unwrap()
            .strip_prefix("published ")
            .unwrap(),
    );
    let p = catalog::read_file(&path).unwrap();
    assert_eq!(
        path.file_name().unwrap(),
        format!("{}.mplugin", hex(&p.digest())).as_str()
    );
    assert_eq!(std::fs::read_dir(&args[2]).unwrap().count(), 1);
    std::fs::write(&path, b"corrupt existing package").unwrap();
    assert!(run(&args, &mut vec![]).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt existing package");
    assert_eq!(std::fs::read_dir(&args[2]).unwrap().count(), 1);
}
#[test]
fn legacy_modes_install_and_explicit_file_no_clobber_remain_compatible() {
    for (command, spec, abi) in [
        ("pack", "rename,summary,operation,attachment", 1),
        ("pack-task", "create-content,edit-content,read-content", 2),
        ("pack-transform", "h,bytes,bytes,64,64", 2),
    ] {
        let (dir, mut args) = setup(command);
        args.push(spec.into());
        execute(&args);
        let p = catalog::read_file(Path::new(&args[2])).unwrap();
        assert_eq!(p.manifest().guest_abi_version, abi);
        let original = std::fs::read(&args[2]).unwrap();
        assert!(run(&args, &mut vec![]).is_err());
        assert_eq!(std::fs::read(&args[2]).unwrap(), original);
        let catalog = dir.path().join("installed");
        let output = execute(&[
            "install".into(),
            args[2].clone(),
            catalog.to_str().unwrap().into(),
        ]);
        assert!(output.starts_with("installed "));
        assert_eq!(
            Catalog::open(&catalog)
                .unwrap()
                .load(p.digest())
                .unwrap()
                .archive(),
            p.archive()
        );
    }
}
