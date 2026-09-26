# SDK 库源码锁定与构建漂移检查

2026-09-26，基于 `c76e87fed6bb968adc6b8fe9b5c80e6731b5e729`；应用保持 `0.1.9-test.56+60`，主线基准 `d9c0431`。未使用 CI/Actions、推送或发布。

## 完成功能

新增 `sdk.lock.toml`，可在新建工程时用 `--lock-sdk` 生成，也可通过 `lock-sdk PROJECT` 为既有工程创建。相对路径、字节长度和原始 SHA-256 锁定 SDK 库源码／契约、Cargo 声明／锁、build.rs 及许可文件；本次 SDK 为 49 个文件。锁中没有机器绝对路径，相同源码在不同目录产生相同锁。

`validate`、`build`、`pack` 自动检查已有锁，`--require-sdk-lock` 拒绝缺失。预检输出状态、锁摘要和文件数。库文件新增／删除／修改都会失败，不因 schema 或版本未改而放过实现漂移。编译后还要核对源码和原锁身份；发生源码变化、锁消失或被重写时停止候选打包。旧工程允许无锁，摘要明确标记 absent。

更新锁必须显式执行 `lock-sdk PROJECT --update`，并继续核对 SDK 契约和 Rust 路径绑定。首次创建不覆盖已存在锁；更新通过同目录暂存和原子替换发布，失败清理暂存，原锁保持。创建依赖文件系统硬链接，不支持时明确失败。拒绝符号链接、junction、非普通文件、重复路径、非法相对路径、畸形摘要或预算溢出。

`verify_plugin_projects.py --lock-sdk` 在模板生成和后续预检／打包过程中要求锁定，可与 `--preflight-only` 组合，在没有工具链时执行 21 种模板的锁定检查。

## 本轮验证

| 检查 | 结果和实际范围 |
| --- | --- |
| Python 工具全量 | 118 项通过、无跳过；新增 12 项源码锁测试，资格门槛原测试增加严格锁场景 |
| 三语言真实 CLI | 21 工程生成锁并通过严格预检，21 份锁字节相同，均锁定 49 个 SDK 文件 |
| 迁移 | SDK 源码复制到含空格／中文的新路径，锁不变；Rust 工程需显式更新 Cargo SDK 路径，之后严格预检通过 |
| 漂移／失败 | 库文件新增／删除／修改、缺失锁、损坏锁、越界／重复字段、符号链接、并发创建不覆盖、发布失败保留原锁 |
| 构建期间改动 | 受控编译器替身修改 SDK、删除或重写锁，证明 pack 不调用宿主打包器；不是实际编译资格 |
| 显式更新 | 不带 --update 拒绝覆盖，更新失败保留旧锁；不兼容 IO 契约即使显式更新也拒绝 |
| SDK 契约与冻结原件 | 同步检查通过，17 个 transport 固定文件摘要保持 |

最终证据见 [源码锁记录](evidence/plugin-sdk-source-lock-2026-09-26/)。归档含一份可迁移锁、21 份严格预检 TOML、测试与契约日志及工具源码摘要。整理首轮证据时发现一份原始 CLI 日志为空，未将其作为完整证据；在新目录重新执行全部 21 项，并逐份解析后归档。最终归档已核对完整。

## 复验命令

```sh
python3 -m unittest discover -s tool/tests -v
python3 tool/verify_plugin_projects.py --preflight-only --lock-sdk --output-root /absolute/new-locked-evidence
python3 tool/plugin_transport_baseline.py verify
python3 tool/sync_plugin_sdk_contracts.py --check
```

普通使用、锁范围与限制见 [项目工具](../docs/PLUGIN_PROJECT_TOOLS.md)。Python 3.11+ 标准库即可，没有新增第三方依赖。没有运行 Rust/Wasm guest 或 Flutter／Windows；历史测试不合并为本轮通过数。

## 范围与剩余工作

此锁只覆盖明确列出的 SDK 库输入，不覆盖生成示例、工程源码、工具链、环境变量、传递依赖实际源码或宿主打包器。Cargo.lock 本身被锁定，不等于已验证所有依赖源码。文件系统预算为 2048 文件、8192 个目录内条目、单文件 4 MiB／合计 16 MiB，锁文件最多 1 MiB。

检查不是签名、安全沙箱或原子快照，无法检测两次检查间改动又还原，也不提供完整多进程互斥。构建后检查失败可能保留编译器产生的候选；下次 pack 仍重新构建。SDK 作者真实性、完整可重现构建、独立分发、第三方实际接入及全平台资格仍需独立完成。

Windows／Flutter 文件与服务任务产品验收、九语言文案、完整文件操作／流式网络／跨进程恢复继续开放。本轮未重试此前被自动审批以间接云元数据访问拒绝的 Flutter 启动路径。检查点继续保存到 Google Drive。
