# Codex 实验内容接口

`extensions/agent-content-v1` 提供独立的 Rust 内容请求与可信宿主适配。它复用原 Core `HostRuntime`、`Connection`、对象授权、修订 CAS 和持久操作记录，没有修改旧 agent-host v1、NativeSession v2、HTTP stream v3 或 SDK 兼容原件。当前是实验候选，完整 M05、SDK26 和 Codex 产品资格仍 OPEN。

| 请求 | 当前可调用范围 |
|---|---|
| `Query` | 按请求顺序查询最多16个唯一卡片ID的摘要；ID须在可信宿主事先指定的有限集合中，每项还须有原ReadSummary授权。没有通配符、SQL或全库扫描。 |
| `ReadRef` | 固定card、revision、正文总长度及完整SHA256，每次读取最多32KiB正文；逐次及最终交付复核原ReadContent授权。 |
| `ProposeMutation` | 固定完整原Core EditContent帧及源ContentRef，在进程内登记提案，不写内容库、不批准、不执行。原ReadContent与EditContent授权须同时有效。 |
| `InspectOperation` | 精确card／operation只读查询，复用原QueryOperation授权；返回AbsentSnapshot或原持久回执，不重新授予写权限或自动重放。 |

客户端先构造 `Request::new`，或用 `Request::decode` 接受原请求。请求对象保留原始字节；`digest()` 是完整原帧的SHA256。回复使用 `Reply::decode_for(bytes, &request)` 核对新schema、原Core契约、完整原请求摘要、请求ID、回复种类、对象、版本、长度、offset和正文摘要。重复提案必须保留完整原帧；相同operation更换外层requestId、源引用或编辑内容会冲突。

`client::read_complete` 逐段调用ReadRef，并在全部正文的总长度与SHA256验证通过后才返回私有构造的 `VerifiedContent`。调用方显式设置内存预算；超预算不会发出读取，空正文也须执行一次获准读取，任一错误立即停止且不自动重试。分段回复中的完整摘要只是元数据，不能把未收齐、未核验的正文标为完整下载。`VerifiedContent` 证明字节符合指定引用，不授予保存或外发权限。

可信宿主用 `ContentHost::new(&runtime, &connection, nominated_cards)` 绑定原runtime与connection。对有效帧调用 `dispatch` 只会执行上述四项请求。提案摘要绑定完整原请求；宿主在单独审查后调用 `approve`，再显式调用 `execute_approved`。两个方法都不是wire命令，没有guest approval bit。批准不延长原grant、不改变包能力上限，也不能恢复已撤销的授权。

执行前重新检查原ReadContent与EditContent授权、源引用和当前版本。适配器在进入原Core提交路径前单向消耗提案，随后由原事务执行CAS与最终授权检查；相同提案不会再次派发。精确持久回执交付成功才标记LocallyCommitted。若提交后回执丢失或晚期授权拒绝，保留DispatchUnknown；历史查询可核对已保存回执，但不把Unknown转换为重放许可。进程重启不恢复旧提案批准，最多32个session内提案与已消耗记录保持有界。

完整开发示例在 [content_roundtrip.rs](../extensions/agent-content-v1/examples/content_roundtrip.rs)。它使用一次性普通SQLite合成库，实际执行Query、ReadRef、提案、独立批准、一次CAS提交和历史核对。仓库根目录运行：

```bash
cargo run --locked --offline \
  --manifest-path extensions/agent-content-v1/Cargo.toml \
  --target-dir build/agent-content-v1-target --example content_roundtrip
```

本云环境先运行 `source /workspace/.morrow-tools/activate.sh`。其他环境须提供Rust及Cap'n Proto 1.4.0与锁定依赖。全部该扩展测试：

```bash
cargo test --locked --offline \
  --manifest-path extensions/agent-content-v1/Cargo.toml \
  --target-dir build/agent-content-v1-target
```

当前实际60个方法通过：codec29、原HostRuntime／临时普通Store17、完整读取client14。另有原Core回归24通过，四操作示例实际运行成功；没有把zero-test unit／doc runner计入60。

本片段只支持卡片正文与现有小型EditContent。附件、Create／Rename、跨workspace复杂查询、内容到获准endpoint的外发交集、持久提案恢复、native IPC协商、package feature／import准入、C／C++绑定及生产UI仍需独立接入。HTTP grant不授予内容披露权限，旧native合同也不会因新codec出现而协商此能力。没有使用普通Store替代受保护Session。当前实测范围及剩余接口见 [缺口与补齐报告](../reports/reconstruction-2026-10-05/codex-sdk-interface-gaps.md)。
