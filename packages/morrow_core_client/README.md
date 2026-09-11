# Morrow core client — test.10

这是独立实验客户端包，尚未加入 Flutter 工作台依赖。原生与 Dart/Wasm 路径使用从 `core/schemas/runtime.capnp` 生成的类型绑定，普通 JavaScript 入口使用共享 Rust/Wasm 编解码器；正式重命名请求只传 Cap’n Proto 二进制。`BigInt` 承载完整无符号 64 位修订；编码时按同一 64 位补码位模式传给生成器，解码时恢复无符号值，不经过 JSON、double 或 JavaScript Number。

## 当前支持边界

- Windows Dart VM：真实 DLL／FFI 往返通过。缓冲区归 Rust 所有，Dart 同步独占并复制结果，成功和异常均释放输入／输出；最多 16 个缓冲区、每个最多 64 KiB。已验证限额、重复释放、释放后句柄失效、句柄不复用和连续错误输入后的零活跃缓冲区。
- Chrome 的 Dart/Wasm：真实 Dart/Wasm → Worker → Rust/Wasm → Dart/Wasm 往返通过，含 UInt64 最大值、Rust 生成的跨段向量及错误拒绝。Worker 示例只有一份可信核心实例，不是插件 runner 或安全沙箱验收。
- **普通 Dart→JavaScript 已提供独立入口 `package:morrow_core_client/web.dart`**：页面先初始化 core-web 的 wasm-bindgen Web 模块并赋给 `globalThis.morrowCodec`，再加载 Dart JavaScript。这里的 RenameCommand 将 BigInt 以十进制字符串转为 JS BigInt，由 Rust/Wasm 编解码；不导入存在 UInt64 schema 字面量限制的旧生成器。已在 Chrome 验证最大修订、跨段消息、Worker OPFS 提交及去重。旧包主入口仍不支持直接 compile js，不能混用两个 RenameCommand 导入。
- macOS、Linux、Android、iOS／iPadOS 和其他浏览器没有本轮运行证据。通用脚本中的动态库命名分支不是支持声明。

`NativeCoreBridge.roundTrip` 只做协议校验和规范化往返，不调用权限状态机、不修改卡片、不持久化。底层指针仅用于同进程可信客户端；持有者必须独占句柄，不允许在 process/free 时并发写入，也不得在释放后继续使用指针。Wasm 端每次分配／调用后重新取得 memory.buffer，输出先复制再释放。进程或 Worker 重启后所有旧句柄失效，不能持久化句柄。

协议版本为 6，请求携带运行期 schema 与内容 schema 的 SHA-256。摘要以 LF 标准化源契约计算，由生成脚本写入客户端；Rust 独立计算并验证，Dart 也校验返回消息。当前采取严格相等策略，不是完整的多版本能力协商。未知 Protobuf 内容保留由 Rust CardRecord 负责，运行期命令不能作为内容保存投影。

## 复现

先在本目录 `dart pub get` 安装锁定依赖。在项目根目录运行：

```powershell
python -X utf8 tool/generate_core_client.py
python -X utf8 tool/generate_core_client.py --check
pwsh -File tool/verify_core.ps1 -Web
pwsh -File tool/verify_core_client.ps1 -Web -Python python
```

需要 Flutter／Dart 3.12、Rust、Cap’n Proto compiler 1.4.0、Rust wasm32-unknown-unknown 目标、Node 和 Chrome／Chromium（可指定 CHROME_BIN）。编译和探针文件均落入 build/core-test.10。生成绑定纳入版本控制；`--check` 在临时目录生成并比较，不修改已有绑定。

验证脚本通过 Flutter 分析入口检查本包，以避开本机 Dart 3.12 独立 analyze 在退出时清理 perf socket 的竞态。编译、分析和测试任一失败仍中止，不跳过错误。

