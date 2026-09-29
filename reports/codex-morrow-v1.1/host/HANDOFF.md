# M-00 / M-01 最小宿主切片交付

当前可消费候选为 **host-kit-003**，仅资格原型；M-00 已完成，M-01 的
NativeSession/Stream/Event/Tool 最小切片已实际实现。完整 M-01、稳定 SDK 冻结、
G0 总门槛与 84 项验收均未宣布通过。插件真实上游接缝探针属于 P-02，不能由
本 kit 的 fake 替代。

## 当前交接身份

- 实际工作目录：`C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor`
- 分支 / 基线：`codex/io-safety-refactor` / `88557916aabf2e10619b1022110035178498898a`
- 唯一 Schema：`contracts/experimental/agent_host_v1/agent_host.capnp`
- Schema raw SHA-256：`da0ac7a42e4b0f6aec0cfbdd2358cf86688b08e862626186bb9de4914f355a7f`
- Wire：major 1 / revision 1，experimental，未知版本/摘要拒绝
- 消费目录：`reports/codex-morrow-v1.1/host/host-kit-003/`
- Manifest：`host-kit-003/manifest.json`
- Manifest SHA-256：`5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01`
- Kit：180 个清单文件，含源合同、Rust 生成绑定、独立 crate/lock、fake、向量、许可和实际构建/解码回执
- M-00 输入：`m00-inputs.json`，178 个既有宿主/SDK/冻结原件/构建输入及 7 个计划/校验文件
- M-00 输入 SHA-256：`391d252a7a9bbc8ef7d76222ed5a043b4a267b61fe3ba7c4ca4088eefb43d5e7`

请只读消费 003，独立指定 target。`host-kit` 与 `host-kit-002` 是保留的历史
候选，不是当前签收对象；不从它们回退消费。导出不覆盖旧目录，没有删除旧产物。

## 实际修改

新增独立 `morrow-agent-host-contract` Rust 2024 crate，依赖精确锁定的
capnp 0.24.1、capnpc 0.24.0、sha2 0.10.9。该 crate 不链接 Store、宿主私有
管理协议、网络实现或执行器。默认构建没有 fake；显式 `qualification` 特性才
提供内存测试宿主。

`agent_host.capnp` 提供 NativeSession、Stream、AgentEvent/AppendBatch、Tool、
ResourceRef、Deadline/ClockDomain、ErrorEnvelope；Rust 公共值提供 U64、Revision、
OperationId、AttemptId、AccountEpoch 和十进制字符串边界。宿主不解释模型内容，
事件 payload 与 semanticKind 由插件拥有。

最小 fake 已执行的行为：

- NativeSession：可信测试夹具提供身份；伪造自报身份/产物/账号代次拒绝，未 Hello 拒绝。
  Drain 阻止新操作并撤销测试许可，保留核对；ObserveExit 返回结构化 Unavailable。
- Stream：固定请求长度/摘要，有限块上传，完整 Commit 后才可读；分块在 EOF 前可见。
  Read 带 expectedOffset/maxBytes；保留最近一块的重读，旧游标冲突，Inspect 返回游标。
  期限、Cancel、Close、Drain 阻止不允许的新交付；不新建付费请求，不解释模型终态。
- Event：宿主拥有 writerEpoch；有界批次和历史，tail CAS，全原帧字节去重。
  同键异字节拒绝，重试返回原 tail；旧 writer 拒绝，ReadAfter 最多返回 8 条。
  durableSequence 是明确标注的内存模拟值，并非磁盘承诺。
- Tool：固定提案无权限；可信夹具审批，伪造 permit 拒绝；只有首个 Claim 返回 execute=true。
  重复领取不执行，Report 不得在 Claim 前，Unknown 保留，核对不产生新操作。
- 消费边界：帧/遍历/嵌套限制，版本/摘要、方向、ID/epoch 相关性、reply family、
  qualification 标志、未知 enum/union、回复大小和 payload 摘要校验。

新增 `tool/agent_host_build.py` 的真实子命令只有 `preflight`、`export-sdk`、
`verify-kit`。缺必需参数、缺输入、输入摘要变化、旧输出存在、失败候选都不成为
成功产物；失败保留检查资料，不复用旧成功目录。生成目录由 Cargo 实际
build-script 回执定位，不扫描 target 选“最新文件”。成功 manifest 最后写入。

agent-content-v1、stream-view-v1、native-package-v1 + bundle-v1 在 crate README
中保留为三类草案，没有伪装成可协商能力，也没有空服务占位。Dart/C 生成绑定未实施。

## 本轮真实命令与结果

以下均从上述实际工作目录运行；没有运行 Flutter 构建。

