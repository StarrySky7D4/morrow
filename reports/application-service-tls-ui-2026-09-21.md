# 应用服务 TLS 协议与 Windows 界面验证

2026-09-21，基于 `150b247`，分支 `codex/io-safety-refactor`，版本仍为 `0.1.9-test.52+56`。本阶段接通已验证的原生 TLS 准入，不改变公共插件 ABI。

## 完成内容

新增私有检查请求和可选启动选择，Dart 与 Rust 均约束路径、证书摘要及所有权。检查返回的路径必须对应所请求文件；复制后的摘要不可修改。私钥字节不进入 Dart、请求帧或工作台持久配置。检查可通过运行中原 owner 的普通业务通道执行，启动保持独立调度通道。

运行面板现在显示已批准 TLS 发布，提供证书链/私钥选择、检查结果和证书 PEM 摘要。未检查不能启动；文件改选清理旧检查结果，发布/后端变化清理旧选择。输入草稿和已检查状态支持语言变化，停止/Unknown/重挂仍不自动重发。启动冻结选择，Rust 再次核验实际文件，避免检查后静默替换证书。

新增文案来自中英文 ARB 源并重新生成 PB/LZ4 语言资源。按钮使用现有轮廓、颜色和圆角。截图发现摘要在 SelectableText 下裁剪，改用可选择区域中的 Text；检查完整 Windows 图像确认可见布局。

## 证据与边界

| 验证 | 本轮结果 | 本地日志 |
| --- | --- | --- |
| Rust 宿主 lib + service_protocol | 78 + 7 passed，0 failed/ignored | `build/tls-wire-host-tests.log` |
| 客户端模型、编解码、运行会话/面板、TLS 选择控件 | 64 passed | `build/tls-client-tests.log` |
| 真实 Dart→Rust 进程 | 2 passed，无跳过 | `build/tls-real-native-tests.log` |
| 完整 Windows 窗口 TLS 流程 | 1 passed；截图调整后的重跑不重复累计 | `build/tls-window-selection-tests.log` |
| 共享语言资源 | 4 passed | `build/tls-i18n-package-tests.log` |
| 相关 Dart 静态分析 | 9 文件通过 | `build/tls-client-analysis-final.log` |
| 宿主 lib 严格 Clippy | 通过 | `build/tls-wire-clippy.log` |
| 私有绑定与语言生成一致性 | `generate_workbench_client.py --check`、`build_i18n.py --check` 通过 | 工具输出 |
| 完整 Windows Release | `lib/main_rust.dart` 构建成功，随包宿主 SHA-256 与实际测试宿主一致 | `build/tls-windows-release-selection.log` |

真实进程测试通过私有通道检查合成 PEM，故意发送错误摘要、检查后更换文件并确认拒绝；原 submission 在准入失败后仍可明确启动。实际 SecureSocket 客户端使用合成可信证书，认证 HTTPS 请求触发 WAT 服务包返回 202；原 Rust 工作台业务在服务期间保存设置，运行中再次检查 PEM，停止回收后重开同库核对设置。

窗口测试打开完整 `MorrowApp`，从设置面板选择 TLS 发布，依次操作选证书、选私钥、检查、启动、停止、确认。它验证实际 HTTPS 返回、冻结选择和同库重开，输入为 Flutter 框架注入。文件选择器通过官方平台接口返回明确的本地合成 PEM 路径，未验证 Windows 系统文件对话框或物理输入。服务包为 WAT 夹具，不能当作公共 Rust SDK 资格；内建业务仍用真实 Rust/Wasm 插件。

截图与结果：`build/tls-window-150b247-selection/01-checked.png`、`result.json`。完整 Release 目录及摘要见本地 `build/tls-ui-build-receipt.json`；不能单独拷贝 EXE。没有新版本号、推送或 GitHub Release。

## 协作审核

GLM max 的两份测试候选错误假定 Result/Base64/copyWith 接口，未原样采纳。DeepSeek max 候选提供模型边界测试结构，主代理修正为实际验证调用后执行测试。另一次 DeepSeek max 控件审核成功返回，但其“保留另一文件路径会绕过检查”意见不成立：所有启用检查都会再次执行宿主配对验证；后端/发布变化由父组件清理选择并重建控件。执行成功与语义正确分开记录。

原始候选和审核保存在本地 `build/bridge-tls-dart-tests*.json`、`build/bridge-tls-dart-deepseek-tests.json`、`build/bridge-tls-picker-review.json`。无用户真实证书、私钥或配置上传给模型。

## 未完成与后续顺序

当前 TLS 是明确选择 PEM 的有限运行能力，不是完整证书管理。下一项为有效期提示/过期策略、受保护密钥存储与明确续期流程；之后推进 Unknown 持久核对、完整文件系统、C/C++/Rust IO SDK 及跨平台资格。尚无自动 ACME、客户端证书认证、服务端证书到期主动停服、真实系统选文件验收或公网部署验收。
