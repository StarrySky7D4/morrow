# IO-D1 第一步：HTTP 提交与结果帧编解码

状态：核心协议层已实现，限定回归与边界见 [整合修正报告](../reports/io-safety-refactor-2026-09-19.md)；这是 IO-D1 的第一步，**不表示真实出站已经可用**。真实网络路径、origin／方法／凭据授权与 Unknown 核对仍未接入。

## 模型

[io.capnp](../core/schemas/io.capnp) 早已定义 `Submission.httpRequest` 与 `Response.httpStatus/headers`；本步把这些字段变成可验证的 Rust 类型：

- `HttpSubmission`：`operation_id`、`deadline_ms`、`endpoint`、`method`、`relative_target`、`headers`、`body`、`credential`。`endpoint` 与 `credential` 都是**不透明宿主引用**，不是 URL、origin 或秘密；`operation_id` 是稳定操作身份，不是授权句柄。
- `HttpOutcome`：`status`、`http_status`、`headers`、`body`。远端 4xx/5xx 仍是 `Completed` 与真实状态；只有传输结果不明才用 `OutcomeUnknown`。
- `Request::encode_http_submit` / `Action::SubmitHttp`：编码后再解码校验，得到精确帧摘要。
- `Response::encode_http` / `Response::decode_http`：绑定 `callId` 与请求摘要，读取前重新验证。

## 编解码不变量

- 方法只允许非空大写 ASCII token；目标是 origin-form（以单个 `/` 开头、不含反斜杠、`://`、`#`、控制字符与空白）。
- 请求头：名字为 HTTP token，值拒绝除 HTAB 外的 C0 控制字节及 DEL；数量 ≤64、名字 ≤128、单值 ≤8 KiB、总量 ≤16 KiB。
- `authorization`、`cookie`、全部 `proxy-*`、`host`、`connection`、`content-length`、`transfer-encoding`、`upgrade`、`te`、`trailer`、`expect`、`keep-alive` 由运行时掌控，guest 提交一律拒绝——避免改写被钉定的 origin 或分帧。
- 正文 ≤64 KiB，凭据引用 ≤4 KiB，操作引用 ≤256 字节，期限 ≤运行时的 30 秒上限。
- `Invalid` 结果状态在编码与解码两侧均拒绝；失败状态不得携带远端响应字段。
- 解码路径与编码路径执行同一套校验，手工构造的帧不能绕过；结果头只做形式与大小校验，因为它们是远端数据。

## 运行时仍需完成

1. 把 `endpoint`／`credential` 引用解析为具体 origin 与方法，并在 Manager／Registry 中建立可撤回的 origin／方法／凭据批准模型。
2. 通过 `network_node::client::Client` 执行真实 HTTP/HTTPS：DNS 全量检查后钉定连接、禁止隐式代理与重定向、按响应上限读取。
3. 把超时、取消与传输失败映射为 `OutcomeUnknown` 并接入 IO-C1／IO-C2 的意图、原件与核对；重定向、取消与远端已执行但响应丢失都必须可解释。
4. guest 侧 `morrow_io_v1.call` 的提交/轮询帧与证据保留、UI 状态和 SDK 类型化接口。

## 边界

本步不发起任何网络请求，不创建授权，不保存证据，也不改变冻结 SDK。本 HTTP 编解码不改变数据库格式；本轮单独引入的材料存储迁移见 IO-C1。字节头值通过本层并不替代未来实际 HTTP 后端的格式验证。
