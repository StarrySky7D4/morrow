# v3 codec kit001 独立编译与规则复核

2026-09-29。独立离线编译成功，联合新增7组定向codec测试全部通过，没有发现本轮结构/摘要测试中的实际失败。只对codec增加限定信用，未执行native host、HTTP、pipe、真实Core/SSE或002/transport复测。

## 可追溯输入和构建

消费固定host wire-handoff-001 `3cb2e70480638d2094b82b2cff0a62739ebee2546fa1236650e81daa99e6286b`、manifest `9d43385067d09a016ef950edf5fe4d489803e02f7bee8c419014d6b6fc2af5ae`，82个原source/kit输入在前后匹配。所有写入位于joint/m03-stream-001新目录，封存候选未改。

将kit source逐字复制到本目录，仅额外加入联合tests/joint.rs。独立Cargo home、target、profile、tmp；清除非必要环境，offline+locked，无网络下载。13个registry归档从只读本地cache按原lock checksum验证后解包，全部路径验证在新vendor内；没有复制个人Cargo配置/凭据。构建实际产生12个唯一package ID的compiler-artifact记录（含本地codec），与13个归档数量含义不同；记录原始JSON输出及编译产物关联。

使用本机1.95.0工具链及固定Capnp1.4.0生成器，工具SHA已记录。重新生成native_http_capnp.rs的SHA256为 `071955f822b2eab7a803ad2f8d6da50dde4d7371e1c1ef79009b272a4eb842c3`，与封存值完全一致；schema SHA256 `8da8f1455200696d8884949f415aae4edd70cee6b583889bba7906c7730cc864`。独立编译的joint测试exe执行exit0，无失败重试。

## 七组已执行结果

| 组 | 真实验证及范围 |
| --- | --- |
| 封存向量独立消费 | 24个合法frame独立解码、kind对文件名、schema绑定、encode回原字节均匹配；2个非法向量拒绝。向量是独立样本，不是已运行会话。 |
| 最大字段/headers/chunk | 32头且总成本恰好8192、operation256、method16、target2048、body声明32768、response限额65536能编码/解码；Prepare帧11468、Head9476、请求/响应最大8KiB块帧8764字节。33头、8193总成本、method/target/body/response+1、chunk+1、末端offset+1及u64溢出拒绝。最大target等是codec结构边界，不是合法HTTP请求证明。 |
| 最大payload及未知判别 | 将单segment扩为带未引用零words的合法Capnp消息，payload恰好32768、总帧32772仍解出原Frame；再+8拒绝。该最大物理帧不是普通encoder的自然紧凑输出。原始字节突变未知major/revision、Kind、union、reserved和Progress三种enum全部拒绝。 |
| 独立hash前像 | Python独立按域分隔、LE及长度前缀构造两种响应限额65536/1024的前像；Rust request_digest两者分别匹配。保留前像bin与oracle。头顺序、epoch变化改变hash，body摘要错误拒绝；收紧Approved limit仍结构可表达，完整Decision关联留给runtime。 |
| 四类credit和offset | 四类合计65536合法；和不匹配、加法溢出、window/maxChunk越界和零chunk拒绝。peerConsumed=6而osCompleted=0合法，符合读者先ACK、host后观察OS完成；其余issued/reserved/received约束越界拒绝。 |
| 正交终态与活guest清理 | 完整Observed+EOF+材料仍可带撤权及后续IPC错误；RequestClosed可在childExited/stdoutEOF/stderrEOF/ownerReleased全false时成立。缺worker join、任一pipe回收或完整Observed证据拒绝；取消Unknown的request关闭也可表达。 |
| 结构与权限边界 | 实际确认same_admission忽略sequence/generation/remaining/budget/code；相应结构frame能解码，codec无lane校验。测试通过表示边界如文档所述，不表示这些消息可被driver接受。 |

所需主要行为保持可表达：原请求requested响应限额进入hash，批准收紧限额另外绑定；raw头不走String；四类消费绝对prefix；完整Observed不因后续IPC/模型错误降级；RequestClosed不依赖guest先退出。

## 剩余运行时责任

结构预审记录见../wire-structure-review-001/review.md。尤其：guest继续echo原generation与取消后旧tuple清理白名单需要双方runtime保持一致，不能误拒零window尾部ACK/Close造成清理死锁，也不能容许旧Commit或正window恢复活动。request_digest可为部分Frame结构不合法输入生成摘要，摘要本身不是HTTP grammar/权限判断。

源码明确把方向、连续序号、状态迁移、live批准、revocation fence、实际pipe peer PID和OS完成事实留给driver；本次没有替代验证。最大quota处无positive body credit的terminal EOF poll、HttpTerminal先于data时防提前EOF、部分写取消后回收及RequestTask ParserDetached接管仍须固定host/plugin候选真实执行。

未系统强制Capnp traversal/nesting阈值、所有内部指针恶意布局或全组合fuzz；本轮只覆盖授权的定向边界，旧通用codec套件不重复。完整M03矩阵仍not_run，M02冻结结果及G0/P02/J00 blocked、G1未通过、产品图0/2、原84not_run均不改变。
