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
    let raw = |text: &str| {
        json!({"text":text,"selection_base":1,"selection_extent":2,
        "affinity":0,"directional":false,"composing_start":1,"composing_end":2})
    };
    let values = json!({"title":raw("A😀B"),"description":raw("Raw native draft"),
        "hypothesis":raw("Hypothesis"),"conclusion":raw("Conclusion"),"todos":raw("Todo"),
        "category":"实验","stage":"待验证"});
    let proposal = json!({"action":"draft_save","draft":{"card_id":"unsubmitted-native",
        "draft_id":"native-draft","source_kind":1,"source_revision":"0","expected_generation":"0",
        "operation_id":"native-draft-save","values":values}});
    let first_draft = run(&mut reopened, proposal.clone());
    assert_eq!(first_draft["effect"], "committed");
    assert_eq!(first_draft["drafts"][0]["generation"], "1");
    assert_eq!(first_draft["drafts"][0]["values"], values);
    assert_eq!(
        run(&mut reopened, json!({"action":"list"}))["cards"],
        completed["cards"]
    );
    drop(reopened);
    let mut reopened = Engine::open(&path).unwrap();
    let draft = run(
        &mut reopened,
        json!({"action":"draft_read","id":"unsubmitted-native","draft_id":"native-draft"}),
    );
    assert_eq!(draft["drafts"][0]["values"], values);
    assert_eq!(draft["effect"], "not_committed");
    let mut successor = proposal.clone();
    successor["draft"]["operation_id"] = json!("native-draft-save-2");
    successor["draft"]["expected_generation"] = json!("1");
    successor["draft"]["values"]["title"]["text"] = json!("A😀B newer");
    assert_eq!(
        run(&mut reopened, successor)["drafts"][0]["generation"],
        "2"
    );
    let historical = run(&mut reopened, proposal.clone());
    assert_eq!(historical["drafts"][0]["generation"], "1");
    assert_eq!(historical["drafts"][0]["current_generation"], "2");
    assert_eq!(historical["drafts"][0]["repeated"], true);
    let discard = json!({"action":"draft_discard","id":"unsubmitted-native","draft_id":"native-draft",
        "generation":"2","operation":"native-draft-discard"});
    assert_eq!(
        run(&mut reopened, discard.clone())["drafts"][0]["active"],
        false
    );
    assert_eq!(run(&mut reopened, discard)["drafts"][0]["repeated"], true);
    assert_eq!(
        run(&mut reopened, json!({"action":"draft_list"}))["drafts"],
        json!([])
    );
    let historical = run(&mut reopened, proposal);
    assert_eq!(historical["drafts"][0]["current_generation"], "3");
    assert_eq!(historical["drafts"][0]["current_active"], false);
    assert_eq!(
        run(&mut reopened, json!({"action":"list"}))["cards"],
        completed["cards"]
    );
    println!(
        "{}",
        json!({"result":"PASS","platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"checks":["create","independent-duplicate-task-id","stale-cas-rejected","historical-receipt-vs-current","reopen","retry-after-reopen","rename-by-id","reorder-preserves-completion","complete-all-and-stage","reorder-replay-after-reopen","query-complete-properties","query-stage","query-completed-tasks-empty","draft-full-raw-values","draft-never-creates-business-card","draft-reopen-read-only","draft-historical-generation","draft-discard-and-exact-retry","draft-active-list-empty","draft-inactive-current-vs-historical"],"profile":"development-unsealed"})
    );
}
