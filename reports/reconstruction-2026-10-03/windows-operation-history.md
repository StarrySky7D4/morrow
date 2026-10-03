# W13：Windows 只读 HTTP 操作历史查询

2026-10-03。当前分支 `codex/windows-sdk-qualification-20261003`，HEAD 保持 `63f38d4a8a5bf453248dc7532197bee6980dc86f`，本阶段未提交／推送／CI／发布。继承 W12 pending tree `f65cda4ce36257ae6082ce8a77571e5e4dd501ac`，SDK327、冻结57个输入原件、Schema、锁与 Linux 汇合源码保持原身份。阶段16遗失增量未恢复。

## 实现结果

既有 QueryOperation Schema 现有显式 managed 只读路由；新增 OperationHistoryGrant、严格的无正文 OperationOutcome 回复、Store 只读历史投影和最终交付的权限复核。使用单个原 command 的完整 pins 与当前批准，只读查询不 dispatch、不 prepare 新效果、不修改历史、不自动 replay Unknown。原 raw／brokered 路由仍拒绝 QueryOperation。

Core 只在归属可证明后解码有界历史；Observed 必须核验真实原 request／response、payload SHA 和新旧关联。人工 reconciliation 或不可验证原帧返回 EvidenceUnavailable，跨 subject／无法证明归属返回 NotFound。实际完成仍返回真实 HTTP 状态，503 不被伪造为200。

Runtime 使用原 `morrow_io_v1.call`，先预付有界回复预算，交付时复核原时钟、owner、generation、live guard 和 cancel。类型化状态与精确回复帧同进同出；历史 Unknown 与当前只读调用产生新效果分开。开发者接口与限制见 [查询指南](../../docs/PLUGIN_OPERATION_HISTORY.md)。

## 实际运行

离线、锁定、Windows x64 Release；分别使用 `qualification-agent/w13-history-isolated/core-target` 和 `runtime-target`，每个 execution 使用新的 synthetic-temp。白名单环境，不继承 MORROW 配置／真实用户库路径。下表是最后一次对应闭包的实际方法数，累计 **144**，不累计此前重复通过的方法。

| 最终执行 | 实际通过 | fail／ignore／measured | 原始退出码 | 编译＋执行时间 |
| --- | ---: | --- | --- | --- |
| `core-execution-006` | 50 | 0 | `0` | 9.000 s |
| `runtime-execution-003` | 84 | 0 | `0` | 42.625 s |
| `runtime-unit-execution-002` | 10 | 0 | `0` | 8.422 s |

- `core-execution-006`：io_codec: 23, io_intent_matching_lookup: 2, io_intent_store: 9, io_operation_history: 16。
- `runtime-execution-003`：http_io: 5, io_binding: 9, io_execution: 18, io_execution_gates: 1, io_jobs_brokered: 11, io_jobs_managed: 12, io_operation_history: 16, sdk_frozen_compat: 9, sdk_frozen_dependency: 3。
- `runtime-unit-execution-002`：io_history::tests: 10（其它 lib methods 被显式过滤）。

Core 新增16、Runtime 实际 Wasm 新增16、grant module unit 新增10，合计42新增方法。Integration suites 无过滤；unit 只选 `io_history::tests` 的10项，其它 lib tests **NOT_RUN**，未宣称整个 Runtime lib 通过。

冻结回归包含原9 base guest和原3 Rust／C／C++ dependency cases；原 `.wasm`／`.mplugin`／provider 未重建或重打包，只编译当前宿主测试程序。新查询 guest 是当前合成 WAT，不能代替原件，也不证明三语言新的历史查询任务均已执行。

所有最终执行的源码／SDK327／原件57／工具前后 SHA 恒同，各自 TEMP 文件残留0。原运行日志／退出码／环境／输入／实际 exe 身份位于本地 `build/windows-sdk-20261003-evidence/w13-operation-history/<execution>/`，报告结果身份如下：

- `core-execution-006/result.json`：SHA256 `04dc23548d5de36809cc08e8dc40346cd9feff223fbbe4f616707b1994c4edfd`。
- `runtime-execution-003/result.json`：SHA256 `39d22cec5421d24dfa027faf5a40a74a00d2f733714a834db1d184a724338485`。
- `runtime-unit-execution-002/result.json`：SHA256 `351f302ae108d4fd1c231c8230b8a2ee9d763217bde877616b9a5190bf6d6916`。

## 修正和保留的失败

执行前的源码交叉审查发现两点：回复256字节已预付，却曾在局部 report 再计一次；历史观察应核验既有 producer 的 payload SHA，不能混用 Material.digest。两者在第一次执行前修正，未声称执行过对应的红测试。另增加既有 Broker→合成503→持久Observed→新Query 的实际链，backend始终为1；手工 Observed fixtures不算真实 producer。

