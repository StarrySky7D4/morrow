# 原生应用服务的持久出站选择与重放身份

日期：2026-09-21。实现基于 `66188a1`，应用版本仍为 `0.1.9-test.52+56`。本轮为本地源码与原生测试，不包含 Windows 应用重建、远端推送或发布。

## 实现与边界

`Workbench::start_service_with_outbound` 接受最多 8 个明确选择的持久端点引用及预期修订。重复、零值、超限、失效修订、缺失记录或未批准的能力均拒绝；原 `start_service` 和现有私有协议仍选择空集合，拒绝出站。UI、私有启动协议的端点选择与插件资源发现尚未接入。

端点和凭据在原 Store 授权锁内解析；在原 Manager/实例上重新签发 HttpRequest，以及确实需要时的 CredentialUse。端点的包摘要、能力、策略和原拥有者检查均在 DPAPI 解密之前完成。RouterFactory 持有已批准的 HttpRouteSet，监督器继续持有实际 Tokio runtime 至退出，不复制或重开拥有者。

新增持久引用批准路径：guest 的端点标识使用保存记录的引用，每次批准仍生成新的 live grant 和唯一 approval_sha256。标识可以保持稳定；旧路由、旧实例或撤销后的授权不能借此复活。已有单次 HTTP 入口的临时引用行为不变。

所选端点按引用排序，摘要覆盖完整 canonical Protobuf 记录及关联凭据记录，包括修订、有效期和密文，不含解密后的值。服务宿主剔除来请求中所有大小写形式及 Connection 指定的 `morrow-outbound-scope`，再插入自己的摘要，使幂等身份绑定本次选择。相同记录重启后可重放；选择、策略或凭据变化后旧键冲突，不能自动重发。此摘要不是实时授权，实际调用继续核验原资源租约与时效。

## 验证

| 范围 | 结果 | 关键证据 |
| --- | --- | --- |
| core outbound_authority | 19 通过 | 完整摘要在修订不变时也区分策略、密文和有效期变更；原锁、存储与 CAS 回归 |
| network managed_http | 27 通过 | 持久引用重连后旧授权拒绝、新授权实际外发；批准 epoch 改变；撤销不生成新意图；原临时引用保持 |
| network managed_content_service | 10 通过 | 混合大小写/重复/Connection 伪造头剔除；关闭重开原库后同策略可重放，改变/移除选择冲突；单 fuel 证明未重新执行 |
| network managed_service_owned | 9 通过 | 原拥有者回收；新增零摘要拒绝后仍取回原 worker 的断言另行通过 |
| workbench 原生服务 | 13 通过 | 实际 Rust guest 入站→出站→完整响应；重批准与查询不重发；端点修订冲突；无显式选择即使记录/Registry 允许也不外发 |

共 78 项相关回归通过，不累计前轮测试。network lib/上述 tests 严格 Clippy、workbench lib 严格 Clippy通过；修改文件格式与 diff 检查通过。工作台 lib test 仍有既有 `prepare_write` 未使用警告，core 的 Wasm 构建有既有仅原生使用项的 dead-code 警告；不称为全仓无警告。

重建了工作台 Rust Wasm 和新增 [实际服务测试 guest](../workbench_host/tests/fixtures/service-outbound/README.md)。新 guest 使用 core codec 检查真实 IO 响应，并由真实请求生成完整服务响应摘要；这不是三语言 IO SDK 稳定资格。最初简化 WAT 回包遗漏 requestSha256，实际测试将它判为 TaskProtocol/Unknown；已替换该测试实现，未削弱生产响应校验。

复现命令见测试 guest README；本机使用共享 `../integrate-track-a/build/io-codec` 目标目录。原生结果记录于忽略目录 `build/service-outbound-native-tests.log`。

## SubagentBridge 协作

使用更新后的官方 CLI，GLM-5.3-flash/max 提供批准辅助函数与 Rust 测试 guest 草稿，DeepSeek-flash/max提供设计/边界审阅。复用两会话至 revision 3；本轮会话调用的供应商 cache hit 分别为 1536 和 1024，仅作为调用事实，不代表必然节省相同比例费用。

累计历史超过本次输入预算时插件明确拒绝，未自动截断或调用供应商；改为更短输入或独立的有界任务。GLM 原草稿存在错误 API、借用和返回类型；初始 WAT 草稿未采用，后续 Rust 草稿经主代理修正实际 API、ABI、原始响应传递及边界后编译验证。DeepSeek 对大小写碰撞的疑问由已有完整 lowercase 比较和真实伪造头测试核对，未把无法复现的描述当成缺陷。执行成功不等于代码审核通过。

## 下一项

1. 私有启动协议和 Flutter 服务面板接入明确端点选择，绑定原会话、包摘要及预期修订，保留失败/Unknown/返回控制规则。
2. 定义插件可用资源引用交付或发现的最小契约；不可把保存引用当成授权，不修改冻结 SDK 字节以绕过准入。
3. 主应用服务运行中的端点/凭据撤销、完成/停止竞争和真实窗口验收；之后继续大帧分段及服务 TLS。

跨重启 Unknown 对账、完整文件系统、C/C++/Rust IO SDK 与全平台验收仍未完成。
