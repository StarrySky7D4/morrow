# test.44：有界读取分片、原子目录与逐条执行原件

日期2026-09-13。应用 `0.1.9-test.44+49`；核心、宿主、运行时、审计和打包清单test.44，SDK仍test.11、guest仍test.13。数据库格式12。第一方AGPL-3.0-only，无新增unsafe；本阶段仅本地开发，不推送、不打标签、不发布Release。

## 实现范围

- 核心新增预编译Protobuf＋LZ4归档Plan／Part／Manifest、独立预算与有序原件哈希链。准备阶段不生成读取事件，provisional ID不抢占全局内容操作命名空间；重复相同追加幂等，错序／冲突拒绝。
- 完整分片、最终目录、ReadObservation schema2根引用、Evidence引用、永久索引和待签名事件同事务发布；最终当前guard与每次显式重试均执行。CommitUnknown保持可查询状态；abort不能删除已发布原件。
- DB11→12在独立事务前后校验全部状态，历史字节和签名保持。独立只读、审计、快照与恢复检查缺片、错根、孤立引用、错误归属及损坏原件；旧格式读写迁移夹具按真实旧表结构验证。
- 运行时新增成功调用逐条捕获：包摘要独立关联，保留实际Invocation、完成帧、预算和消耗；当前实例／包绑定前后检查，失败不伪造成功记录。独立有界Observation编解码保留unknown原字节，重放不接收HostRuntime、DB或恢复grant。
- 资格适配器记录真实130张卡查询的过滤／分段排序／归并和空库实际Filter；保存冻结Card原件、来源事实、读点、EOF census、每次实际调用和最终宿主ID列表。删除自有源库与凭据后从签名快照恢复，并重新读取恢复库中的包和全部回放输入，核对每次完成原帧、完整调度、总燃料和最终ID。旧ID仅作比较oracle。

## 独立审查与真实失败记录

三个子代理分别实现核心、编写独立核心测试和审计恢复测试，并交叉审查根代理的实际捕获资格。独立审查发现非末分片仅检查局部前驱不能证明归属最终root，已改为同一读取事务完整核验；另补损坏published→pending状态仍不得清理原件的闭包。两项都在对应回归首次运行前修复，未声称取得旧实现失败日志。

根代理实际首轮多页资格1PASS／1FAIL：独立重算census时把字符串长度误写成u64，原契约为u32。修复该资格代码；独立审查同时指出跨页最终130项不能使用128项guest响应解码器，新增独立宿主QueryResult PB，保留130张规模。重跑两项及全宿主通过。

首次核心全量在旧evidence_chunks夹具出现12项失败：当前版本断言仍11，降级夹具保留新三表。修正最新断言为12，并在构造旧格式时移除新表；10→11故障中间态保持10／11。全量重跑通过。首次宿主Clippy拒绝同一common.rs被重复作为模块加载，改为共享测试模块；严格检查和12项lib测试复验通过。原失败日志保留，不改写为首次全PASS。

## 验证记录

| 验证 | 结果 |
|---|---|
| 核心 release + fault-injection 全量 | 309项 PASS，新增分片专项21项包含1个故障子进程入口 |
| 宿主 release + fault-injection 全量 | 95项 PASS；共享测试模块调整后12项lib复验PASS |
| 运行时 release + packages 全量 | 214项 PASS，新增逐条捕获2项 |
| 审计 release + fault-injection 全目标 | 63项 PASS，新增实际签名／CLI／恢复3项 |
| 四部分全目标严格Clippy | PASS，`-D warnings` |
| 核心Wasm普通与web-storage配置 | 两种`--lib`编译PASS；不代表浏览器实际运行 |
| Flutter实际宿主集成等8个测试文件 | 39项 PASS，使用本轮实际Rust宿主及打包插件 |
| Windows Release与实际自检 | 构建48.5秒PASS；实际test.44+49，4项自检PASS、退出码0 |

唯一测试数量来自各`--list`日志，不重复计算嵌套子进程输出；包含测试框架中的故障入口。分片独立测试含12个实际exit86故障边界、append/final两类真实deferred-FK CommitUnknown、超过旧64MiB限额的独立预算、同操作冲突和篡改拒绝。审计使用合成typed parts，不将其冒充实际guest证据；实际guest由根代理资格单独覆盖。

日志：`build/test44-{core,host,runtime}-full.log`、对应`-clippy.log`和`-list.log`；`build/test44-audit-full*.log`、`build/test44-host-lib-final.log`、`build/test44-read-archive-post-review*.log`、`build/test44-final-flutter.log`。首次失败保存在`build/test44-query-archive-initial.log`、`build/test44-core-full-initial.log`、`build/test44-host-clippy-initial.log`。

## Windows实际产物

实际构建截图已查看：`build/workbench-host/test44-final-5c2ff14bc9e24ad9b2b5a5b8730f5389/result.png`，正常显示侧栏、设置栏与Rust工作台卡片，无加载占位或Flutter错误画面。四项自检分别为原生合成blur 0/1/12/40与关闭API、实际静音WAV解码时钟、seek／播放互斥／恢复不自动播放、实际工作台渲染。透明效果此处只验证API，不声称完成桌面背景像素比较。

自检库格式12、SQLite integrity_check=ok；6份内容Evidence、6次引用，34个唯一物理块、144次块引用，缺块与孤块均0。该库归档三表计数均0，明确不是默认查询自动存证的证据。元数据：`build/test44-final-verification.json`；5个SHA256已重新核对，Windows随包host/plugin分别与实际生产构建相同。

| 文件 | SHA256 |
|---|---|
| `morrow_studio.exe` | `a29755950d325d9228edb81b4d355b58c6c75d6bc360983bb2d8716e41aac973` |
| `morrow-workbench-host.exe` | `362713552f327de4108bc2fc53f77438da54603a9cede4723e81279fed1a5abb` |
| `workbench.morrowplugin` | `a1082b45ac4b31cd6c7ee2686237aeb0858bbf98697ddc85c0f94e9f64eaf02b` |
| `morrow-content-replay.exe` | `613aede0d8392568155f06992da5f7a1a19141b5605dd570a77c869933fb2b78` |
| `morrow-audit-check.exe` | `706511fe7549bfd877e8cb3edde6a811de030a44b41fb18d0c2798243b4e2c72` |

## 尚未完成

默认Workbench::query保持test.43的一致快照读取，尚未自动存证。当前分片单次读取O(N)，全遍历O(N²)，需固定快照的已验证游标；全局暂存数量／容量、过期清理、保留与GC尚未建立。单归档显式上限不是无限候选／全部理论长键查询都可持久化的承诺。

查询全程和结果交付权限、稳定查询上下文、未提交失败／取消恢复、结果历史修订映射、独立查询CLI、Web一致来源和依赖图完整证据仍待完成。签名及census不独立证明来源从未遗漏、历史授权或用户已收到。见[归档契约](../docs/PLUGIN_READ_ARCHIVE.md)与[未来路线](../docs/FUTURE_ROADMAP.md)。本阶段结论为PASS_SCOPED，完整插件系统和全平台重构目标继续推进。
