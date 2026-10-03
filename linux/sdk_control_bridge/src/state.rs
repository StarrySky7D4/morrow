//! Ordinary temporary SQLite and a non-original channel declaration fixture.
//! Neither is protected storage, a production owner or a mapping substitute.
use crate::{Result, frozen};
use morrow_core::{
    channel::{Budget, Kind},
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{Package, catalog::Catalog, proto::TransformHandler, registry::Registry},
    response::Outcome,
    runtime::{Command, RenameRequest},
    store::{EventBudget, Store},
    task::Invocation,
    transaction::Lookup,
};
use morrow_plugin_runtime::{
    Fault, Limits,
    channel::{ChannelBroker, Source},
    manager::{ManagedInstance, Manager},
};
use morrow_workbench_host::product_gate::ProductGate;
use std::{path::Path, sync::Arc, time::Instant};
pub struct State {
    pub manager: Manager,
    pub host: HostRuntime,
    pub task: Arc<ManagedInstance>,
    pub transform: Arc<ManagedInstance>,
    pub channel: Arc<ManagedInstance>,
    pub foreign: Arc<ManagedInstance>,
    pub broker: ChannelBroker,
    pub gate: ProductGate,
    pub start: Instant,
    pub task_id: String,
}
fn selected(manager: &mut Manager, p: &Package) -> Result<()> {
    manager.select(p, manager.revision())?;
    manager.approve(
        &p.manifest().package_id,
        p.digest(),
        p.capabilities().clone(),
        manager.revision(),
    )?;
    manager.set_enabled(
        &p.manifest().package_id,
        p.digest(),
        true,
        manager.revision(),
    )?;
    Ok(())
}
impl State {
    pub fn new(repo: &Path, root: &Path, language: &str, gate: ProductGate) -> Result<Self> {
        let task_package = frozen::load(repo, language, "task")?;
        let transform_package = frozen::load(repo, language, "transform")?;
        let wasm = wat::parse_str(
            r#"(module
   (import "morrow_channel_v1" "call" (func (param i32 i32 i32 i32)(result i32)))
   (memory (export "memory") 4)
   (func (export "morrow_run")(result i32)i32.const 0))"#,
        )?;
        let mut manifest = Package::manifest_for_transform(
            "org.fixture.linux.sdk.bridge.channel",
            "1.0.0",
            &wasm,
            vec![TransformHandler {
                handler: "bridge.channel".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                max_input_bytes: 32768,
                max_output_bytes: 32768,
            }],
        );
        manifest
            .required_features
            .push(morrow_core::channel::FEATURE.into());
        manifest.channel_declaration = Some(morrow_core::channel::declaration(
            vec!["bridge.channel".into()],
            vec![Kind::ByteStream],
        ));
        let declaration = Package::build(manifest, &wasm)?;
        let catalog = Catalog::open(&root.join("catalog"))?;
        for p in [&task_package, &transform_package, &declaration] {
            catalog.install(p)?;
        }
        let registry = Registry::open(&root.join("registry"), catalog)?;
        let mut manager = Manager::new(registry, Limits::default());
        for p in [&task_package, &transform_package, &declaration] {
            selected(&mut manager, p)?;
        }
        let mut store = Store::open(
            &root.join("ordinary-fixture.sqlite"),
            EventBudget::default(),
        )?;
        store.create_local(
            "seed",
            &CardRecord::new("bridge-card", "note", 1, "before", vec![])?,
        )?;
        let mut host = HostRuntime::new(store)?;
        let task_id = task_package.manifest().package_id.clone();
        let mut original = manager.connect(&task_id, &mut host)?;
        let mut foreign = manager.connect(&task_id, &mut host)?;
        for instance in [&mut original, &mut foreign] {
            host.grant(
                instance.parts_mut().1,
                GrantKind::Rename,
                "bridge-card",
                10_000,
                1,
            )?;
        }
        let task = Arc::new(original);
        let foreign = Arc::new(foreign);
        let transform =
            Arc::new(manager.connect(&transform_package.manifest().package_id, &mut host)?);
        let channel = Arc::new(manager.connect(&declaration.manifest().package_id, &mut host)?);
        let budget = Budget {
            max_channels: 1,
            max_frame_bytes: 32768,
            max_bytes: 1024 * 1024,
            max_messages: 128,
            max_requests: 4096,
            max_duration_ms: 10_000,
        };
        let broker = manager.bind_channel(
            &host,
            &channel,
            declaration.digest(),
            manager.revision(),
            Source {
                kind: Kind::ByteStream,
                duplex: true,
                checkpoint_scope: None,
            },
            budget,
            10_001,
            1,
        )?;
        let originals = [task.clone(), transform.clone(), channel.clone()];
        gate.register(Arc::new(move || {
            for instance in &originals {
                instance.request_stop();
            }
        }))
        .map_err(|error| error.to_string())?;
        for instance in [&task, &transform, &channel, &foreign] {
            let l = instance.package().limits();
            if l.fuel != 20_000_000 || l.memory_bytes != 16 * 1024 * 1024 || l.host_calls != 16 {
                return Err("original budgets changed".into());
            }
        }
        Ok(Self {
            manager,
            host,
            task,
            transform,
            channel,
            foreign,
            broker,
            gate,
            start: Instant::now(),
            task_id,
        })
    }
    pub fn tick(&self) -> u64 {
        1 + self.start.elapsed().as_millis() as u64
    }
    pub fn runtime_call(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        let command = Command::decode(bytes)?;
        let invocation = Invocation::new("bridge-original-task", &command)?;
        let now = self.tick();
        let report = self.task.run_task(&mut self.host, &invocation, || now);
        if report.execution.outcome != Ok(0) || report.execution.host_calls != 1 {
            return Err(format!("original task execution: {:?}", report.execution).into());
        }
        Ok(report
            .response
            .ok_or("missing original response")?
            .encode()?)
    }
    pub fn transform_call(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        let invocation = Invocation::decode(bytes)?;
        let now = self.tick();
        let r = self.transform.run_task(&mut self.host, &invocation, || now);
        if r.execution.outcome != Ok(0) || r.execution.host_calls != 0 || r.response.is_some() {
            return Err(format!("original transform execution: {:?}", r.execution).into());
        }
        Ok(r.output.ok_or("missing transform output")?.bytes)
    }
    pub fn stop_proof(&mut self) -> Result<String> {
        if !self.gate.closed() {
            return Err("gate still open".into());
        }
        let revision = self
            .host
            .store_local()
            .card("bridge-card")?
            .ok_or("card missing")?
            .summary()
            .revision;
        let cancelled = Command::Rename(RenameRequest {
            operation_id: "after-loss-original".into(),
            card_id: "bridge-card".into(),
            expected_revision: revision,
            title: "must not commit".into(),
        });
        let input = Invocation::new("after-loss-original", &cancelled)?;
        let r = self.task.run_task(&mut self.host, &input, || 1);
        if !matches!(
            r.execution.outcome,
            Err(Fault::Cancelled | Fault::InactiveConnection)
        ) || r.execution.host_calls != 0
        {
            return Err(format!("original control not stopped: {:?}", r.execution).into());
        }
        let original_fault = r.execution.outcome.clone();
        if self
            .host
            .store_local()
            .lookup_for_card("bridge-card", "after-loss-original")?
            != Lookup::Absent
        {
            return Err("cancelled task committed".into());
        }
        let foreign = Command::Rename(RenameRequest {
            operation_id: "foreign-after-loss".into(),
            card_id: "bridge-card".into(),
            expected_revision: revision,
            title: "foreign remains live".into(),
        });
        let input = Invocation::new("foreign-after-loss", &foreign)?;
        let now = self.tick();
        let r = self.foreign.run_task(&mut self.host, &input, || now);
        if r.execution.outcome != Ok(0)
            || !matches!(r.response.map(|r| r.outcome), Some(Outcome::Renamed(_)))
        {
            return Err("foreign control was stopped".into());
        }
        let late = Arc::new(self.manager.connect(&self.task_id, &mut self.host)?);
        let stop = late.clone();
        if self
            .gate
            .register(Arc::new(move || stop.request_stop()))
            .is_ok()
        {
            return Err("late registration admitted".into());
        }
        let r = late.run_task(&mut self.host, &input, || 1);
        if !matches!(
            r.execution.outcome,
            Err(Fault::Cancelled | Fault::InactiveConnection)
        ) || r.execution.host_calls != 0
        {
            return Err(format!("late worker not stopped: {:?}", r.execution).into());
        }
        let late_fault = r.execution.outcome.clone();
        Ok(format!(
            "original-rejection={original_fault:?};late-rejection={late_fault:?};original-managed-cancelled=true;original-after-loss-host-calls=0;foreign-original-guest-task-live=true;late-registration-stopped=true;no-automatic-replay=true"
        ))
    }
}
