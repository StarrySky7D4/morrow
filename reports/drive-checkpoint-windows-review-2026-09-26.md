# Google Drive 检查点 Windows 本地复核 — 2026-09-26

## 结论与范围

已将 Google Drive 检查点 `807b9cd4385b716cc3141b0ec41d41269c709e22` 拉回独立工作树及分支 `codex/drive-review-807b9cd`。相对原开发线已提交基线 `d9c043191a400df972832d398b80cb73dfc51f56`，包含 12 个提交，主要扩展服务 SDK、受管文件任务、三语言模板以及 SDK 源码锁。

本轮 Windows 本地测试支持继续评审集成；不等于 SDK 冻结、完整 Windows UI 验收或全平台资格。未合并、提交、推送或发布。原开发目录已有的主题 1.0.1 等未提交工作仍留在原目录，不包含在此检查点中。

## 来源与完整性

来源文件夹：https://drive.google.com/drive/folders/1UOU209FTucEMxKEM58E1pkdiHCEtK_L2

- Drive 清单覆盖的 5 个下载文件 SHA-256 全部匹配。
- 源码 ZIP 内 2,164 个受跟踪文件与 Git 检查点 blob 完全匹配。
- 证据 ZIP 的 197 个文件通过其内部完整清单校验。
- Git bundle SHA-256：`8dc35e97e949fd7ffa8537a6d1de1ba2504f2b496289fe4dd320938201b95200`。
- 源码 ZIP SHA-256：`01d85f15a420b2a0552203c38783444a987befb7ddec63390c213b88e492ff21`。
- 证据 ZIP SHA-256：`e4ba8a9bdecec8512c6ac48dc4f3aefa1c146c17f0524e54b3ce5a7099a11b92`。

## 本机实测

日志均在本工作树 `build/local-review/` 中；导入报告与本轮实测分开记录。

| 检查 | 结果 | 日志或说明 |
| --- | --- | --- |
| Python 工具测试 | 118 通过 | `python-tests-fixed.log` |
| Rust SDK | 66 通过 | `rust-sdk-tests.log` |
| Rust 宿主完整 lib 测试 | 143 通过，0 失败、0 忽略，116.53 秒 | `host-lib-tests-qualified.log` |
| 文件 owner、受管 IO、服务 codec、冻结传输 | 37 通过，1 忽略 | `runtime-file-service-tests.log` |
| C/C++/Rust 服务原包真实回环 HTTP | 6 通过 | `service-http-originals.log`；使用校验过的原包，未重打包 |
| Dart 文件协议及会话 | 28 通过，无跳过 | `dart-file-tests-with-frames.log`；注入 Rust 原始帧目录 |
| Flutter 插件、HTTP、IO 控制与服务回归 | 101 通过 | `flutter-plugin-regression.log` |
| 21 种模板源码锁预检 | 21 通过 | `locked-preflight.log`；此检查不构建或执行模板插件 |
| SDK 契约同步 | 通过 | 本机检查 |
| 冻结传输原件 | 17 个文件通过 | 本机清单检查 |
| Flutter 相关文件静态分析 | 无 error/warning；9 条 info，命令退出 1 | `flutter-file-analysis.log`；均为条件分支大括号风格提示 |

宿主测试使用本轮从源码编译的 workbench、service-outbound 和 http-forward 三个真实 Wasm guest。初次运行未配置这些测试夹具，因此产生夹具缺失失败；补齐环境变量后完整重跑 143 项通过。日志保留，未把初次失败删除。

## 隔离分支内的最小修正

仅修正 4 个测试文件，生产代码未修改：

1. `tool/tests/test_plugin_project_preflight.py`：子进程显式使用 UTF-8，父进程指定 UTF-8 解码。修复 Windows 中文环境下 CP936 输出被 UTF-8 读取导致的测试错误。
2. `workbench_host/src/http_tasks_tests.rs`、`io_tasks_tests.rs`、`service_tasks_tests.rs`：旧夹具通过已移除的 `catalog` 安装插件，改用现有 `manager.install_package(package.archive())`。这三处问题已存在于共同基线，并非本次 Drive 提交引入。

修正尚未提交。Flutter 生成文件的状态变化仅体现换行/工作树元数据，规范化 Git diff 无内容差异。

## 尚未覆盖及合入前事项

- Runtime 忽略项 `native_c_and_cpp_handles_return_core_verified_frames` 需要另行构建原生 C/C++ 可执行夹具；本轮通过的三语言 HTTP 测试使用 Wasm 原包，不能替代原生 ABI 验收。
- 未构建完整 Windows 应用安装包，未实测窗口中的原生选文件交互；未进行 Web/Android、远程 CI 或公网 API 互操作验收。
- 服务 HTTP 测试中的凭据提供器不能证明 Windows DPAPI/TLS 全生命周期资格。
- 新增文件任务界面仅有中文/英文文案，既有其余七种语言仍需补齐。
- 静态分析的 9 条 info 尚未清理。
- 完整文件系统写入/目录能力、完整流式网络能力、跨重启业务核对及独立 SDK 发布资格，仍须按现有路线继续推进。

建议保留这些增量，在合入前携带本轮测试夹具修正，并与原开发目录的主题及文档未提交改动分别核对。不能用此检查点覆盖原工作树。
