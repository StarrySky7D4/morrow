# 独立文件恢复会话与真实进程恢复

日期：2026-09-27；基线 `b9225f64f6c62584ad7243e30249d8a088bcb155`，开发分支 `codex/io-safety-refactor`，本轮增量未提交。

## 实现

新增 `MutationRecoverySession`，统一原计划发现、独立历史核对及其 IO 清理状态；通过 RustWorkbench 稳定持有，监听器与页面离开不触发重执行。所有动作串行，缓存最多一页原计划和一个核对结果，未引入后台轮询或全库常驻缓存。

会话保留未知提交的原令牌，仅显式精确重取回执；status 本身不证明命令属于原提交。已消费但丢回包的计划页标记 resultLost，禁止自动重读／跳页。未消费命令允许刷新验证后的显式领取。核对 Unknown 保留原语义。停止、维护、实际退出和确认分别处理。

修补打包工具缺失的 file-create／file-replace／file-delete 名称，保持仅声明、不授予权限的行为。未知能力负例改为真正未知名称，原负例覆盖继续保留。

## 验证

| 范围 | 最终结果 | 日志 |
|---|---|---|
| 恢复会话／变更客户端／生成绑定／文件任务组合 | 61 通过、0 失败、1 既有夹具跳过 | `build/mutation-recovery-dart-combined.log` |
| 真实 Windows 宿主恢复＋受控传输／清理 | 27 通过、0 失败、无跳过 | `build/mutation-recovery-flutter-combined.log` |
| 指定 Dart 文件严格分析 | exit 0，JSON diagnostics 空 | `build/mutation-recovery-final-analyze.log` |
| 离线打包工具回归 | 16 通过、0 失败 | `build/mutation-recovery-package-test.log` |
| 本轮真实 Windows 宿主构建 | exit 0，Debug 14.68 秒、5 条既有警告 | `build/mutation-recovery-host-build.log` |
| 实验包夹具构建及容器检查 | exit 0，仅声明 FileCreate／FileDelete | `build/mutation-recovery-fixture-build.log` |

Dart 组合中的 15 项为新增恢复会话测试；唯一跳过是依赖另外 MORROW_FILE_WIRE_FIXTURES 的既有文件帧测试，Mutation 七份真实 Rust 夹具均启用。Flutter 27 项包含一个真实宿主综合场景和 26 项受控传输测试。测试构建与实际产品 UI 验收分开统计，不重复相加宣称覆盖率。

早期默认 Dart analyze 遇到系统 perf 清理 errno 1920；最终使用工作树内 DART_DATA_HOME 严格分析，退出 0。没有以异常退出冒充通过。

真实宿主使用本轮源代码编译的 `workbench_host/target/debug/morrow-workbench-host.exe`；内建包使用既有 `build/workbench-host/bundle/workbench.morrowplugin`。实验包由 `core/examples/plugin_package.rs` 打包既有 `sdk/compat/transport-v1-rc1/rust-io.wasm`，显式声明两个变更能力与八个资源。本轮未重建 Wasm guest。所有实际效果均在全新临时受保护库与其临时目录中；不接触用户资料库。

真实集成场景：以明确临时选择构造原计划，Create 暂存空内容并实际创建，Delete 只 Prepare；释放并实际 join／ack 后关闭 Rust 进程。新进程重新打开相同库，按两种范围发现字节一致的原计划，独立核对出 Observed＋OS 成功和 Prepared＋未指定效果。原文件仍保留，未执行 Delete。进一步模拟实际宿主已消费而 Dart 丢回包，验证不重读／跳页，清理后显式新扫描能找回原件。

还通过真实宿主验证：错误包摘要启动被拒绝，宿主保留提交令牌但未创建任务；会话允许明确放弃本地请求，然后用新令牌启动成功。另有受控用例验证 ACK 在执行前后丢回执、停止在执行前丢回执、并发调用拒绝、监听器异常隔离及 Unknown 原样保留。

审核修复了三类状态风险：不能凭 status 推断未知提交归属；停止意图须在发送前建立，丢回执也不能重新交付页面；权威状态无活动任务时，历史提交令牌不能永久阻止纯本地 abandon（不清除宿主令牌，不认领外来活动任务）。

这里证明正常关闭后的真实进程重启与 Dart 交付故障处理，不等于强杀／系统掉电的整链验收。底层已有崩溃测试也不代替完整产品 UI 验证。

## 后续

恢复 UI 和选择审批流程、大库预算／期限耗尽续扫、真实崩溃恢复、完整 IO guest SDK 及跨平台资格继续开放。条件替换仍 Unsupported，SDK 未冻结。本轮没有提交、推送、Release 或安装包。
