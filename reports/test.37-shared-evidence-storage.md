# test.37：原始任务证据的物理分块共享

日期：2026-09-13。应用 `0.1.9-test.37+42`，审计／工作台宿主 test.37；核心 crate仍为test.22，内容库格式由7升至8；运行时／SDK crate test.11，默认工作台guest test.13，随包清单test.37。第一方AGPL-3.0-only。本轮仅本地开发，无推送、标签、Release或上传。

## 实现

为减少每次内容任务重复保存内嵌插件包，将原Evidence.container按固定32KiB切块。`task_evidence.payload`改存版本化Protobuf＋LZ4配方，绑定原证据摘要、原容器长度／摘要和有序块摘要；`evidence_chunks`保存版本化Protobuf＋LZ4块原件，`task_evidence_chunks`保存有序反向引用。物理共享不改变TaskEvidence、原始PB、原容器、Commit或签名格式。

配方、必要的新块、有序链接与内容／回执／事件位于同一提交事务。复用块同时检查摘要、原始字节和既有关联，不能收编孤立块；逻辑预算仍是每操作最多16份、原PB＋原容器合计64MiB，重复位置继续计入，不能靠共享绕过限额。

读取先以SQL CASE检查长度，再按固定schema检查字段计数、重复字段、线类型、版本、块数量和大小；校验配方与数据库有序链接、每块长度／摘要，重建原容器后再按原证据摘要解码。完整性检查拒绝缺失、额外、孤立、错序和超限记录。没有可变共享指针或新增unsafe。

格式7先校验旧库，再在同一事务建共享表、按摘要逐份读取旧原件、切块、立即比对重建原容器／原PB、更新版本并完整检查。单份处理有界，不一次收集整库原件。4／5／6仍经已有迁移阶段到7，再进入8；各阶段原子，不宣称全程一个事务。只读入口支持5／6／7／8，旧格式7只读不迁移，旧核心不支持格式8写入。

## 验证

- 最终核心Release、fault-injection完整回归 **240项通过**，独立--list核对240。首轮239项通过后补充真实共享快照闭包测试，并纳入最终完整回归。全目标严格Clippy及新增快照测试严格Clippy通过。日志 `build/test37-core-final.log`、`build/test37-core-final-list.log`。
- 新 evidence_chunks 组 **9项通过**（8个实质场景、1个子进程入口）。14类损坏覆盖缺块、额外链接、同步篡改配方和链接的错序、超限长度／数量／块、错误块sha、空块和错误容器sha；原件中的未知PB字段也逐字节保留。
- 真实签名格式7库只读不迁移，迁移后原Commit、容器、PB、digest及原签名段均完全相同，并可继续新写。内容写块后／提交前／提交后和迁移提交前／后共5个真实进程中断边界通过。
- 快照闭包单独验证：包含两份共享块证据的真实签名库snapshot_to后，关闭并删除合成原库，只用快照与固定TrustedLog只读打开；全部原container/raw/digest及原签名段逐字节一致，快照内仍有实际共享块。未用普通无证据快照测试代替该证明。
- 审计Release、fault-injection全量 **57项通过**，宿主同配置全量 **37项通过**；两者全目标严格Clippy通过，独立清单数量一致。日志 `build/test37-audit.log`、`build/test37-audit-list.log`、`build/test37-host.log`、`build/test37-host-list.log`。不重复计数子进程。
- 宿主审计组5项全部通过，包含1100次实际创建、自动封存、只读检查及重开，组耗时131.89秒。运行期间有并行编译，不能与test.36组的139.65秒作为严格性能对比；此次主要改善存储重复，完整性检查仍逐份核验任务原件，CPU和规模性能继续优化。
- 核心普通wasm32-unknown-unknown与web-storage配置编译检查通过，不宣称浏览器迁移或运行通过。真实Flutter／宿主集成 **5项通过**，Windows生产包构建、Release编译与实际应用自检通过。没有UI变更，未重复整套UI测试或把旧Runtime全量结果记成本轮新结果。

