# Morrow core client — test.5

这是独立实验客户端包，尚未加入 Flutter 工作台依赖。它使用从 `core/schemas/runtime.capnp` 生成的类型绑定，正式消息只传 Cap’n Proto 二进制。`BigInt` 承载完整无符号 64 位修订；编码时按同一 64 位补码位模式传给生成器，解码时恢复无符号值，不经过 JSON、double 或 JavaScript Number。

## 当前支持边界

- Windows Dart VM：真实 DLL／FFI 往返通过。缓冲区归 Rust 所有，Dart 同步独占并复制结果，成功和异常均释放输入／输出；最多 16 个缓冲区、每个最多 64 KiB。已验证限额、重复释放、释放后句柄失效、句柄不复用和连续错误输入后的零活跃缓冲区。
- Chrome 的 Dart/Wasm：真实 Dart/Wasm → Worker → Rust/Wasm → Dart/Wasm 往返通过，含 UInt64 最大值、Rust 生成的跨段向量及错误拒绝。Worker 示例只有一份可信核心实例，不是插件 runner 或安全沙箱验收。
- **普通 Dart→JavaScript 编译未通过**：当前固定的 capnpc_dart 0.1.0 生成 UInt64 schema ID 整数字面量，无法在 JavaScript 中精确表示。不能把该包直接接入现有 Flutter JavaScript 产物。后续比较单独 Wasm 协议适配与支持精确整数的替代生成链，避免为了编译通过而截断 ID。
- macOS、Linux、Android、iOS／iPadOS 和其他浏览器没有本轮运行证据。通用脚本中的动态库命名分支不是支持声明。

`NativeCoreBridge.roundTrip` 只做协议校验和规范化往返，不调用权限状态机、不修改卡片、不持久化。底层指针仅用于同进程可信客户端；持有者必须独占句柄，不允许在 process/free 时并发写入，也不得在释放后继续使用指针。Wasm 端每次分配／调用后重新取得 memory.buffer，输出先复制再释放。进程或 Worker 重启后所有旧句柄失效，不能持久化句柄。

协议版本为 2，请求携带运行期 schema 与内容 schema 的 SHA-256。摘要以 LF 标准化源契约计算，由生成脚本写入客户端；Rust 独立计算并验证，Dart 也校验返回消息。当前采取严格相等策略，不是完整的多版本能力协商。未知 Protobuf 内容保留由 Rust CardRecord 负责，运行期命令不能作为内容保存投影。

## 复现

先在本目录 `dart pub get` 安装锁定依赖。在项目根目录运行：

```powershell
python -X utf8 tool/generate_core_client.py
python -X utf8 tool/generate_core_client.py --check
pwsh -File tool/verify_core.ps1 -Web
pwsh -File tool/verify_core_client.ps1 -Web -Python python
```

需要 Flutter／Dart 3.12、Rust、Cap’n Proto compiler 1.4.0、Rust wasm32-unknown-unknown 目标、Node 和 Chrome／Chromium（可指定 CHROME_BIN）。编译和探针文件均落入 build/core-test.5。生成绑定纳入版本控制；`--check` 在临时目录生成并比较，不修改已有绑定。

验证脚本通过 Flutter 分析入口检查本包，以避开本机 Dart 3.12 独立 analyze 在退出时清理 perf socket 的竞态。编译、分析和测试任一失败仍中止，不跳过错误。

参考：[运行库](https://pub.dev/packages/capnproto_dart/versions/0.1.0)、[固定生成器](https://pub.dev/packages/capnpc_dart/versions/0.1.0)。两者为候选实现；发布者的平台标签不代替本项目实际测试结果。