参考：[运行库](https://pub.dev/packages/capnproto_dart/versions/0.1.0)、[固定生成器](https://pub.dev/packages/capnpc_dart/versions/0.1.0)。两者为候选实现；发布者的平台标签不代替本项目实际测试结果。

Web 存储完整复现见 [core-web](../../core-web/README.md) 和 `tool/verify_web_storage.ps1`。页面桥接初始化、Worker 生命周期与消息串行化仍由可信第一方宿主负责；示例不是可直接托管不可信插件的通用分派器。

test.7 的 Web 入口新增 ReadSummaryCommand 与 RuntimeReply，重命名返回完整二进制响应后提取修订。RuntimeReply 检查请求关联 ID，错误响应与成功响应有明确种类；读取和写入授权分开。该 Dart 适配目前投影种类、修订、标题与错误类别，完整回执／摘要字段仍保留在 Rust 解码的响应内。test.8 原生 Dart 已通过 NativeHostSession 接入持久化分派。

## test.8 原生宿主 API

`native_bridge.dart` 新增 NativeHostSession、NativeHostConnection 和 NativeCapability。NativeHostSession 显式打开已存在的独立实验库，connect 建立连接，grant／revoke 是可信宿主管理面，dispatch 才接受 Cap’n Proto 消息。连接或宿主应显式 close；关闭后不可复用。

```dart
final host = NativeHostSession(libraryPath, existingDatabasePath);
final connection = host.connect();
try {
  connection.grant(NativeCapability.queryOperation, 'card-1');
  final query = QueryOperationCommand(
    requestId: 'query-1', cardId: 'card-1', operationId: 'edit-1');
  final reply = RuntimeReply.decode(
    connection.dispatch(query.encode()), requestId: query.requestId);
  // resultState is locallyCommitted or absentSnapshot; denied is a rejected reply.
} finally {
  connection.close();
  host.close();
}
```

原生 RuntimeReply 解码完整提交回执、摘要和受限操作结果，包括精确 BigInt 修订、事件 ID 与内容 SHA-256。Web 同样增加 QueryOperationCommand，当前 Web 投影提供结果状态、卡片／操作 ID、修订、标题和错误类别，完整字段投影仍待统一。请求关联 ID 错误会被拒绝。

NativeHostException 表示传输／控制面失败；RuntimeReply 的 rejected 是业务结果。未收到写入响应不能自动解释成未提交。本机 DLL 探针覆盖响应丢失后查询、提交前缓冲区预留、8 个宿主／128 个连接上限、撤权和到期、跨连接与陈旧句柄拒绝、关闭后零存活宿主／缓冲区、重启后重新授权及 UInt64 最大值；这不是跨进程插件隔离证据。

## test.9 附件读取

原生与 web.dart 均提供 ReadAttachmentCommand、RuntimeReply.attachmentPart 和 AttachmentTransferVerifier。NativeHostConnection.grantAttachment／revokeAttachment 为可信管理方法，按卡片和附件授权；BrowserStore 提供对应管理入口。

每次请求带相同的 expectedRevision 和当前 offset；length 默认 32768 且不能超限。响应的 AttachmentPart 持有独立、不可变的字节和摘要副本；UInt64 修订保持 BigInt。调用者将有序数据写入私有临时目标，同时传给校验器；仅在 finish 成功后发布完整文件。校验器不保存完整文件，拒绝乱序、混合修订、变更的总长／摘要和最终 SHA-256 不匹配。空文件也必须收到合法空响应；缺包不能提前完成。

原生探针通过真实 DLL 读取 5 MiB＋17 字节（161 包）；普通 Dart JS／Chrome 读取 100000 字节（4 包）。两者均检查原字节与整件摘要、撤权／到期及修订变化。单元测试另行覆盖损坏、乱序与不可变副本；这些证据不表示 Web 大文件导入、生产导出 UI 或插件隔离已经完成。test.9 当时使用运行期 v5／实验库格式 3；当前版本见下节，没有旧数据库自动迁移。

## test.10 契约同步

内容契约为工作区、视图位置和草稿补充独立修订，运行期版本升为 v6 并更新固定摘要。既有原生／Web 客户端命令回归保留；三类新记录目前由 Rust 可信本地事务 API、CLI 和 BrowserStore 管理入口访问，尚未加入 Dart 通用分派接口。实验数据库格式 4，没有旧实验库自动迁移。

声明式 UI 契约 v1 增量：Rust／Dart 共用有界表单和事件 schema，Rust 宿主检查视图代次、修订、序号、节点和动作；原生及浏览器 Dart/Wasm 消息往返通过。见 [UI 消息协议](../../docs/PLUGIN_UI_PROTOCOL.md)。基础 Flutter 渲染器已建立，见 [控件渲染与接入边界](../../docs/PLUGIN_UI_RENDERER.md)；三语言 guest UI 构造器、包 UI 注册和实际插件事件调度仍待实现，M6 尚未完成。