三名子代理分别实现共享物理模块与schema、独立损坏／崩溃／快照测试、真实工作台度量工具；主代理负责版本门禁、迁移调用、逻辑预算与存储集成、全量检查、文档和最终Windows产物。

## 真实工作台容量和重放

新示例 `workbench_host/examples/qualify_shared_evidence.rs` 使用最终test.37默认包，普通Release配置、不启用故障注入。它完成30次create和1次Favorite编辑，保留31份原件，正常finish封存后读取合成SQLite统计；重开核对每份raw/container/digest，重复所有创建返回历史revision1而首卡当前revision保持2，错误意图拒绝，记录数量、文件大小和原签名段不变。删除源临时库、目录及密钥后，31份独立纯任务重放全部MATCH。

| 实测项目 | 字节／数量 |
| --- | ---: |
| 逻辑证据原件／操作 | 31 |
| 独特块／有序块引用 | 84／744 |
| 等价格式7完整原容器载荷 | 24,017,368 B |
| 格式8配方PB＋LZ4载荷 | 29,295 B |
| 格式8独特块PB＋LZ4载荷 | 2,336,959 B |
| 配方及块载荷合计 | 2,366,254 B |
| 载荷减少比例 | 90.15% |
| 完整新建SQLite文件 | 2,744,320 B |

比较基准是实际31份原容器的字节总量，不是捏造的旧SQLite文件；新文件大小包括内容、索引和签名段。该比例只适用于这个实际样本。预研也用真实不同长度task/card ID确认中间块可共享；跨包版本、字段布局和小型原件不保证相同比例。首块容器头及尾部观察通常不同，所以不是按包身份保证整个包只保存一次。

最终资格及严格检查日志：`build/test37-shared-evidence-qualification-final.log`、`build/test37-shared-evidence-strict.log`。早期示例编译遇到SQL FromSql不接受u64和Package不实现Clone，改为i64检查转换与decode原archive，未改生产接口；最初编译证据保留在非final日志中。

迁移不自动VACUUM。旧库释放的SQLite页可以复用，但现有数据库文件不保证立即缩小；不能把新建库测量推广为旧文件立刻压缩90.15%。本轮不访问用户资料库，不宣称完成用户库迁移验收。

## 最终Windows产物

实际版本 `0.1.9-test.37+42`，自检退出0，合成活动库登记存在；目录 `build/workbench-host/test37-final-5ce5f29312ab49b796aa35a1b8574dec/`。结果保存于 `build/test37-final-verification.json`。

自检后只读核对：SQLite完整性 `ok`、格式8，5份原件配方／5次操作引用、32个独特块／120个块引用，无缺块。配方4,725字节，块容器969,518字节，数据库文件1,196,032字节。

实际Release通过Rust工作台渲染、背景blur 0／1／12／40 API及关闭、静音WAV解码与播放时钟、跳转、互斥及恢复不自动播放。背景项是API检查，没有桌面像素比较；运行须保留整个Release目录。

| 包内产物 | SHA-256 |
| --- | --- |
| `build/windows/x64/runner/Release/morrow_studio.exe` | `95ac82709e4f945d5f54db818fc046e7ccb0e380b57620734e3f0a8dc923407c` |
| `build/windows/x64/runner/Release/morrow-workbench-host.exe` | `71d46922c0868bf6f9518f2ebe2b0a6ac9d076cc2be1df466395917b1b3df294` |
| `build/windows/x64/runner/Release/plugins/workbench.morrowplugin` | `af8e930c00f1079c0fb7b06ae5587f538f938b483ad7fd5b9c90d17cf58e7439` |

## 后续门槛

下一阶段继续设置最多649页的有界批量证据、原保存意图和总执行预算，随后推进capture→create与查询因果关联、宿主投影重建、完整依赖图／响应证据、全局配额／保留／GC、异步恢复、独立检查点与六平台运行验收。共享存储没有自动完成这些功能。M3–M7与0.2.0退出门槛保持未完成，本阶段为Windows原始证据共享存储PASS_SCOPED。
