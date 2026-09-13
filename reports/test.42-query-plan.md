# test.42：版本化查询调度与完整观察重放

日期2026-09-13。应用 `0.1.9-test.42+47`，工作台宿主及打包清单 test.42；核心／审计保持 test.41，运行时／SDK test.11、guest test.13，数据库格式11。第一方 AGPL-3.0-only，仅本地推进，不推送、打标签或发布 Release。本阶段无新增 unsafe。

## 实质变化

默认 Workbench::query 已接入 query_plan V1：输入候选要求唯一升序 ID；按记录数与编码字节分段过滤，反向命中序列后使用实际 guest 稳定排序。所有 Filter、SortRun、Merge 都经同一个 Backend::invoke，未来捕获适配器不能漏记某种调用。

逐对归并改为左右各最多64个前缀的实际 guest 排序，编码预算不足时缩小前缀。只发出到任一侧前缀耗尽为止的安全部分；未发部分在下轮补齐。先校验整个响应是完整排列，且保持每侧内部顺序，包括未发后缀，避免恶意排列使游标跳过或重复元素。业务排序仍由 guest 决定。

过滤结果必须是有序无重复子集，排序结果必须是完整无重复排列；外来 ID、内容输出、缺项或乱序直接失败，无部分成功。过滤允许排除候选，调度器无法单凭响应判断假阴性。标题按 UTF-16 稳定排序，收藏并列和同标题保留反 ID 顺序；最近添加沿用反 ID 枚举，并非创建时间。

空库显式核对当前选中包、读取能力、活动实例及条件，并执行一次真实过滤任务。原有无插件的本地内容读取保持。响应超过128 KiB时用新的有界帧返回错误，丢弃整个结果，不在大 Builder 上清空指针后继续发送；错误不宣称此前操作未执行。

query_plan::replay 验证计划版本、阶段和原请求字节，用全部原响应重建最终 ID；缺失、额外、调换和不匹配观察拒绝。计划行为及所依赖的 command 默认值、codec/persistence 规则变更必须另定版本。Observation 是调用者持有的原负载，不自动产生签名、执行或来源证明。

## 专项证据

- 独立调度器9项通过：真实 Rust 业务函数作为后端，4096条三种排序及 Unicode UTF-16 对照、5000候选4命中、192个16384字节标题迫使小窗口、空输入、异常响应与完整序列篡改。使用独立全局稳定排序作为参照，未复刻分块实现作为预期。
- 实际 SDK 多步资格通过：160个合成候选走真实 Pool.record_transform，覆盖过滤、初始排序及归并；所有调用与 Evidence 一一对应，关闭并删除原宿主库／包目录后逐个隔离重放实际 Wasm，再重建相同计划结果。候选为显式合成夹具，不冒充真实数据库完整读取证据。
- 实际空查询能力测试通过：无包、停用和缺少 ReadContent 均拒绝；重新启用可查询；未知 section/sort 在空库也失败。
- 实际子进程传输通过：600个长 ID（小于4096条）触发超帧，返回小于1 KiB错误且无部分IDs/payload；同进程继续完成分页、精确查询和修订读取，正常退出。

专项日志 `build/test42-query-actual.log`、`build/test42-query-transport.log`；独立调度器构建目录 `build/query-plan-review`。

## 全量及产物

本阶段结果为 **PASS_SCOPED**。核心、审计、运行时未改生产源码，不重跑未受影响的整套验证，也不将历史通过数冒充本轮结果。

| 范围 | 实测结果 |
| --- | --- |
| 宿主完整 Release＋fault-injection | 88项通过；独立清单88项；全目标严格 Clippy 通过 |
| 生产 Rust bundle | 实际 guest、宿主及不可变包构建核验通过 |
| Flutter 与生产宿主 | 39项通过，包含查询协调器、编辑器及真实原生插件交互 |
| Windows Release | 构建62.6秒通过；实际版本0.1.9-test.42+47，退出码0，四项自检通过 |

日志 `build/test42-host-{full,clippy,list}.log`、`build/test42-bundle.log`、`build/test42-final-flutter.log`、`build/test42-windows-build.log`。本轮未运行新的 Web／Android 构建或设备验收。

实际程序 `build/windows/x64/runner/Release/morrow_studio.exe`，需保留整个 Release 目录。宿主及插件包哈希与生产 bundle 相同。

合成库与实际截图位于 `build/workbench-host/test42-final-4f58d1dc4a014f29ae31047b3b50a83f/`。`result.png` 已目视检查：实际工作台、侧栏、外观面板和卡片正常显示。自检验证桌面合成API接受 blur 0/1/12/40及关闭、静音WAV真实解码及播放时钟、定位／互斥／恢复不自动播放、实际Rust工作台渲染无Flutter错误；不是桌面背景像素对比。

检查元数据 `build/test42-final-verification.json`：库格式11、SQLite integrity为ok，6份内容Evidence和6个引用、34个唯一块／144个块引用，无缺失或孤立块。此处是既有内容证据，不能解释为默认查询已经自动写入读取日志。

| 产物 | SHA-256 |
| --- | --- |
| morrow_studio.exe | `708d2fbdef1d9932a2bbdc195999bdd8cfe144e0dbbb32383ddccc5a8034236b` |
| morrow-workbench-host.exe | `e1ce64c45a0a004c174156bda029dc3bc4da8ddfa838792e805d947b285f4073` |
| workbench.morrowplugin | `6904464ac8e041ec0b0bd298200bd402d6eac7b1929faa8ba8d6319a6b165fe5` |
| morrow-content-replay.exe | `2e5ff284a0bd34d3d0b24c5d734aa944706949538169791d9f1b95b61d5b8d98` |
| morrow-audit-check.exe | `42f481f2ee25477b54a092f5c7c2b0e261e621722324d9d97b76a33bd06b986b` |


## 仍未完成

默认候选当前仍由多次分页／逐卡授权读取组成，不是同一 SQLite 快照；默认查询也没有自动写入读取日志。完整存证需要一致来源、所有类型及终结事实、冻结卡片授权、全部调用分片和原子最终目录，再接稳定操作、历史重试及按修订交付。详见[查询捕获设计](../docs/QUERY_CAPTURE_DESIGN.md)。

分块归并减少短键调用，不保证长键最坏情况低于1024观察或16份Evidence；不能以达到现有证据预算为由裁掉候选或省略调用。4096是协议结果数限制，128 KiB是独立字节限制；本轮修复恢复性，不宣称所有4096个长ID都能单帧交付。完整 M3–M7 和0.2.0门槛仍在推进。
