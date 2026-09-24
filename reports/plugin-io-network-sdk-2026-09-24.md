# 三语言 IO SDK 真实 HTTP 资格

2026-09-24。延续 [IO SDK codec 增量](plugin-io-sdk-2026-09-24.md)，本轮将编译后的 Rust／C／C++ guest 接入实际 Package、Manager、IO worker、HttpEndpoint 和 TCP 传输。状态 **PASS_SCOPED**，Windows 本地测试服务器、临时内容库及合成数据；SDK 未冻结，未提交推送或发布。

## 已验证

| 用例 | 实际证据 |
| --- | --- |
| 七种方法 | 三语言分别发送 GET／HEAD／POST／PUT／PATCH／DELETE／OPTIONS，共 21 场；服务端收到原始方法、重复查询参数、重复头和二进制正文，响应字节／头经 guest 返回并持久记录 |
| 凭据及 HTTP 错误 | 三语言各一场；仅传输层注入合成 Bearer，服务器返回 429；guest 收到 Completed + 429 + 正文，原请求帧及其持久材料不包含秘密 |
| 拒绝路径 | 三语言分别验证错误端点、未批准方法、错误凭据，共九场；无网络发送、无 IO intent、不误报 Unknown |
| 收请求后断线 | 三语言各一场；结果 Unknown，同操作第二次提交不重发，连接计数为 1；停止 worker、关闭并重开数据库后仍为 OutcomeUnknown／ReconcileOnly |

四个新增专项用例显式运行后全部通过、零跳过。29 个既有 managed_http 用例全部通过；普通运行显示四个需预构建 guest 的 ignored，用专项命令随后逐个执行。网络测试目标严格 Clippy 通过。前置的五个文件／codec IO 专项和旧插件原件兼容门槛同时通过。

这些是实际本地 HTTP 传输证据，补上此前“仅由核心编码 HTTP 回执”的边界；不代表公网服务商互操作、三语言入站服务 SDK、OAuth、流式或全平台均完成。数据库重开证明 Unknown 记录可持久读取，不代表跨进程恢复授权及业务核对界面已接通。

## 复现

```powershell
pwsh -File tool/verify_plugin_io_network.ps1 -Sysroot "实际的/wasi-sysroot-34.0"
```

入口先构建三语言 IO 示例、检查冻结原件并运行 IO 专项，然后运行现有 managed_http 回归和显式 SDK 网络专项。缺任一 guest 产物会失败。仅新的测试辅助函数可注入已编译 guest；原有测试继续使用原 WAT 模块，未放宽生产权限或网络规则。

日志与三种 guest 摘要见 [机器记录](plugin-io-network-sdk-2026-09-24.json)。日志位于 `build/sdk-io-network-qualification.log` 与 `build/sdk-io-network-clippy.log`。

## 项目工具与原包闭环

`tool/morrow_plugin.py new --kind io` 已支持 C／C++／Rust。默认只声明 file-read；显式选择 http-request 后生成 `morrow.http.forward.v1`，与工作台真实 HTTP 入口匹配。打包 CLI 只允许当前已支持的 file-read／http-request／credential-use，拒绝未知、重复、不完整及混合执行声明；固定 IO 上限为两个资源、一个作业，总量与单作业各 1 MiB。`pack`／`inspect`／`check` 显示 IO 能力、handler、schema 和预算，保持无授权语义。31 项项目工具单测和九项打包 CLI 用例通过。

三个 HTTP 项目实际完成 new → build → pack → check；之后以**原始生成包**执行上述四项三语言 HTTP 专项，再次通过，旧 hash 包保留。原包验收要求工作台 HTTP profile，精确使用归档内容及预算，不重建 Manifest。验收过程中发现测试宿主原固定 4 MiB worker 预算超出模板 1 MiB 上限，运行时正确拒绝；现按包声明取较小值，与正式 HTTP 宿主行为一致，没有修改包或放宽运行时。仅 io.request 的早期模板虽能底层运行，却缺工作台兼容声明，已修正生成逻辑并重建三包。

原包模式可用 `verify_plugin_io_network.ps1 -RustPackage <包> -CPackage <包> -CppPackage <包>` 复验；三个包须一并提供。该测试使用专用 ID `org.example.managed.http.tests`，各包需 HttpRequest／CredentialUse 和 `morrow.http.forward.v1`，不会对任意用户包隐式补声明。其余构建参数与上文相同。包摘要、最终打包日志及 `build/sdk-io-network-packages.log` 收录在机器记录。

`verify_plugin_projects.py` 已覆盖 15 种模板并支持显式 sysroot：原有 12 包执行门槛保留，三个默认 file-read IO 包在该入口只静态准备。**本轮没有重新执行全套 15 模板流程**；本轮实际证据是单测、CLI 测试和三种 HTTP IO 原包真实运行。也没有新增 Flutter 窗口交互验收或应用构建。

后续优先补三语言入站服务开发接口／模板及真实服务节点验收，再推进完整文件系统、流式和跨进程业务核对。模板的受管 HTTP 转发 profile 并非任意 guest 自主网络协议执行器；后者需要显式演进授权和任务契约。
