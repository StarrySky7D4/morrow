# IO-D1 第一步：HTTP 提交与结果帧编解码

日期：2026-09-17。结果：**PASS_SCOPED（仅第一步）**。范围：`morrow-core 0.1.9-test.50` 新增 HTTP 提交与结果帧的严格编解码。**IO-D1 整体未完成**：真实出站、origin／方法／凭据授权、重定向／取消／Unknown 核对与证据接入仍属未完项。已推送 `track-a/w1-io-contract`（`d086eaf`），未发布。

## 实际交付

- `core/src/io.rs`：`Header`、`HttpSubmission`、`HttpOutcome` 类型；`validate_http_submission`／`validate_http_outcome`；`Action::SubmitHttp`；`Request::encode_http_submit`；`Response::encode_http`／`decode_http`。
- 解码路径对 `Submission.httpRequest` 做与编码相同的校验，其余提交类型仍是 `Unsupported(Submit)`；读配置的 `Response::decode` 显式拒绝把 HTTP 提交当作读成功。
- `plugin_runtime/src/file_io.rs`：固定读取 broker 显式拒绝 `SubmitHttp` 为 `Unsupported`。
- 测试：`core/tests/io_codec.rs` 新增 4 项（往返与绑定、请求校验、结果校验与帧绑定、手工构造帧的解码校验），并调整既有 12 变体测试以排除已实现的 HTTP 变体。

## 关键不变量

| 项 | 规则 |
| --- | --- |
| 方法 | 非空、≤16、仅大写 ASCII |
| 目标 | origin-form：`/` 开头、≤2048、无 `://`、无 `#`、无控制字符与空白 |
| 请求头 | token 名；值无 CR/LF/NUL；≤64 个、名字 ≤128、单值 ≤8 KiB、总量 ≤16 KiB |
| 运行时头 | `host`／`connection`／`content-length`／`transfer-encoding`／`upgrade`／`te`／`trailer`／`expect`／`proxy-*`／`keep-alive` 一律拒绝 |
| 正文／凭据／期限 | ≤64 KiB／≤4 KiB（引用）／≤30 000 ms |
| 结果 | 4xx/5xx 仍为 `Completed` 且带真实 `http_status`；非完成状态不得携带远端字段 |
| 绑定 | 结果必须匹配 `callId` 与请求摘要；手工构造帧在解码时同样校验 |

## 验证证据

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --test io_codec` | **19 通过，0 失败**（+4） |
| 核心全量 `cargo test --offline` | **416 通过，0 失败**（412 → 416） |
| 核心故障注入 `cargo test --offline --features fault-injection` | **455 通过，0 失败** |
| 核心 clippy `cargo clippy --offline --all-targets --features fault-injection -- -D warnings` | 无警告 |
| 运行时全量 `cargo test --offline --features packages` | **284 通过，0 失败**（file broker 显式拒绝新动作） |

日志：`build/io-d1-codec-test.log`、`build/io-d1-core-default.log`、`build/io-d1-core-fault.log`、`build/io-d1-runtime-default.log`。

## 发现与修复

- 首版在 `init_headers` 后再次使用同一 capnp builder 导致移动错误；改为 `reborrow`。
- 常量重复转型触发 strict clippy；改为直接引用运行时上限。
- 既有“全部提交变体均不支持”测试需排除已实现的 HTTP 变体，并保留手工构造帧的拒绝覆盖。

## 仍未完成（IO-D1 剩余）

真实第三方 HTTP/HTTPS 出站与 `network_node` 集成、origin／方法／凭据的可撤回批准模型、DNS 钉定与重定向策略、响应上限与流式边界、超时／取消／远端已执行但响应丢失的 Unknown 核对、IO-C1／IO-C2 证据与意图接入、guest 提交/轮询 ABI、UI 与 SDK。见[提交帧说明](../docs/PLUGIN_HTTP_SUBMISSION.md)。
