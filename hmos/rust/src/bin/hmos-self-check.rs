//! Exercises the actual OHOS linked core on an isolated device-side fixture.
use morrow_hmos::{Engine, Request};
use serde_json::{Value, json};
fn run(engine: &mut Engine, value: Value) -> Value {
    let request: Request = serde_json::from_value(value).unwrap();
    serde_json::to_value(engine.execute(request).unwrap()).unwrap()
}
fn main() {
    let base = std::env::args()
        .nth(1)
        .expect("provide NEW isolated fixture directory");
    let root = std::path::Path::new(&base);
    std::fs::create_dir(root).expect("fixture directory must not already exist");
    let path = root.join("hmos-development.sqlite");
    let mut engine = Engine::open(&path).unwrap();
    let first = run(
        &mut engine,
        json!({"action":"create","id":"native-card","operation":"create","title":"鸿蒙 Rust 真机链路","description":"SQLite + Protobuf + Rust","category":"进行中","stage":"计划中"}),
    );
    let add = json!({"action":"task_add","id":"native-card","operation":"add","source":first["cards"][0]["source"],"task_id":"task-1","text":"同名待办"});
    let second = run(&mut engine, add.clone());
    let third = run(
        &mut engine,
        json!({"action":"task_add","id":"native-card","operation":"add-2","source":second["cards"][0]["source"],"task_id":"task-2","text":"同名待办"}),
    );
    let fourth = run(
        &mut engine,
        json!({"action":"task_toggle","id":"native-card","operation":"toggle","source":third["cards"][0]["source"],"task_id":"task-1","flag":true}),
    );
    assert_eq!(fourth["cards"][0]["tasks"][0]["completion"], 1);
    assert_eq!(fourth["cards"][0]["tasks"][1]["completion"], 0);
    let mut stale = add.clone();
    stale["operation"] = json!("stale");
    assert!(
        engine
            .execute(serde_json::from_value(stale).unwrap())
            .unwrap_err()
            .contains("RevisionConflict")
    );
    let retried = run(&mut engine, add.clone());
    assert_eq!(retried["receipt_revision"], "2");
    assert_eq!(retried["cards"][0]["revision"], "4");
    drop(engine);
    let mut reopened = Engine::open(&path).unwrap();
    let persisted = run(&mut reopened, json!({"action":"list"}));
    assert_eq!(persisted["cards"][0]["revision"], "4");
    let replay = run(&mut reopened, add);
    assert_eq!(replay["receipt_revision"], "2");
    println!(
        "{}",
        json!({"result":"PASS","platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"checks":["create","independent-duplicate-task-id","stale-cas-rejected","historical-receipt-vs-current","reopen","retry-after-reopen"],"profile":"development-unsealed"})
    );
}
