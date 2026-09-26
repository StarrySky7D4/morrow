# 插件文件私有协议、Dart 客户端与二进制摘要修复

2026-09-26，基于本地 `faf86d7`。应用版本和主线基准保持；没有使用 Actions/CI、推送或发布。

## 本轮完成

在既有原 owner 文件任务上追加 `fileStart / fileChunk / fileFinish / fileRead` 私有调度协议、生成的 native/web 绑定和独立 Dart 类型化客户端。RustWorkbench 委托该客户端，沿原调度通道发送，文件动作不能嵌入后台业务命令。任务继续使用现有轮询、取消、实际回收、修复和确认流程。

启动显式绑定随机提交身份、包 ID／摘要、注册表修订、声明 handler、选中文件路径、捕获 ceiling 和期限。文件尝试身份最多 512 个，同一尝试不重放；HTTP／文件状态统一当前提交身份，避免带出陈旧 HTTP submission。原已打开句柄 API 保留，可信私有路径由 worker 在授权／取消检查后打开；准入线程不执行文件系统操作。路径必须绝对、无 NUL、最多 4096 字节，打开错误不返回原路径。

一次领取结果分为 Pending、Captured、Chunk、Finished，不公开 guest grant 引用。64 KiB 单块和 128 KiB 私有帧约束保持。Dart 以 BigInt 保留 UInt64，拒绝畸形／混合结果字段、超界摘要和块、偏移溢出及错误任务／提交身份；自有不可变字节复制完成后擦除私有回执帧。Rust 的中间文件结果帧也增加擦除。客户端不自动重试启动或读取。

## 发现并修复的实际缺陷

首次真实协议测试用独立 `sha2::Sha256` 核对 90,007 字节文件，发现上一轮捕获摘要误用了 `schema_digest`。该函数会执行 UTF-8 有损转换和 CRLF 规范化，不适合二进制文件。先前部分测试调用同一个 helper，因而没有发现问题。

现改为对原始字节计算 SHA-256；文件引用派生的二进制 secret seed 同样修正，防止两组不同非 UTF-8 secret 经有损转换得到同一引用。增加非法 UTF-8／CRLF／不同 secret 回归，更新旧测试为独立摘要预期，并由 Dart crypto 再次核对实际 Rust 帧中的文件摘要。文件引用不持久化，没有旧引用迁移或旧 guest 重编。

## 实际验证

| 检查 | 结果与范围 |
| --- | --- |
| runtime 常规全量 | 415 通过，6 ignored；包含文件 owner 10、FileBroker 24，分项不重复计数 |
| Workbench Linux lib | 64 通过；7 个文件任务测试覆盖三语言真实 guest、路径打开错误脱敏、身份耗尽／重复、错误 TaskKey、Ready 后取消和嵌套调度拒绝；保留 8 个既有警告 |
| 独立三语言 IO SDK | 原 5 个 ignored 显式执行全部通过；使用冻结 Wasm 和测试内 FileRead 声明包 |
| 独立 Dart 文件客户端 | 7 通过、无跳过；6 个模型／客户端边界测试，加三语言真实 Rust 回执解析和独立 SHA-256 验证 |
| runtime lib Clippy | `--features packages --lib -- -D warnings` 通过 |
| 私有绑定生成 | `generate_workbench_client.py --check` 通过；源 schema 与 native/web 绑定、私有 digest 一致 |
| 冻结 transport 原件 | 17 个固定文件摘要保持；无 guest 重编／重封 |

Dart 测试使用独立 Dart 3.12.0，在 `packages/morrow_core_client` 的已锁定依赖下执行。Workbench 的真实协议往返由 Linux 测试构造中的原 Store／Manager／worker 执行，输出 Cap'n Proto 回执供 Dart 解析；这不是生产宿主子进程和 Flutter 窗口的端到端验收。生成器因根目录 Flutter lints 不可用输出警告，但生成比较成功；不能把格式解析记为完整适配器静态分析。

证据位于 [本轮日志及真实帧](evidence/plugin-file-wire-2026-09-26/)。历史网络／原服务包资格本轮没有重跑，不沿用为本轮新增测试计数。

## 尚未完成与环境限制

系统选择器、Flutter 文件任务控制／结果页面、Windows 实机、实际 DPAPI/TLS 产品资格仍开放。尝试启动 Flutter 和获取其依赖时，自动审批检测到间接云实例元数据访问并拒绝执行；已停止该路径，没有以其他方式重试该访问。独立 Dart 测试不依赖 Flutter 启动。本轮 RustWorkbench 接线仅完成源码委托和解析检查，没有完整 Flutter 类型分析／窗口测试。

路径入口仅对可信 UI 开放，不能证明路径由选择器产生；不提供 symlink/junction 根约束、选中时刻对象或原子快照保证。文件身份从 worker 实际 open 起确定，单个同步系统调用不可强制中断。Linux 生产受保护存储仍不支持，测试结束的维护失败仍保持 RecoveryRequired；没有增加明文生产后备。目录、写入、持久文件证据和跨重启恢复也不在本轮范围内。

下一步：真实平台文件选择和任务页面 → 原身份恢复观察／按 offset 与最终 hash 校验消费 → Windows 选中、取消、丢回执、修复和实际退出。当前仍为本地稳定候选，不宣布插件底座跨平台整体收尾。
