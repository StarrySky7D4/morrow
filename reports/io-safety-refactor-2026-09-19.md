# Track A IO 修补与局部重构验收

日期：2026-09-19。结论：**PASS_SCOPED**。保留候选的 Store 材料、执行、作业和 HTTP 编解码增量；局部重构实时身份和结果交付，其余定向修补。原候选不可直接合并，本报告只评价修正后的隔离开发分支。

## 固定来源与范围

- 开发基线：`457e023533c7a3129da5a6dbd24991d9d9455142`，`codex/refactor-test.51-network`。
- 候选来源：`track-a/w1-io-contract` 的 `dab3e7089b5fac7b33ba681fc8d5d08bb4d7c73a`，tree `7d4aacf17a7f37033c91c10355477421bbaafc3d`。
- 先复核隔离来源的 1012 个原文件与 Git blob 完全一致，再定向移植 44 个代码／相关说明文件；没有整分支覆盖或直接解决文本冲突后放行。
- 修正分支：`codex/io-safety-refactor`，工作树 `build/io-safety-refactor/`。参考快照保持原样；新增复现已归入正式测试，删除重复 review 测试副本。
- 保持应用版本 `0.1.9-test.52+56` 与冻结 guest SDK；本轮不发布安装包或 Release。数据测试仅使用合成临时库。

## 原审查失败的闭环

| 原失败范围 | 修正 | 正式覆盖 |
| --- | --- | --- |
| 执行：停止后仍交付成功、错误包摘要仍可绑定（2项） | 命令绑定真实包摘要；采样实时宿主时钟，在提交、调用和最终交付复核原实例；Observed 事实保留与敏感结果交付分开 | `plugin_runtime/tests/io_execution.rs` |
| 作业：Ready后取消／停止仍成功、外部撤权仍路由、公开限额绕过（4项） | 所有状态统一受控，read最后核验，实际连接撤权贯通；spawn复验硬限额与声明预算 | `plugin_runtime/tests/io_jobs.rs` |
| 材料：取消终态预留漏检、请求载荷绑定漏检、损坏材料幂等误成功、原容器被重压缩（4项） | 按实际末尾phase判断；共用有界完整载入；重试核验已存内容；保留接受时的LZ4容器原字节 | `core/tests/io_evidence.rs` |
| HTTP：认证头、无效结果状态、控制头值被接受（3项） | 编码和手工构造帧解码双向拒绝；额外覆盖Cookie、全部proxy-*、DEL、歧义目标 | `core/tests/io_codec.rs` |

上述13项失败场景均已转为正式回归并通过。附加校验包含 SQL 分配前大小限制、满事件计数但仍有材料字节空间、Ready背压、ReadBound、运行中丢弃／退休不提前释放槽位、期限和外部撤权。交叉审查发现的 drain 过早断开连接导致 Ready 丢失也已修正，温和排空允许有效结果读取／丢弃，超时后不让无载荷元数据阻塞收尾。

## 验证记录

运行环境：Windows，本地 release 测试，离线锁定依赖。下列统计按各日志的 `test result` 汇总，不把先前专项与全量重复相加。

| 检查 | 最终结果 | 隔离工作树下的日志 |
| --- | --- | --- |
| core 全量，fault-injection | 468通过，0失败；2个由父测试启动的子进程入口ignored | `build/io-c1-fixed-core-full-final.log` |
| plugin_runtime 全量，all-features | 304通过，0失败；1个子进程入口ignored | `build/runtime-full-final.log` |
| core／runtime全目标严格Clippy | 通过，`-D warnings` | `build/io-c1-fixed-core-clippy.log`、`build/runtime-clippy.log` |
| core／runtime默认wasm32库编译 | 通过；core有7个未使用项警告，未将此列为无警告验收 | `build/core-wasm.log`、`build/runtime-wasm.log` |
| 冻结SDK完整性 | 36固定文件、13组原Wasm／包对通过，未重建或重封装基线 | `build/sdk-baseline.log` |
| 冻结SDK执行 | 9项基础＋3项依赖回归通过，包含在runtime全量内 | `build/runtime-full-final.log` |
| SDK工具／契约测试 | 14通过 | `build/sdk-python.log` |
| 工作台宿主＋当前实际Rust guest | 分批覆盖全部单元／集成组，116通过，最终0失败；同组以最后一次运行计 | `build/workbench-host-v17-final.log`、`build/workbench-host-remaining.log` |

core 包含最终 HTTP codec 23项、材料集成19项、材料崩溃4项、材料codec与真实SQLite页容量耗尽6项，以及全库迁移／既有材料／审计回归。快照试验实际关闭并删除合成原库，仅打开副本验证原容器与预留。runtime 包含执行15项（开启fault额外执行崩溃父用例）、最终jobs17项，以及旧包原样运行兼容。

