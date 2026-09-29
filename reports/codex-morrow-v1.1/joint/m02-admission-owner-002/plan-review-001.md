# 方案早期审查 · 2026-09-29

本记录基于宿主方案消息与既有 Core 源码只读检查；新实现尚未 ready，没有本批运行结论。准备基线 2,033 个输入身份一致，预检实际 exit 2。原14项矩阵保持冻结，本记录追加方案约束。

现有 `core/src/service_authority.rs` 的持久 Record 只覆盖 Authentication / Publication，不是通用原生执行批准。复用其存储/权威机制时必须保持类型区别；新增 native ledger 应明确独立版本、Protobuf+LZ4 编码、所属数据库/表/文件、原子读写点，不可把已有网络批准当作启动许可。

现有 `core/src/store/service_authority_lock.rs` 根据持久 store_id 选择本地锁，使用实际 File::try_lock。它支持同用户、可信 profile 下的合作进程协调；同用户任意替换 profile/锁目录、恶意复制修改数据库或强隔离不在该机制的证明范围。需要核查本片 init 并发、identity 的持久创建及再打开、别名和 profile 绑定，尤其不能通过重新 init 清掉 Unknown。

宿主方案：新增 native_session_owner_002，旧 native_session/kit001 不变；capnp001 guest wire 与 ordinary client001 复用。init 只允许空 profile 绑定一个 slot；serve 默认 proposal。受控 operator harness 才可 approve；获得同一 issuer 内不可反序列化的 live approval，首次 Admission::authorize 启动单调时限；claim 不重新构造批准。不把 stdin 管理入口视为完整产品 CLI 身份认证。

关键副作用顺序须在源码和事件中对应：取得 Core owner pin；同一 IMMEDIATE 事务消费批准并写 LaunchPending；commit 成功后 spawn；登记实际 child；退出与双 EOF 后持久 Released，再释放 pin。commit不确定、spawn成功登记失败、host死亡等窗口必须保留 Unknown/非Released owner，不能自动按TTL/PID缺失/锁自然释放接替。

待宿主明确并审查：

1. approve、claim、revoke、Release 的线性化点；Active批准撤销如何传播到真实supervisor；是否支持第二host撤销另一issuer批准，如不支持则明确限制。
2. plugin/role/operation、slot、grant代次、artifact/config/schema/能力是否真正成为固定批准与claim匹配条件，而非仅日志描述。
3. init并发、profile/库identity稳定及路径别名；重新初始化不得规避未知owner。
4. 查询只观察不能恢复live authority；外来或历史grant_id无法重新构造许可。
5. 不提供clear/steal/TTL回收的本片只证明fail-closed；crash后人工测试清理不升级为产品自动恢复。

新KnownProcess观察器仅OpenProcess已知PID并持有句柄，读取image/creation/exit，不包含全机枚举或终止API。PID来源须逐条标记，父子关系来自可信launch/事件关联，不宣称独立OS PPID核验。只有获得句柄后才进行host crash注入；缺句柄不得冒充独立退出确认。

历史peer将使用同一冻结exe的capture/replay-hello/replay-query，保存前会话真实原字节并与host记录比对。150ms观察窗口计入既有deadline，不续期。ClosingUnconfirmed需要holder显式输出本批材料目录内的已知PID，再核对该PID的image与creation，不能靠全机搜索找后代。

本阶段不给M-02/G0/G1/原84项增加产品信用；收到封存source/lock/build/exe/接口后，再选择有意义的实际新场景。
