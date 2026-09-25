//! Native execution vectors for browser parity, using the unmodified bundled archive.
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::Package,
    runtime::{Command, RenameRequest},
    store::{EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{Limits, package::PreparedPackage};
use morrow_workbench_plugin::{Action, Idea, Request, codec};
use std::{fs, path::PathBuf};

fn registry_vectors(
    output: &std::path::Path,
    actual: &Package,
) -> Result<(), Box<dyn std::error::Error>> {
    use morrow_core::{
        lifecycle::GrantKind,
        plugin_package::{catalog::Catalog, proto::Capability, registry::Registry},
    };
    use std::collections::BTreeSet;
    let dir = tempfile::tempdir()?;
    let mut registry = Registry::open(
        &dir.path().join("registry"),
        Catalog::open(&dir.path().join("packages"))?,
    )?;
    let wasm = b"\0asm\x01\0\0\0";
    let id = "org.example.registry";
    let first = Package::build(
        Package::manifest_for(
            id,
            "1.0.0",
            wasm,
            vec![Capability::RenameCard, Capability::ReadSummary],
        ),
        wasm,
    )?;
    let second = Package::build(
        Package::manifest_for(
            id,
            "2.0.0",
            wasm,
            vec![Capability::RenameCard, Capability::ReadAttachment],
        ),
        wasm,
    )?;
    for (name, package) in [("first", &first), ("second", &second), ("actual", actual)] {
        fs::write(
            output.join(format!("{name}.package.bin")),
            package.archive(),
        )?;
        registry.install_package(package.archive())?;
    }
    fs::write(output.join("actual-id.txt"), &actual.manifest().package_id)?;
    let save = |name: &str, registry: &Registry| -> Result<(), Box<dyn std::error::Error>> {
        fs::write(
            output.join(format!("{name}.registry.bin")),
            registry.persisted_snapshot()?.ok_or("missing snapshot")?,
        )?;
        fs::write(
            output.join(format!("{name}.revision.txt")),
            registry.revision().to_string(),
        )?;
        Ok(())
    };
    registry.select(first.digest(), 0)?;
    save("selected", &registry)?;
    registry.approve(
        id,
        first.digest(),
        BTreeSet::from([GrantKind::Rename, GrantKind::ReadSummary]),
        registry.revision(),
    )?;
    save("approved", &registry)?;
    registry.set_enabled(id, first.digest(), true, registry.revision())?;
    save("enabled", &registry)?;
    registry.select(second.digest(), registry.revision())?;
    save("upgraded", &registry)?;
    registry.set_enabled(id, second.digest(), true, registry.revision())?;
    save("enabled-v2", &registry)?;
    registry.approve(id, second.digest(), BTreeSet::new(), registry.revision())?;
    save("revoked", &registry)?;
    registry.set_enabled(id, second.digest(), false, registry.revision())?;
    save("disabled", &registry)?;
    registry.remove(id, registry.revision())?;
    save("removed", &registry)?;
    registry.select(first.digest(), registry.revision())?;
    save("reselected", &registry)?;
    registry.select(actual.digest(), registry.revision())?;
    save("actual-selected", &registry)?;
    registry.set_enabled(
        &actual.manifest().package_id,
        actual.digest(),
        true,
        registry.revision(),
    )?;
    save("actual-enabled", &registry)?;
    registry.set_enabled(
        &actual.manifest().package_id,
        actual.digest(),
        false,
        registry.revision(),
    )?;
    save("actual-disabled", &registry)?;
    Ok(())
}

fn request(action: Action, idea: &Idea) -> Request {
    Request {
        action,
        current: idea.clone(),
        proposed: idea.clone(),
        text: String::new(),
        flag: false,
        now_ms: 123456789,
        ideas: vec![idea.clone()],
        section: "概览".into(),
        filter: "全部".into(),
        sort: "最近添加".into(),
    }
}

fn task(name: &str, request: Request) -> Result<Invocation, Box<dyn std::error::Error>> {
    Ok(Invocation::new_transform(
        name,
        Transform {
            handler: "workbench.command".into(),
            input_type: "morrow.workbench.request.v1".into(),
            output_type: "morrow.workbench.response.v1".into(),
            input: codec::encode_request(&request)?,
        },
    )?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let archive = fs::read(args.next().ok_or("package archive required")?)?;
    let output = PathBuf::from(args.next().ok_or("output directory required")?);
    fs::create_dir_all(&output)?;
    let package = Package::decode(&archive)?;
    registry_vectors(&output, &package)?;
    dependency_vectors(&output)?;
    fs::write(output.join("package.bin"), &archive)?;
    fs::write(output.join("digest.bin"), package.digest())?;
    let prepared =
        PreparedPackage::new(package, Limits::default()).map_err(|e| format!("prepare {e:?}"))?;
    let temporary = tempfile::tempdir()?;
    let mut host = HostRuntime::new(Store::open(
        &temporary.path().join("vectors.db"),
        EventBudget::default(),
    )?)?;
    let connection = prepared.connect_approved(&mut host, &Default::default())?;
    let idea = Idea {
        id: "web-parity-idea".into(),
        title: "同一原包 🧭".into(),
        description: "Win/Web Unicode".into(),
        category: "灵感".into(),
        stage: "待整理".into(),
        todos: vec!["first".into(), "第二项".into()],
        ..Idea::default()
    };
    let mut cases = Vec::new();
    for (name, action) in [
        ("create", Action::Create),
        ("edit", Action::Edit),
        ("favorite", Action::Favorite),
        ("todo", Action::Todo),
        ("project", Action::ToProject),
        ("delete", Action::Delete),
        ("restore", Action::Restore),
        ("query", Action::Query),
    ] {
        let mut input = request(action, &idea);
        match action {
            Action::Create => input.proposed.stage.clear(),
            Action::Edit => input.proposed.title = "修改后的标题 😀".into(),
            Action::Favorite => input.flag = true,
            Action::Todo => {
                input.text = "第二项".into();
                input.flag = true;
            }
            Action::Restore => {
                input.current.deleted = true;
                input.current.deleted_at = 123;
            }
            Action::Query => {
                input.text = "Unicode".into();
                input.sort = "标题排序".into();
            }
            _ => {}
        }
        cases.push((name, task(name, input)?));
    }
    let mut invalid = request(Action::Create, &idea);
    invalid.proposed.category = "not-a-category".into();
    cases.push(("plugin-failure", task("plugin-failure", invalid)?));
    let mut names = Vec::new();
    for (name, input) in cases {
        let report = prepared.run_task(&mut host, &connection, &input, || 1, Default::default());
        if report.execution.outcome != Ok(0) || report.execution.host_calls != 0 {
            return Err(format!("{name}: {:?}", report.execution.outcome).into());
        }
        let expected = match (report.output, report.failure) {
            (Some(out), None) => input.output_completion(&out.bytes)?,
            (None, Some(failure)) => input.failure_completion(&failure)?,
            _ => return Err("missing native result".into()),
        };
        fs::write(output.join(format!("{name}.request.bin")), input.bytes())?;
        fs::write(output.join(format!("{name}.expected.bin")), expected)?;
        names.push(name);
    }
    let unauthorized = Invocation::new(
        "unauthorized",
        &Command::Rename(RenameRequest {
            operation_id: "unauthorized".into(),
            card_id: "protected".into(),
            expected_revision: 1,
            title: "must-not-write".into(),
        }),
    )?;
    fs::write(output.join("content-command.bin"), unauthorized.bytes())?;
    fs::write(output.join("cases.json"), serde_json::to_vec(&names)?)?;
    host.disconnect(&connection)?;
    println!("Generated {} actual native guest vectors", names.len());
    Ok(())
}

fn dependency_vectors(output: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use morrow_core::plugin_package::{catalog::Catalog, registry::Registry};
    use morrow_plugin_runtime::{
        Cancellation, dynamic_dependencies::Context, instance_pool::Pool, manager::Manager,
        shared_objects::SharedObjects,
    };
    let root = tempfile::tempdir()?;
    let registry = Registry::open(
        &root.path().join("registry"),
        Catalog::open(&root.path().join("packages"))?,
    )?;
    let mut manager = Manager::new(registry, Limits::default());
    let mut host = HostRuntime::new(Store::open(
        &root.path().join("dependency.db"),
        Default::default(),
    )?)?;
    let mut pool = Pool::new(&host, Default::default())?;
    let provider = Package::decode(&fs::read("sdk/compat/guest-v1-rc1/rust-provider.mplugin")?)?;
    fs::write(
        output.join("dependency-provider.package.bin"),
        provider.archive(),
    )?;
    manager.install_package(provider.archive())?;
    manager.select(&provider, manager.revision())?;
    manager.set_enabled(
        &provider.manifest().package_id,
        provider.digest(),
        true,
        manager.revision(),
    )?;
    for language in ["rust", "c", "cpp"] {
        let caller = Package::decode(&fs::read(format!(
            "sdk/compat/guest-v1-rc1/{language}-dependency.mplugin"
        ))?)?;
        fs::write(
            output.join(format!("dependency-{language}.package.bin")),
            caller.archive(),
        )?;
        manager.install_package(caller.archive())?;
        manager.select(&caller, manager.revision())?;
        let id = &caller.manifest().package_id;
        manager.set_enabled(id, caller.digest(), true, manager.revision())?;
        manager.approve_dependency(
            id,
            caller.digest(),
            "reverse",
            &provider.manifest().package_id,
            provider.digest(),
            manager.revision(),
        )?;
        let revision = manager.revision();
        let session = pool.start(&mut manager, &mut host, id, &[], revision)?;
        let input = Invocation::new_transform(
            "frozen-web-dependency",
            Transform {
                handler: "bytes.dependency-wrap".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                input: b"abc\0\xff\x01".to_vec(),
            },
        )?;
        let mut objects = SharedObjects::new(&host, Default::default())?;
        let result = pool.run(
            &manager,
            &mut host,
            &mut objects,
            &session,
            &input,
            Context::new("qualification", 100),
            Default::default(),
            || 1,
            Cancellation::default(),
        )?;
        result.validate(&host, pool.root(&session)?.connection(), 1)?;
        fs::write(
            output.join(format!("dependency-{language}.request.bin")),
            input.bytes(),
        )?;
        fs::write(
            output.join(format!("dependency-{language}.expected.bin")),
            input.output_completion(result.bytes())?,
        )?;
        drop(result);
        pool.close(&mut host, &session)?;
    }
    Ok(())
}
