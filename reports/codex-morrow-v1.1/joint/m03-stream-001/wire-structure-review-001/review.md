# v3 wire-kit-001 联合结构审查

2026-09-29，仅只读schema、codec、rules、测试/向量构造与README；本批没有调用候选codec、generator、compiler或native程序，没有新增HTTP运行。固定输入摘要见identities.json。

结论：当前结构足以表达已对齐的M03首片语义，未发现需要阻断唯一合同消费的字段缺失。此结论不证明运行时状态机、权限、持久claim、OS pipe或Core成功链；生产方4项codec测试、24合法独立向量及2非法向量仍是生产方执行结果。

## 对齐点

- schema major3/revision1 SHA256 `8da8f1455200696d8884949f415aae4edd70cee6b583889bba7906c7730cc864`，生成Rust `071955f822b2eab7a803ad2f8d6da50dde4d7371e1c1ef79009b272a4eb842c3`；旧协议不原位改动。
- 长度前缀、单个unpacked消息、最大32768字节payload、traversal/nesting界限、version/reserved/unknown enum及kind-payload组合在codec明确验证。HTTP grammar/allowlist、方向、连续序号、批准及状态迁移有意留给runtime；不能把结构验证的能力扩大。
- request_digest的域分隔、LE整数、长度前缀、原operation/attempt、有序raw header、requested responseLimit与body组成前像，符合README。请求limit被收紧后，request hash仍指原请求，完整Approved Decision另绑定批准limit，Commit要求精确匹配；必须保持这两个摘要对象不同。
- Core Header.value已有Vec<u8>。新native可直连RawHttpRequest，避免旧managed/String转换；仍须Core HTTP grammar与native无credential白名单，并保留原字节。
- 信用额是absolute snapshot，四种互斥消费分类和为prefix，重复snapshot不增加credit；peerConsumed可领先host观察到的OS完成，但不能领先issued。pending write占同一reservation，禁止重复加额度。
- 精确响应上限后允许受原期限约束的terminal poll，以真实EOF或额外字节Limit分类。不能因为长度刚好到上限就假EOF。控制HttpTerminal早于数据的情况已有说明：成功EOF须本地连续prefix达到终态receivedOffset；失败/取消应报相应错误并清理，不能等待无法再到达的数据而挂死。
- Progress各维正交，Observed需要HTTP EOF及完整材料；partial/取消不得覆盖已Observed。RequestClosed只证明request worker和pipe清理，允许guest随后Close并退出；最终owner release由operator观察，避免等自身退出的循环依赖。
- 所审测试覆盖全24种结构roundtrip、截断/尾随、部分边界、hash变化和正交终态；向量是独立样本而非完整有效会话。例如结构合法的CancelAccepted样本仍保留generation1，绝不能把它直接当实际已撤权交互模板。

## runtime消费时必须具体落实的三点

1. **取消后代次与清理白名单。** README要求guest Query/Close/Credit/Cancel继续echo原Challenge generation1，又要求host应用取消后generation2且stale tuple不授予活动。建议明确执行为：原tuple仅允许历史Query、幂等Cancel、零window的已issued尾部ACK及Close；拒绝Commit、正window/新读写，不让代次检查误拒清理而死锁。host发出的新generation禁止被guest当续权；不可简单统一成“generation必须永远等于initial”或“任何旧generation全拒”。如双方解释不同，以新增合同说明对齐，勿改已封存输入。
2. **辅助API不是批准入口。** same_admission明确不比较generation/remaining/budget/code/方向/序号；request_digest也没有复用Frame::validate的全部method/target非空及ASCII检查。正常消费先Frame decode+session状态+HTTP policy再hash/批准，不能从“hash可生成”推断输入有效。Progress仅做部分事实蕴含检查，State/RequestClosed的真实事实仍由host构造与跨消息检查负责。
3. **限制及消费记录。** 两个lane分别连续序号，host唯一writer保持控制顺序；128次guest控制预算包含小块Read/ACK和收尾，应选择足够块长并为终态清理预留，不以超额命令续期。zero-window收尾ACK不得启动网络poll；唯一例外的精确limit terminal poll仅检查原响应EOF，不能发新HTTP或交付超限字节。parserYielded/各discard由一个RequestTask lease分账，禁止取消后晚到data生成业务事件。

无需因本结构审查重复002或整个旧codec套件。下一步应在固定host/plugin候选里验证上述跨通道/代次/EOF行为，并用真实Core首delta屏障、实际OS pending及完成回收取证。这里只读审查不提升M03运行信用，原产品0/2、84not_run及既有门槛不变。
