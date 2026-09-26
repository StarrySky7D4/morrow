# 插件单文件句柄捕获与底座回归

2026-09-26，基于本地 `601fb0b` 继续。远端 main 核对仍为 `d9c043191a400df972832d398b80cb73dfc51f56`。应用版本不变，未使用 Actions/CI，未推送、发布或更新 SDK 冻结承诺。

## 本次增量

此前 FileBroker 只接受宿主已经提供的 `Vec<u8>`，无法保证获取这些字节时已经按原实例授权和额度准入。本轮增加 `grant_open_file`，接收可信平台适配器已选中的普通文件句柄：先验证 FileRead，再以原账本预留完整长度和 EOF 探测字节，分配有界内存，分块检查授权与期限，最后固定实际字节、长度和 SHA-256。旧 Vec 入口复用同一预留／发布实现，其计费保持不变。

没有引入依赖、unsafe、新 guest 能力或 schema。guest 继续使用 Read／Finish／Cancel，无法提交路径或自行打开 OS 文件。失败不暴露半成品引用，清理释放并发槽但不退累计费用。

具体接入条件、错误与生命周期见 [单文件合同](../docs/PLUGIN_SELECTED_FILE.md)。源码主要位于 `plugin_runtime/src/file_io/selected.rs`；专项在 `plugin_runtime/tests/support/selected_file.rs`，三语言路径在 `plugin_runtime/tests/sdk_io_guest.rs`。

## 验证范围

新增八项文件专项覆盖：

- 已打开对象的路径移位／替换后仍读取原句柄，捕获后删除源对象，guest 两块读回完整 90,007 字节及 EOF。
- 宿主长度、实例单作业预算、资源名额不足、跨实例与 FileList-only 绑定，在 seek/read 或扣费前拒绝；错误能力不能推进共享时钟。
- 捕获前、读后和摘要后发生停止、期限到达或时钟回拨，均无引用交付，job/resource 释放，累计费用保留。
- 文件增长、截断、只写句柄读取失败，空文件的一字节预留，Unix 目录／设备拒绝。
- 等长修改可能形成跨时刻字节，摘要准确描述固定内容，不把两次 stat 误报为原子快照。

三语言专项使用 `transport-v1-rc1` 保留的 Rust／C／C++ Wasm 模块；测试建立 FileRead 临时包后运行，未改变冻结原件，**不是原 HTTP 包获得文件读取能力，也不是发布新的文件原包**。

| 验证 | 实际结果 |
| --- | --- |
| 最终文件专项 | 23 项通过，含 15 项既有与 8 项新增 |
| 三语言 SDK IO 专项 | 5 项通过，每项覆盖三语言；真实选中文件分块读取、删源、结束释放已接入 |
| runtime 完整回归 | 403 项通过、6 项 ignored；运行于最后新增 FileList-only 测试前，该新增项及改进后的原对象替换场景随后通过最终 23 项专项 |
| network 完整常规回归 | 134 项通过、12 项 ignored；本轮不计 ignored 专项为重验通过 |
| runtime lib 严格 Clippy | `-D warnings` 通过 |
| transport 原件完整性 | 17 个固定文件保持不变；没有重编 guest 或重封基线 |

分项有重叠，不相加为产品通过率。runtime 的五个 SDK ignored 已显式运行，另一个 C/C++ 原生 service codec 专项本轮未重跑；Windows 专属依赖目标在 Linux 为 0 项，不计通过。旧内容／UI 兼容运行包含在 runtime 常规回归中。原始文本日志仅规范化行尾，见 [本轮证据](evidence/plugin-selected-file-2026-09-26/)；源码指纹单列保存。

## 剩余门槛

本轮完成原生运行时的单文件捕获入口，并未接入工作台系统文件选择器、文件任务私有协议或 Flutter 页面。同步 OS 调用不能强制中断，必须在原 owner 调度；没有另开 Store 的绕行路径。字节仍为当前 broker 内存资源，没有新增持久录制、跨重启引用恢复或完整文件系统。

下一步应把文件选择授权、原 owner 任务准入／取消／回收和分块消费接入工作台，再验证 Windows 真实句柄生命周期与其他平台资格。目录安全解析、创建／替换／删除、分块写入仍开放。此前配置服务的 Windows/Flutter、DPAPI 和 TLS 实机资格继续开放；整体仍为 Linux 本地稳定候选，不能宣称插件底座已全平台收尾。

源码、增量 Git 历史、保留原包及本轮验证记录另存 Google Drive 恢复检查点。
