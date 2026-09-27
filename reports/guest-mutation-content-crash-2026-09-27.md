# 16 MiB Guest 内容事务故障与恢复资格

日期：2026-09-27。分支 `codex/io-safety-refactor`。接续 [最大正文文件效果资格](guest-mutation-max-crash-2026-09-27.md)，本报告单独覆盖 Prepare 的内容提交事务。SDK 尚未冻结。

## 验收范围

真实 Rust／C／C++ Wasm Guest 经产品 Dart 会话传递 16,777,216 字节正文，使用原 30 秒期限。正文按最多 60 KiB 的块传递，274 是由长度计算的逻辑块数，不是独立宿主计数器。内容事务矩阵的所有场景均不调用 Execute，也不领取执行许可；文末另列的旧流程回归仍执行各自原有的受控 Create／Delete。

| `MORROW_TEST_CRASH_AT` 注入点 | 正文 | LiveStaging 回执 | 原操作／目标 |
| --- | --- | --- | --- |
| `file-content-after-bytes` | 回滚 | 不存在 | Prepared／不存在 |
| `file-content-after-receipt` | 回滚 | 回滚 | Prepared／不存在 |
| `file-content-before-commit` | 回滚 | 回滚 | Prepared／不存在 |
| `file-content-after-commit` | 完整持久化 | 完整持久化 | Prepared／不存在 |

故障宿主必须实际退出 86；新普通宿主显式发现原计划并重复核对。普通宿主携带相同环境变量的对照只完成 Prepare，仍等待独立执行确认。

## 独立存储证据

恢复 DTO 的 Prepared 阶段不能区分内容事务是否提交。因此另提供 `workbench_host/examples/verify_guest_mutation_store.rs`，只接受系统临时目录下本测试专用前缀的直接子目录及其原计划文件。所有宿主确认退出后，取得原库的管理与数据库租约，加载已有受保护密钥，通过 `Store::open_read_only_audited` 独立验证，不初始化、迁移、修复或重放操作。

校验包括原计划逐字节一致、命令身份、Prepared／AwaitFreshAuthorization、无 Response 原件、正文与回执同时存在或同时缺失。提交后的正文必须满足原长度与 SHA-256，回执必须匹配主体、操作、原计划摘要、正文摘要、正文容器摘要及 LiveStaging 来源。Dart 再将原计划声明的正文摘要与本次实际传入的正文对比。辅助程序不会新增生产或 Guest 接口。

管理锁和选择文件必须已经存在且不是重解析点。管理锁与数据库锁按正常租约接口打开；这里只承诺数据库只读，不将取得租约描述为整个临时目录绝对零写。参数缺失与项目目录冒充临时库的拒绝检查见 `build/guest-mutation-content-crash-recovery/store-verifier-rejected-arguments.log` 和 `store-verifier-rejected-scope.log`。

## 测试流程与清理修正

共用准备逻辑拆出仅审阅原计划的 helper，原文件效果测试仍保留 Prepare 成功断言。新测试在 Prepare 前保存原计划，目标目录在准备、恢复、重复核对前后都须为空；不能只看最终文件不存在而漏掉临时写入。新会话不能继承原 Execute 许可。

Widget 使用实际的工作流选择、发现、原计划选择、释放／确认和两轮核对按钮；第二轮须读回同一份原始记录。校验和清理采用 `runAsync<bool>` 的明确完成结果，并在内部捕获错误，避免异步异常被包装器消费后仍删除失败现场。正常恢复宿主必须确认以 0 退出，故障宿主必须确认以 86 退出；未知退出、校验失败或清理失败不能打印最终成功标记。

## 本轮运行证据

```powershell
./tool/verify_guest_mutation_content_crash.ps1 -Flutter C:/flutter/bin/flutter.bat
```

脚本要求前一轮 52 项摘要与固定宿主、内置包、三语言 Wasm／插件包均可核对，复用这些产物而不重新编译宿主；单独编译只读校验 example。新矩阵写入独立目录，检查每项精确整行标记、实际通过计数、无跳过、产物摘要及前后相同的源码摘要。`EXECUTE_CALLS=0` 是测试路径的标记，不冒充宿主独立计数器。

本轮统一源码及产物的完整矩阵 **20/20 通过，无跳过**：

| 场景 | 通过 |
| --- | ---: |
| Rust／C／C++ 各四个内容事务故障点 | 12/12 |
| 三语言普通宿主对照 | 3/3 |
| Rust 实际恢复 Widget 四个故障点 | 4/4 |
| Rust Widget 普通宿主对照 | 1/1 |

完整摘要为 `build/guest-mutation-content-crash-recovery/summary.json`，运行目录为 `run-20260927T104216372-d9486a9d`，总日志为 `verifier-final.log`，逐项日志保存在运行目录的 `logs/`。主代理独立核对 20 个不同用例的实际通过计数、正文／回执预期、固定正文摘要、32 项源码摘要以及普通／故障宿主、内置包、校验器和三语言产物摘要，结果为 `root-evidence-audit.json`。此前 52 项文件效果资格及本轮试跑不拼入这个通过数。

最终只读校验器 SHA-256 为 `daa8c9ef5c8252e2ec0c5afb90b9db8e4df7488b8bf188b6c68e1a35d5f53602`。正文 SHA-256 为 `bb62a28e904f44b184a8004fd6872310292b86f4e11b55931205aeedd4414245`。普通／故障宿主沿用前一轮已固定的 Release 产物，构建校验器未改变它们；本轮没有重编完整 Windows 应用。校验器构建成功，宿主库仍有五项既有警告，未宣称 Rust 全量无警告。

共用 helper 拆分后的旧流程代表性回归另 **2/2 通过**：184,393 字节的 Rust 原生 Create 普通对照与 Rust Widget Delete 普通对照，日志分别为 `legacy-native-create-control.log` 和 `legacy-widget-delete-control.log`。这两个旧对照仍在独立临时目录执行原有 Prepare／Execute／恢复流程，不属于上面的 Prepare-only 矩阵。这不是重跑此前 52 项，也不计入本轮 20 项。新原生测试、新 Widget 测试及共享 helper 的严格 Flutter 分析 **3 文件、0 问题**，见 `flutter-analysis-final.log`；`git diff --check` 通过。

## 边界与后续

这是 Windows 本地受控进程退出资格，不等价于断电或任意磁盘损坏验证。完整应用构建、系统选择器、30 秒人工审批可用性和其他平台资格继续开放；Debug 最大正文的期限限制仍存在。条件 Replace 未提供弱化的替代实现。

本轮未提交、推送或发布。
