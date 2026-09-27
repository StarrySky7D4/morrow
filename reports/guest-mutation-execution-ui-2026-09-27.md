# Guest 文件变更会话与审批界面验收

日期：2026-09-27。工作树：`codex/io-safety-refactor`，基线 `b9225f64f6c62584ad7243e30249d8a088bcb155`。本报告覆盖在既有 guest 私有协议之上新增的 Dart 会话、产品审批面板和 Windows 实际 SDK 联调；不表示 SDK 冻结。

## 实现

- `GuestMutationExecutionSession` 独立于 Widget 存活，保留原 Start、提交令牌、命令回执、计划及原始 Core 响应。review 只选择与生成计划；prepare 只准备、分块及提交正文；execute 需要再次提供精确计划 SHA-256。提交或领取结果丢失不自动续传或执行，显式重取仅使用原身份。
- `GuestMutationExecutionManager` 接入插件 IO 设置。先批准实际预算并生成计划，再审阅目标、操作、包与内容摘要，分别确认 Prepare 和 Execute。扩展预算默认空白，不将包声明视为批准；普通包不获得扩展授权。
- 原有直接原生入口排除预算扩展包；新入口受 Windows guest 支持能力门控。进入设置不会隐式启动变更任务。
- 审批期间目录修订或包授权变化使旧确认失效；页面隐藏暂停展示轮询，离页保留会话。本地正文读取可取消，正文上限 16 MiB，诊断文字折叠且限长。九语言新增 14 条流程文案。
- Prepare 被拒绝时不把错误帧引用当作 guest 租约；可通过原 owner 释放。Query 仅能保留已有、身份及 Prepared 内容一致的执行资格，不能从丢回执或 Unknown 恢复资格。原执行结果和后续历史核验分别展示。
- 停止中断前端等待但不宣称 worker 已退出；必须观察真实退出、必要修复与 ACK 才能重新开始。外来任务不会被接管或清理，原错误与清理错误分开保留。

## 验证与证据

| 验证 | 本轮结果 | 证据 |
| --- | --- | --- |
| 独立 guest 会话受控边界 | 11/11 | `build/guest-mutation-session-dart-tests.log` |
| guest 客户端、新旧执行会话、恢复会话和视图状态组合 | 58/58，包含上述 11 项 | `build/guest-execution-session-regression.log` |
| Windows Release 三语言真实 Wasm → 私有协议 → Dart 会话 | Rust/C/C++ 各 1 项，共 3/3 | `build/guest-mutation-ui/summary.json` |
| Rust 实际 Widget → Release 宿主 → Wasm | 1/1，点击实际审批按钮 | 同上 run 目录 `rust-widget.log` |
| 设置平台能力门控 | 2/2 | `build/guest-execution-library-entry.log` |
| 新 guest 审批面板受控测试 | 4/4，含旧弹窗撤权与本地读取取消 | `build/guest-mutation-ui-widget.log` |
| 原生执行／恢复面板与插件设置组合 | 45/45，包含上述 2 项门控 | `build/guest-execution-ui-regression-retry.log` |
| 本地化资源与目录测试 | 4＋7 通过 | `build/guest-execution-i18n-package-final.log`、`build/guest-execution-i18n-catalog-final.log` |
| 十个相关 Dart 文件严格分析 | 0 诊断 | `build/guest-execution-final-analysis-retry.log` |
| 九语言生成一致性 | 22 artifacts 通过 | `tool/build_i18n.py --check` |

Rust 测试打包器按 edition 2024 格式检查、PowerShell 脚本语法解析及 git diff whitespace 检查通过。测试日志为本地构建证据，不纳入源码分发。

一键复验：`tool/verify_guest_mutation_ui.ps1 -Flutter C:\flutter\bin\flutter.bat`。默认 WASI sysroot 为相邻 `../tools/wasi-34/wasi-sysroot-34.0`，其他机器使用 `-Sysroot` 指定。脚本离线构建三语言实际 Wasm、当前 Release 宿主及测试专用打包器，每次新建测试包目录；拒绝空执行、跳过或缺少通过标记。只使用测试临时内容库和目标文件，恢复调用者环境变量。

成功 run：`build/guest-mutation-ui/run-20260927T085255669-39c2fa32`。三个会话均验证非空高熵四块正文（184357 字节）、错误计划摘要拒绝、review/prepare 不产生目标效果、显式 Execute 创建、Query、Delete、取消及 Release/真实退出/ACK。Rust Widget 使用实际面板和真实宿主，系统文件选择器由测试注入临时路径。

| 产物 | SHA-256 |
| --- | --- |
| Release 宿主 | `5c111905854085037f32ad5656ec81450bb3bb743e9e571feaabe5b73471eff4` |
| 测试打包器 | `81ba3b77218c6ec614e2d263d807ad2577402317428626b1125b85cc3b4df087` |
| 原有内置包 | `c71dc27866378894d5288053238fef9e0ef15f6b578449279bc681e7fe8b560e` |
| Rust Wasm | `fad599e58b7d540bef1ae1ee1407b6454508684bc76b9a1579487b9d6f68c1fa` |
| C Wasm | `ca1529e768a1c3b39aca7a2bf0ceff7e7992309b24159d9098c15c93078d81c0` |
| C++ Wasm | `157a5a4102752f30c2a37027a75993d171982df781e555105a99cafe769bb940` |

宿主及打包器重建前后摘要相同，测试期间再校验稳定；内置包前后摘要相同。完整测试包摘要见 summary.json。

初次验证暴露测试夹具父构造参数遗漏、文件读取及弹窗关闭时序断言问题，均修正后重验。共享 Flutter test_cache 并发造成的 PathExistsException 改为串行执行；未删除缓存。Dart 分析首次退出时发生 perf 临时文件删除错误，保留失败日志，独立重跑无诊断。验证脚本最初 sysroot 默认路径错误已修正。首次失败记录不作为通过证据。

## 剩余边界与下一步

1. 为 mutation-budget-v1 包补齐当前授权下的跨重启只读发现与核对；不复活旧 owner、句柄或执行许可。
2. 将非空 guest 真实故障恢复接入产品恢复界面，扩充 16 MiB 完整故障组合。
3. 完整 Windows 应用构建、系统选择器及人类操作耗时验收：当前授权最长 30 秒，过期须释放后重新审阅，不能静默续期。本轮证明自动化链路，不证明用户可轻松完成审批。
4. 其他平台及条件 Replace 继续开放；当前无条件 Replace 回退。完整 SDK 尚未冻结。

本轮未提交、推送、发布或操作用户真实目标文件。
