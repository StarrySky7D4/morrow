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
        json!({"action":"create","id":"native-card","operation":"create","title":"鸿蒙 Rust 真机链路","description":"SQLite + Protobuf + Rust","hypothesis":"Native Hypothesis Needle","conclusion":"Native Conclusion Needle","category":"进行中","stage":"计划中"}),
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
    let renamed = run(
        &mut reopened,
        json!({"action":"task_rename","id":"native-card","operation":"rename","source":persisted["cards"][0]["source"],"task_id":"task-2","text":"重命名步骤"}),
    );
    let reorder = json!({"action":"task_reorder","id":"native-card","operation":"reorder","source":renamed["cards"][0]["source"],"order":["task-2","task-1"]});
    let reordered = run(&mut reopened, reorder.clone());
    assert_eq!(reordered["cards"][0]["tasks"][0]["id"], "task-2");
    assert_eq!(reordered["cards"][0]["tasks"][0]["text"], "重命名步骤");
    assert_eq!(reordered["cards"][0]["tasks"][0]["completion"], 0);
    assert_eq!(reordered["cards"][0]["tasks"][1]["completion"], 1);
    let completed = run(
        &mut reopened,
        json!({"action":"task_complete_all","id":"native-card","operation":"complete","source":reordered["cards"][0]["source"],"stage":"已完成"}),
    );
    assert_eq!(completed["cards"][0]["stage"], "已完成");
    assert_eq!(completed["cards"][0]["tasks"][0]["completion"], 1);
    assert_eq!(completed["cards"][0]["tasks"][1]["completion"], 1);
    drop(reopened);
    let mut reopened = Engine::open(&path).unwrap();
    let replay = run(&mut reopened, reorder);
    assert_eq!(replay["receipt_revision"], "6");
    assert_eq!(replay["cards"], completed["cards"]);
    for text in ["hypothesis needle", "conclusion needle"] {
        let result = run(
            &mut reopened,
            json!({"action":"query","section":"概览","filter":"全部","text":text,"sort":"标题排序"}),
        );
        assert_eq!(result["ids"], json!(["native-card"]));
        assert_eq!(result["cards"], json!([]));
        assert_eq!(result["receipt_revision"], "");
        assert_eq!(result["effect"], "not_committed");
    }
    let stage = run(
        &mut reopened,
        json!({"action":"query","section":"小项目","filter":"已完成","text":"","sort":"最近添加"}),
    );
    assert_eq!(stage["ids"], json!(["native-card"]));
    let pending = run(
        &mut reopened,
        json!({"action":"query","section":"概览","filter":"有待办","text":"","sort":"收藏优先"}),
    );
    assert_eq!(pending["ids"], json!([]));
    println!(
        "{}",
        json!({"result":"PASS","platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"checks":["create","independent-duplicate-task-id","stale-cas-rejected","historical-receipt-vs-current","reopen","retry-after-reopen","rename-by-id","reorder-preserves-completion","complete-all-and-stage","reorder-replay-after-reopen","query-complete-properties","query-stage","query-completed-tasks-empty"],"profile":"development-unsealed"})
    );
}
