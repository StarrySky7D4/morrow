# 正文 API · test.11

运行期协议 v7；C ABI 的新类型为 `mp_content_request_v1`。正文是不透明二进制，宿主不解析第三方正文类型。所有写入由核心原子保存卡片、操作回执与事件；SDK 自身不访问磁盘。

## 操作与权限

| 操作 | Manifest 能力与实例授权 | 约束 |
| --- | --- | --- |
| CreateContent | CREATE_CONTENT / CreateContent | 精确卡片 ID；格式版本大于零；创建后修订为 1 |
| EditContent | EDIT_CONTENT / EditContent | 精确卡片 ID；必须匹配预期修订；一次增加 1 |
| ReadContent | READ_CONTENT / ReadContent | 精确卡片 ID；每个片段固定同一修订；读取后再次检查权限 |

插件不能自己授权。包中声明能力只是上限，实际宿主仍须对该实例、该卡片授予有效权限。创建没有默认读取权限；编辑不同时授予读取或附件权限。新建初始预览为空，可在后续编辑中提交预览。正文编辑保留类型、格式版本、附件、关联和未知外层字段；调用方负责保留自己正文格式的未知字段。

## Rust

```rust
use morrow_plugin_sdk::protocol::{Action, Request, Reply};
let request = Request {
    request_id: operation_id, // 跨重试保持同一个 ID
    card_id,
    action: Action::EditContent {
        revision: known_revision,
        title: "新标题".into(),
        body: vec![0, 255, 42],
        preview: "插件缺席时仍可读的预览".into(),
    },
};
let response = client.exchange(&request.encode()?)?;
match request.decode_reply(&response)? {
    Reply::ContentCommitted(receipt) => { /* 用权威修订更新投影 */ }
    Reply::Rejected(reason) => { /* 显示拒绝原因，保留草稿 */ }
    _ => unreachable!(), // decode_reply 已进行操作与类型匹配
}
```

## C11

```c
mp_content_request_v1 request = {0};
request.abi_version = 1;
request.struct_size = sizeof(request);
request.kind = MP_CONTENT_READ;
request.request_id = request_id;  /* UTF-8 mp_span */
request.card_id = card_id;
request.revision = known_revision;
request.offset = 0;
request.length = 32768;
/* mp_content_request_encode -> mp_exchange -> mp_content_reply_decode */
/* mp_reply_get 返回 MP_REPLY_CONTENT；最后 mp_reply_free。 */
```

正文 `body` 是带长度的字节 span，可以包含 NUL 或非 UTF-8 字节。文本字段必须为 UTF-8。所有借用 span 在调用期间保持有效。回复视图由不透明回复句柄持有，释放后不得继续访问。

## C++17

```cpp
auto request = morrow::content_request::read(
    "request-1", "card-1", known_revision, 0, 32768);
auto encoded = request.encode();
// 检查 encoded.status，再交给 morrow::client。
auto reply = morrow::decoded_reply::decode(request, response.bytes);
// 检查 reply.status()，然后查看 reply.view().kind。
```

请求拥有文本和正文字节；回复不可复制、可以移动，析构时释放句柄。不会因失败自动重试。

## 容量、关联与恢复

- 每条消息最多 64 KiB；单次创建／编辑正文最多 32 KiB。标题与预览各至多 16 KiB，但总消息限制优先。
- 核心更大的已有正文可分段读取，正文总上限 8 MiB，每段最多 32 KiB。偏移必须不超过总长度；末尾允许空片段。
- 每段都匹配卡片、修订、偏移和请求长度。收齐后，调用方核对完整正文 SHA-256；`ContentPart.sha256` 不是该片段哈希，也不是完整卡片哈希。
- 未提供大正文流式写入或 SDK 自动装配器，不会把截断正文伪装为完整内容。超限在提交前拒绝。
- 内容命令不能注入原生路径、任意附件引用或宿主句柄；文件导入仍由受控平台服务处理。
- 超时或传输失败不能证明回滚。保持原操作 ID，使用原有 QueryOperation 和相应授权查询；若重发同一操作，其内容必须一致。SDK 不把“快照不存在”解释为没有在途提交。
- 通用任务转发用 `mp_task_get_command`／`task.command()`；`mp_task_get_content`／`task.content()` 返回新类型的借用参数。返回值依旧不携带授权。

## 已验证与后续

已验证 Rust/C/C++ 的真实 Wasm → 任务宿主 → SQLite 链路，以及契约不匹配、旧修订、权限撤销／过期、重复操作和回复关联。旧 v6 包必须重新构建。后续继续补大正文写入会话、精确范围的集合查询、安装更新生命周期和跨平台运行验收。
