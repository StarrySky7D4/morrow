# test.41：独立读取审计底座与查询响应协调

日期2026-09-13。应用 `0.1.9-test.41+46`；核心／审计／工作台宿主test.41，运行时／SDK test.11、guest test.13。内容库格式11，第一方AGPL-3.0-only。仅本地开发，不推送、打标签或发布Release。本阶段验证为 PASS_SCOPED：下列全量回归及 Windows 实际产物检查通过，完整插件系统与跨平台验收仍未完成。本阶段未增加 unsafe；现有受控边界保持。

## 实现与范围

新增明确类型的读取观察，通过预编译Protobuf＋LZ4保存原请求／响应、摘要和有序Evidence引用，以kind4进入同一operations／outbox／永久事件索引；最终可信guard和原件／引用／块在一笔事务提交。不是隐藏Card或内容空修改，正文修订不变。签名、独立验证器、完整性和快照已显式接入；DB10→11原子门禁保留历史原始字节。

默认query目前仍未自动存证。本阶段为必要存储基础及真实SDK资格，不声称完整候选清单、归并观察、权限连链或用户已收到。详见[设计及边界](../docs/PLUGIN_READ_JOURNAL.md)。

Flutter查询新增200ms防抖、单调serial、结构化条件／内容generation／后端失效，A→B→A不能采用旧响应；只合并尚未发出的查询，已发任务继续完成。等待或失败不把旧结果显示为已确认，重试按钮实际发起请求，组合输入暂缓发送。

## 已完成专项

核心读取底座16项通过（15实质＋1子进程入口），严格Clippy通过。覆盖codec有界分配、typedlookup／跨kind／同ID冲突、guard拒绝、真实deferred外键COMMIT失败、六个读取提交崩溃点及两个迁移崩溃点、原件共享和损坏、逻辑64MiB及超长SQL载荷。迁移保留原Evidence raw／container／digest与已签名段；没有通过缩短测试或把旧格式样本改成新格式来获得通过。

审计新增3项通过，完整审计Release＋fault-injection 60项通过，全目标严格Clippy通过、独立枚举60项。实际读取合成Card后形成独立读取事件，与Create混合封为两段共3个事件；原件字节进入签名，实际morrow-audit-check核验core-store／独立archive。删除原库与密钥后，从快照恢复sealed与pending读取原件、旧签名及原保护身份；篡改subject索引使只读、Session和CLI拒绝。CLI将计数文案改为audit events，避免把读取事件统称内容commit。

真实SDK资格1项通过：从两张合成卡片收集输入，实际执行Rust Query标题排序，经Pool捕获原调用与结果，写入读取日志并签名；卡片原字节及revision不变。快照后删除源DB／catalog／registry／凭据，在恢复库取回相同原件与Evidence容器，隔离重放实际guest通过。该测试为单次显式Query转换，不冒充默认Workbench全分页查询的完整证据。

## 全量与Windows结果

| 范围 | 实测结果 |
| --- | --- |
| 核心完整Release＋fault-injection | 270项通过；独立清单270项；全目标严格Clippy通过 |
| 审计完整Release＋fault-injection | 60项通过；独立清单60项；全目标严格Clippy通过 |
| 宿主完整Release＋fault-injection | 76项通过；独立清单76项；全目标严格Clippy通过 |
| Flutter与生产Rust宿主 | 39项合并回归全部通过，包含查询4、编辑器10、旧宿主5与富内容／功能20 |
| Dart与Wasm | 全项目analyze零诊断；核心普通与web-storage两种Wasm配置编译通过 |
| Windows | Release 构建通过（76.3秒）；实际应用退出码0，四项自检通过 |

日志 `build/test41-core-full.log`、`build/test41-core-clippy.log`、`build/test41-core-test-list.log`；审计与宿主分别为`build/test41-{audit,host}-{full,clippy,list}.log`。Flutter `build/test41-final-flutter.log`、`build/test41-query-analyze.log`，生产bundle `build/test41-bundle.log`。子进程执行不重复计数。未重跑完整运行时测试，也未用Wasm编译替代浏览器运行验收。

完整核心首轮在旧fresh/current DB版本断言遇到11项预期10／实际11的失败，日志保留 `build/test41-core-full-initial.log`。仅更新当前或迁移最终版本断言及一处旧库迁移后的最终版本；原9→10中间故障结果10和故意降级样本保持。随后完整270项重跑通过。新增独立夹具曾有一次局部变量替换的编译错误，修正后16项及全量通过，未改变生产类型来适配测试。

查询协调器额外验证实际重试显示Confirmed card，再改变搜索时旧卡从当前结果消失；输入listener立即选择新条件，避免等下一帧时旧防抖先发出。只修改读取协调，不声称取消已运行guest或证明屏幕内容修订。

## Windows 实际产物

实际版本 `0.1.9-test.41+46`，程序位于 `build/windows/x64/runner/Release/morrow_studio.exe`，运行时必须保留整个 Release 目录。实际宿主及插件包与生产 bundle 的 SHA-256 一致。

独立合成库及截图：`build/workbench-host/test41-final-43ca16c097c04ec49f84b4dfd85e37ab/` 下的 `result.md`、`result.png`。实际截图已目视检查：工作台、侧栏、外观面板及卡片正常显示，无持续加载或错误页。自检覆盖桌面合成 blur 0/1/12/40 与关闭、静音 WAV 解码及播放时钟、定位／播放互斥／恢复不自动播放、Rust 工作台渲染。合成效果自检是 API 验证，不能代替桌面背景像素对比。

合成库格式11，SQLite integrity 为 ok；6份内容 Evidence 原件及6个引用，34个唯一块、144个块引用，缺失块及孤立块均为0。这些内容证据不代表默认查询已自动写入读取日志；读取日志资格由上述独立核心、审计和 SDK 测试证明。

构建日志 `build/test41-windows-build.log`；产物及数据库检查元数据 `build/test41-final-verification.json`。

| 产物 | SHA-256 |
| --- | --- |
| morrow_studio.exe | `9023d24d960da8504081678f66c07d16957de8df0f6e5d1fa90a44a3c1192ff0` |
| morrow-workbench-host.exe | `b0192c96afc875cd93024538f11bfc8e8387a8e9f674b14ead6ed89923ebdfe2` |
| workbench.morrowplugin | `444553e22f998fc88258989d03ae13bdf060b3008b2171c391296b7439c9fd0b` |
| morrow-content-replay.exe | `6e4bd280325d5f8332393a6234a51c96dbb2bdde8ae83523b219f8d9e31e10f0` |
| morrow-audit-check.exe | `42f481f2ee25477b54a092f5c7c2b0e261e621722324d9d97b76a33bd06b986b` |

## 后续

下一阶段将默认查询接入读取底座：固定已授权候选与读取时点、保留全部真实过滤／排序／归并观察、版本化最终结果投影、稳定重试与分块取证；还需解决读取完整性与当前交付政策、错误／取消、响应帧容量及规模性能。其余签名提交连链、依赖图／响应、全局配额／保留／GC、未提交恢复／检查点与六平台运行验收继续推进。完整M3–M7及0.2.0门槛未完成。