| 命令 / 检查 | 实际结果 | 回执 |
|---|---|---|
| `rustc -Vv`、`cargo -V`、`capnp --version`、`python --version`、`rustup target list --installed` | 退出 0；Rust/Cargo 1.95.0、Capnp 1.4.0、Python 3.14.5 | `m00-inputs.json` |
| `python tool/verify_plugin_sdk_baseline.py` | 退出 0，36 pinned files / 13 original Wasm/package pairs；未重建/运行 guest | `m00-inputs.json` |
| `python tool/sync_plugin_sdk_contracts.py --check` | 退出 0 | `m00-inputs.json` |
| 两组旧原件逐字节摘要检查 | guest 36/36、transport 17/17；178 固定输入再次相符 | `final-preflight.json`；联合 M-00 回执 |
| `cargo generate-lockfile --offline --manifest-path contracts/experimental/agent_host_v1/Cargo.toml` | 新 crate 的依赖准备，退出 0；正式构建保持 locked/offline | `contracts/experimental/agent_host_v1/Cargo.lock` |
| `cargo test --locked --offline --manifest-path contracts/experimental/agent_host_v1/Cargo.toml --features qualification --target-dir build/agent-host-g0-target` | 修正后 14 passed / 0 failed / 0 ignored | 导出副本的同等完整重跑见下一项 |
| `python tool/agent_host_build.py export-sdk --baseline reports/codex-morrow-v1.1/host/m00-inputs.json --output reports/codex-morrow-v1.1/host/host-kit-003 --target-dir build/agent-host-g0-kit-target` | 退出 0；在导出副本上 14 测试、default 无 fake check、19 对向量与 38 次独立 capnp decode 全通过 | `host-kit-003/evidence/*.json`、对应 stdout/stderr |
| `python -m unittest discover -s tool/tests -p test_agent_host_build.py -v` | 6 passed，含模拟编译失败、缺输入、已有成功目录、部分 kit、路径逃逸 | `tool-tests.json` |
| `python tool/agent_host_build.py verify-kit --input reports/codex-morrow-v1.1/host/host-kit-003` | 退出 0，180 文件匹配 | `final-verification.json` |
| 对 003 重复 export；对不存在 baseline export | 均退出 1；003 摘要未变，缺输入没有创建输出目录 | `final-verification.json` |
| `git diff --exit-code -- core plugin_runtime network_node workbench_host sdk lib packages` | 退出 0，既有生产源码未改 | `final-verification.json` |

向量入口为 `host-kit-003/examples/vectors.rs`，清单为 `host-kit-003/vectors/manifest.json`。
包含 19 个真实 request/reply 字节对和未知版本拒绝向量，共 40 个向量文件。
请求和回复均经 Cap’n Proto CLI 独立解码；消费者编译的是相同宿主权威生成类型，
没有复制一份自定义 AgentEvent。早期 002 的 40 个向量文件也曾成功逐字节重产，
见 `reproduction-and-failures.json`；这项历史成功不取消下面的 002 边界缺陷。

## 已发现并修正的缺陷

独立复核促成四项修正：回复验证与 family 对应、完整 ResourceRef 测试绑定、
流最近一块重读/Inspect 游标，以及最大事件批次的消费预算。

002 的最后一项已真实复现：加入
`maximum_event_batch_remains_readable_by_consumer_after_validation` 后，8×4096 字节
批次测试因 `ReadLimitExceeded` 退出 1。原因是验证消耗了返回给消费者的同一
Reader 的累计预算。003 在验证后重新提供独立且仍有界的 Reader，该新增回归
与其余 13 项全部通过。没有改变 Schema 或旧合同字节。两个早期候选不再用于签收。

## 消费说明与剩余边界

消费 crate 路径即 `host-kit-003`。公共入口为 `agent_host_capnp` 生成类型及
`frame/encode/decode/exchange_checked`、`Transport`。fake 入口为
`fake::{FakeHost, FixtureIdentity, hello}`；四类构造示例在
`host-kit-003/tests/common/mod.rs`。各消费者使用独立 target；不要往 kit 写代码
或在其中创建 target。没有新的生产宿主启动命令。

```powershell
cargo test --locked --offline --manifest-path C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor\reports\codex-morrow-v1.1\host\host-kit-003\Cargo.toml --features qualification --target-dir <消费者自己的独立target>
```

仍未实现：M-02 真实原生准入/进程退出；M-03 实际增量网络、独立控制通道和
多层预算；M-04 磁盘事件/检查点/gap/outbox；M-05 内容外发权限；M-06 真实 B
派发/许可撤销；M-07 UI；M-08 凭据/登录；M-09 原生包/套件安装；完整多语言
SDK 和全平台资格。旧 Wasm 运行回归、本机完整宿主构建、真实登录/付费请求均
未执行。本轮没有读取个人账号、MCP 或密钥，没有调用 SubagentBridge，没有
提交、推送、标签、发布、关机，也没有删除现有产物或修改插件仓库。

本宿主写入范围为新增 contract crate、两份工具/测试文件，以及 `reports/.../host/`；
联合验收目录由验收会话独立维护。最终联合签收以其最新 003 回执为准。
