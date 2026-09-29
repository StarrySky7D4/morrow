# J-00 第二轮：正式宿主 kit 与插件冻结交接

截至 2026-09-28 19:35（Asia/Shanghai），**宿主 003 最小资格 kit 和插件独立交付切片已完成联合复核；G0/J-00 仍阻塞，84 个产品场景均未执行，SDK 未冻结。**

第一轮索引仍保留 31 个工作包、G0–G6 七门槛、48+36=84 原编号验收与 W01–W28 映射；本轮只更新实际收到的证据和状态，不把局部通过扩成工作包结项。

## 宿主正式候选 003

- 清单：`../host/host-kit-003/manifest.json`。
- SHA256：`5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01`。
- 唯一 Schema：宿主 `contracts/experimental/agent_host_v1/agent_host.capnp`；raw SHA256 `da0ac7a42e4b0f6aec0cfbdd2358cf86688b08e862626186bb9de4914f355a7f`，major 1 / revision 1。
- 独立回执：`runs/host-kit-003-review/result.json`、`commands.json`，退出 0；执行检查器原件也保存在该目录。

实际重新执行：180 文件/12 源项摘要和大小核对、宿主只读 verify-kit、19 对向量共 38 次独立 Cap'n Proto 解码和明确语义断言、独立 target 的 locked/offline 消费编译、14 测试/0 ignored、default 无 fake check、向量重新生成。40 个向量文件逐字节一致，消费生成的 Rust 绑定也匹配 kit，运行前后 kit 与权威源文件均未改变。

独立语义核对包括：请求/回执关联，qualificationOnly，握手前拒绝，固定请求后读取，EOF 与模型终态分离，精确 u64，append 原回执/同键冲突，伪造许可拒绝/一次有效领取，Unknown 保留，Drain 后拒绝和 ObserveExit 的 Unavailable。

002 的 `8×4096` 最大合法事件批次问题由宿主复现为 `ReadLimitExceeded`；003 将验证与消费分配给各自有界 Reader。联合重跑明确包含 `maximum_event_batch_remains_readable_by_consumer_after_validation`，全量读取 payload、摘要和关联字段通过。未提高旧 Wasm 限额，未在消费者中绕过校验。旧 002 检查与源变动退出 1 原样保留于 `runs/host-kit-002-review/`，不作为当前候选。

以上只接受 NativeSession/Stream/Event/Tool 四类实验 Rust/fake 切片。其余三组合同、Dart/C 绑定、真实网络、磁盘 durability、原生身份/隔离、生产许可、安装与 UI 后端不因此获得资格。完整 M-01 仍未结项。

## 插件唯一 ready 交接

只消费明确 ready 的 `C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex\receipts\handoff.json`，SHA256 `995cfa721bae3b94a494a0ee88e9e76f9e29afeec71ee6481c9ac756446d3ca4`。对应联合回执 `runs/plugin-ready-review/result.json` 退出 0。

17 个冻结输入与提供方 before/after/实际文件摘要完全一致；最终 43 项入口测试及 12 次 CLI 调用的回执、退出码和失败语义一致。此处是对生产方执行回执的独立只读复核，没有从联合会话执行会写入插件仓库的工具。旧 12 项以及 191737/191929 等中间回执不再绑定当前实现；两次发现过期摘要的联合失败证据继续保留。

209 个 Codex 文件与 13 个 CC Switch 文件，共 222 文件，已逐项核对准确集合、SHA256、Git blob SHA-1 和 LICENSE/NOTICE；来源锁固定原计划提交。源快照清单可解析且与实际目录一一对应。以前截断的 `upstream/codex-git-tree-complete.json` 作为取源失败原件保留，正式审查明确排除它；无需删除失败证据来取得静态交接资格。

`docs/upstream-audit.md` 和两份闭包 JSON 已交付，包含真实执行/网络/存储注入点与旁路、normal/build/dev/target/feature 等静态信息。上游锁分别含 1471/750 个 package 记录，其数量由联合读取复核；这些不是插件两图的已解析最终锁。

完整资格仍被正确阻断：两份来源均为 `partial_snapshot`；sources/full 返回非零 `source_incomplete`。native/Wasm 产品构建图仍为 0/2，manifest/lock 均为空；后续未实现阶段非零失败，未复用旧 dist。入口回执没有把这些预期失败升级为 P-00 或 G0 成功。

另外已只读看到 `receipts/p02-readiness-001/result.json`：曾针对真实上游 exec-server 做 Cargo metadata 准备尝试并退出 101；该历史回执绑定已撤回 002 与当时来源锁，不是当前 003 上的真实接缝执行。不能说从未尝试准备，也不能说 P-02 已执行通过。

## 当前门槛和下一步

| 对象 | 状态 | 范围 |
|---|---|---|
| M-00 | 已验证 | 第一轮基线台账、178 输入和旧 SDK 原件 |
| M-01 最小 kit | 已验证限定切片 | 正式 003 的四类实验 Rust/fake；完整 M-01 未齐 |
| P-00/P-01 交付 | 已验证限定切片 | 入口失败路径回执、222 固定文件和静态审查；完整退出门槛阻塞 |
| J-00/G0 | 阻塞 | 缺真实上游 P-02 双端执行/网络/存储替换及断开拒绝证据 |
| 84 产品场景 | 全部未执行 | 0 项通过，不用脚本测试数量抵扣 |

继续须取得可构建的固定上游最小真实源码闭包与依赖，建立实际目标/feature 构建图，再对真实调用链完成三个接缝及无 fallback 负面探针。联合会话仅复核交接和证据，不开发第二份 Schema，也不修改对方实现。

运行平台是 Windows 本机。没有真实账号/付费模型、个人 MCP/密钥访问、设备掉电、其他平台、真实沙箱或最终安装资格；没有提交、推送、发布、清理旧产物或关机。