首次存储测试有方法拼写编译错误；首轮core全量启动于HTTP修正落盘前，运行的是旧复现并失败。这些初始日志仍保留，最终全量重新编译并通过。宿主首轮缺少测试明确要求的 `MORROW_WORKBENCH_WASM`，17项前置条件失败；随后构建实际当前guest重跑，发现查询归档资格测试仍硬编码v15，而新快照为v17；仅更新该格式断言，再完整复验。不以首轮前置条件失败当作产品缺陷，不跳过功能用例。之后目录测试因隔离树缺少3份历史生成包及工作台bundle而失败；历史包从原工作树逐字节复制，工作台bundle使用本轮实际guest生成。仅重跑失败目录组和未执行的剩余组，不重复已通过的1,100次写入封存长测。

主要复验入口（在修正工作树执行，目标缓存可自行指定）。宿主目录测试还要求 `build/workbench-host/bundle/workbench.morrowplugin`、`build/test50-upgrade-{v1,v2}.mplugin` 和 `build/test49-projects/rust-ui/dist/*.mplugin`。本轮保留历史生成包原字节，未重封装，详细来源见 `build/catalog-fixture-provenance.json`：

| 原包 | SHA-256 |
| --- | --- |
| test.50升级v1 | `d74178050b3e51d3a9b76bc4c8b435e5cfa242e184fec5034923c488d4ad8b46` |
| test.50升级v2 | `5541fd9d2665dafe20f4e0889937e03d15a0b9d3b4274032e2eae0747e01fe51` |
| test.49独立Rust UI发布原包 | `eb22853dea47b5b8eb3f9897b9e8444e2de1fd3f0c2ee4ea0dd4799cae84d368` |



```powershell
cargo test --manifest-path core/Cargo.toml --features fault-injection --locked --offline --release
cargo test --manifest-path plugin_runtime/Cargo.toml --all-features --locked --offline --release
cargo clippy --manifest-path core/Cargo.toml --features fault-injection --all-targets --locked --offline --release -- -D warnings
cargo clippy --manifest-path plugin_runtime/Cargo.toml --all-features --all-targets --locked --offline --release -- -D warnings
cargo check --manifest-path core/Cargo.toml --target wasm32-unknown-unknown --lib --locked --offline --release
cargo check --manifest-path plugin_runtime/Cargo.toml --target wasm32-unknown-unknown --lib --locked --offline --release
python tool/verify_plugin_sdk_baseline.py
python -m unittest discover -s tool/tests -p 'test_sdk_*.py'
cargo build --manifest-path plugins/workbench/Cargo.toml --target wasm32-unknown-unknown --locked --offline --release
$env:MORROW_WORKBENCH_WASM = (Resolve-Path 'plugins/workbench/target/wasm32-unknown-unknown/release/morrow_workbench_plugin.wasm').Path
New-Item -ItemType Directory -Force build/workbench-host/bundle
cargo run --manifest-path workbench_host/Cargo.toml --all-features --bin package --locked --offline --release -- $env:MORROW_WORKBENCH_WASM build/workbench-host/bundle/workbench.morrowplugin
cargo test --manifest-path workbench_host/Cargo.toml --all-features --locked --offline --release
```

## 合入边界与下一步

1. 这是可评审的定向修正增量，不能将原候选 PR 整体标为通过；旧报告的产品完成声明没有继承。更新 [看板](../docs/DEVELOPMENT_BOARD.md) 与 [执行路线](../docs/ROADMAP_UPDATE_2026-09-15.md)，仅移动已验证的子集。
2. Store v17 迁移是实际格式变更，旧程序不可读取新格式。没有迁移用户资料；主应用升级、真实资料副本及旧版回退需单独验证。
3. IoWorker 仍是可信宿主 Router 接口，未直接持有 Manager/Pool 的 IoBinding。下一步须把 Router、Broker、批准和同一预算串接，再连接实际网络／文件后端；包声明和连接Ready本身不授予资源权限。
4. HTTP 仅有编解码与校验，尚未形成 guest→授权→第三方API 的端到端链；API节点发布、文件变更、凭据注入、真实效果核对、证据访问策略、配额退休和隔离重放仍待完成。同步可信回调已经开始时不能宣称可强制中断。
5. 默认Wasm编译不是浏览器存储、设备或全平台运行验收；未构建Flutter预览、Android或安装包。冻结旧SDK兼容通过不表示实验性IO扩展已经冻结。
