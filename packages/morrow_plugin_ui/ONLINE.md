# 在线插件表单

`lib/online.dart` 提供 `PluginUiTransport`、`PluginUiController` 和 `ManagedPluginForm`。它把真实 Flutter 输入转换为 Cap’n Proto `UiEvent`，等待异步宿主结果，再严格解码 `UiDocument` 并重新渲染。它不提供权限，不确认内容已经保存。

## 传输适配约定

```dart
abstract interface class PluginUiTransport {
  Future<PluginUiReply> open(String seed);
  Future<PluginUiReply> event(Uint8List bytes);
  Future<void> close();
}
```

每个 transport 实例必须固定拥有一个宿主 UI 会话。适配器可以在方法内部执行 submit + poll，但每个 Future 只返回该操作的最终结果。关闭旧 transport 不能关闭之后创建的新会话；适配器应以真实宿主会话身份执行关闭。传输层负责自己的请求期限、消息上限及断线结束，不能让已经失效的请求永久悬挂。

`PluginUiReply` 构造参数：

- `String view`
- `BigInt generation, revision, serial`
- `Uint8List? documentBytes`
- `PluginUiFailure? failure`

正文与 failure 必须恰有一个。`PluginUiFailure(kind, message)` 的 kind 为 `busy / rejected / plugin / execution / unavailable`。回执计数必须来自宿主当前状态；Dart 编号只是路由提案，仍由 Rust 接纳并校验。

初次打开的成功回执为 revision=1、serial=0。事件成功时 revision 必须增加 1，serial 必须等于此次提出的序号。明确失败时 revision 不变；未接纳事件保留旧 serial，接纳后执行失败消费此次 serial。事件回执不得更换 view 或 generation。

连接断开、操作结果未知时应抛出错误，不能伪造成“未接纳”。controller 保留最后文档及本地输入，进入 interrupted 状态并停止提交；调用者需关闭并创建新的 controller / transport，不自动重试原事件。

```dart
final controller = PluginUiController(transport);
await controller.open('初始标题');
// 将 ManagedPluginForm(controller: controller) 放入有边界的界面区域。
// 页面结束时调用 controller.dispose()；需要等待宿主关闭时先 await controller.close()。
```

transport、controller 都由调用者持有。ManagedPluginForm 不隐式打开、重开或释放 controller，便于在页面切换中明确管理宿主所有权。更换 controller 即更换视图会话，旧 Future 及旧控件回调不能更新新界面。`viewIdentity` 同时包含 controller 实例与宿主 view/generation。

## 交互与失败

最多一个异步操作在途。按钮和开关忙碌期间禁用且不排队；文本输入保持可用，每节点至多保留一条最新已提交文本。连续键入会合并未发送的中间文本，不重放已经接纳的事件。主机正常回执后，下一条最新文本使用新 revision/serial 编码。主机结果不会覆盖用户在等待期间继续输入的更新草稿。

明确失败会保留最后有效文档与本地草稿、清空后续自动发送队列并显示失败信息。草稿可见不代表已经提交，也不会因为失败被自动重新提交；用户下一次修改才发出新的事件。主机坏文档、错误身份、错误计数及传输未知结果使控制器停止发送。

输入法组合过程不发送事件，主机回复也不清除正在组合的文字和选区。组合提交后再发送有效文本；原始 PluginForm 的 UTF-8 上限和控制字符校验继续生效。宿主节点、动作、类型与版本校验仍是权限边界，Dart 的预检仅用于交互与有界输入。

`PluginForm.actionsEnabled` 默认 true，保留既有使用方式；在线托管控件在忙碌期间仅关闭离散动作，文本编辑不因此失焦。

## 本轮验证与边界

2026-09-12，在 Windows 上：`flutter analyze --no-pub` 零问题；包内 `flutter test --no-pub` **17 项通过**，其中新增 6 项在线控件测试：

1. 实际 PluginForm 输入 → 实际 UiEvent 编码 → 异步结果 → 实际 UiDocument 解码 → 重渲染；验证单在途和文本合并。
2. IME 组合期间宿主回执保持组合状态，提交后才发送新事件。
3. 接纳后业务失败保留文档／草稿、清空队列、不自动重试，之后新输入使用正确序号恢复。
4. 替换 controller/generation 拒绝旧回调、旧 Future 结果，关闭旧 transport。
5. 断线结果未知、错误 generation、坏文档阻止继续发送。
6. 旧 revision 与忙碌的按钮请求不进入传输。

这些包内测试使用可控 fake transport 注入异步时序，但编解码和 Flutter 编辑器均是真实实现。不能据此宣称已经通过真实 Windows Rust/Wasm 宿主、主应用或浏览器的全链验证。实际宿主适配和主应用集成由单独步骤验收；本模块没有修改核心授权与持久协议。