- `core-execution-001`：exit101；实际 33 方法通过，1方法失败；stdout SHA256 `adbc357ca9f126645fb2712bdbafc61e632178e8944733dd5b1d9a14e222ccbb`，stderr SHA256 `2d4afdab87968630107acb62f9ab6ef3064739979cf90490489f5f5f45fa265e`。
- `core-execution-002`：exit101；实际 49 方法通过，1方法失败；stdout SHA256 `6ebda9f1b854a2a9cbed9fd0f7a76a86ed80e18d8f040f28595cb04e0d8d634f`，stderr SHA256 `3940bb662428c0d44408d89a44eee1bfef623b066b2e6c33e69bd875ad17133c`。
- `runtime-execution-001`：exit101；实际 71 方法通过，1方法失败；stdout SHA256 `2a1f81a89e0b1e9e834575e5c8bb9902367db517d3a0017f60d0e1cdf6ab7098`，stderr SHA256 `71b011395415d88ff5104dc72e367ba71611d1d99eb1cff9023d8829e40de7dc`。
- `core-execution-004`：exit101；实际 0 方法通过，编译失败，测试未运行；stdout SHA256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`，stderr SHA256 `bb12ddad207ccce04b886009a6e3cfb7689fed793d9cda717da83c4d37fe31a5`。
- `runtime-unit-execution-001`：exit101；实际 0 方法通过，编译失败，测试未运行；stdout SHA256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`，stderr SHA256 `3cad43fd61ffa1ab71a3c163c2a69b040a56617d4d2cf2e9b7498f7fff65ccd0`。

第一轮失败来自既有 v14 migration fixture：当前v24合成库降标时遗漏两个channel表，生产完整性校验正确拒绝。只修 `core/tests/io_intent_store.rs` 的 downgrade SQL，先删 child ack receipts再删 parent checkpoints，旧断言与生产迁移／校验均保留。

第二轮失败来自新损坏历史负例的准备：修改event外键为不存在的目标被SQLite拒绝。仅在那条合成外部连接的定向损坏步骤关闭FK再恢复；其它测试与生产连接未变。失去初始ownership事实后，同subject也按无法证明归属返回None；原本提出的EvidenceUnavailable断言在静态审核中纠正，未编造额外失败运行。Core execution-003首先真实50项通过；不将重复方法累计计数。原日志与修正 diff 分别保留在 `core-failure-review-001/002`。

复用同一target先编Runtime再编Core后，core-execution-004出现同版本prost trait身份不匹配的编译错误，0测试执行；Core源码和锁并未改变。未改源码／锁／trait、未删除原缓存，只让两个manifest分别使用新target；最终core-execution-005在新core-target真实50项通过。target复用与该错误的关联作为本机观察，未宣称已定位Rust／Cargo内部根因，见 `core-target-review-001`。

Runtime第一轮失败是第三预算负例给worker的总上限大于manifest允许的总上限，被既有spawn校验拒绝。只修新fixture：合法且相等的worker／declared上限，再用原binding admission预先消耗不退款的IO域预算，验证worker尚有空间、IO域差1字节时查询在lookup前拒绝。未放宽生产限制，见 `runtime-failure-review-001`。

Runtime execution-002先真实84项通过。新增grant unit的第一次编译则发现cfg(test)模块把manifest声明函数解析成Core IO命名空间，0测试执行。仅给测试内manifest IO模块设明确别名并修改2个测试构造调用，生产函数体／API／Schema保持，原编译错误保留。最终grant unit002、Core006和Runtime003均绑定修正后的源码字节，不累计重复通过数；见 `grant-unit-failure-review-001`。

第一轮Runtime还观察到153个合成TEMP文件残留。保留其路径、SHA与原目录；结构核查发现旧4个suite的5个fixture把TempDir放在handle字段前，Windows删除发生在释放句柄之前。只将这5个TempDir字段移至末尾，保留全部显式stop／join与旧断言。首次路径归属属于结构／时序推断；最终重新执行的独立TEMP为0残留，并不抹去旧153。没有改生产Drop或清理旧测试数据，见 `runtime-residual-review-001`。

`cross-source-review-001`／`cross-test-review-001` 是第一轮前的内部交叉静态审查，绑定其当时源码SHA，不冒充最终测试身份。后续测试准备与注释改动须由最终证据复核单独绑定；这不是外部认证。

## 范围和继续工作

该阶段推进 G07 的单个HTTP历史状态，不完成统一重启恢复或完整插件SDK。查库中途撤权、真正fresh owner重新批准恢复、三语言新查询guest、正常用户token、protected owner／UI、其它平台均未新增资格。实际 child token未采样；elevated orchestration不能被称为普通用户运行通过。没有使用真实数据库／DPAPI／账号凭据／公网服务，也没有运行 GUI 或重新构建完整 Flutter 应用。

G01生产channel资格、G02公开SSE／WS bridge、G03账号、G04完整文件IO、异步组合、内容cursor及各平台继续OPEN。W12 SSE只是传输层；下一片公开网络源必须覆盖获批source、Receive／ACK真实背压、queued revocation和durable claim，不能把传输EOF或send完成当成消费ACK。

交接包含本阶段源码增量、当前累计源码身份、真实日志、temporary-index从63f原base恢复的tree等价证明及未改写W12整包；不含Cargo target／用户数据／合成DB内容，旧残留的身份清单保留。源码树身份来自继承manifest＋19显式变化，未在本阶段重新逐文件扫描整个51k树。最终tree／ZIP SHA由独立交付回执提供，SDK整体仍OPEN。
