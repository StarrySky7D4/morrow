# W12 有界 SSE 传输与原始 HTTP 回归

2026-10-03，Windows x64 本地离线 Release 验证。判定为 **PASS_SCOPED_SSE_TRANSPORT**；整个 SDK、G02 公共插件网络源及全平台资格仍 OPEN。该阶段位于已合入 Linux 17 路径、保留 Windows 增量的同一开发工作树，HEAD 保持 `63f38d4a8a5bf453248dc7532197bee6980dc86f`。未提交、推送、CI 或发布。

## 实现范围

在独立 `network_node_stream_001` 原型中新增纯 `sse/decoder.rs`、`SseLease` 和测试。原 `StreamLease::check_delivery` 仅改为 crate 内可复用，注册一个新模块；没有新依赖或锁修改。普通 `network_node` 的旧 SubmitHttp 整帧路由、SDK327、57 个固定原件、schema/pins 和 Linux 汇合源码均保持原身份。本阶段未重编或封装原 guest。

Decoder 逐块处理首个 BOM、严格 UTF-8、CR/LF/CRLF、多行 data、默认/自定义 event、持久 id、空 id 和 retry metadata。每次至多产生一个事件，调用者保留未消费的 chunk 尾部；没有内部事件队列。EOF 不派发无 blank delimiter 的末尾块，Finish 明确记录 truncated；非法 UTF-8（包括注释、未知字段和 EOF 尾部）返回固定编码错误，区别于浏览器替换非法编码的策略。retry/id 不建立授权、checkpoint、自动续期或重放，[DONE] 是普通数据。

默认配置：line 8 KiB、block 64 KiB、total 4 MiB、最多4096个事件、id 1024字节、retry 20位且必须可表示为u64。line不计初始 BOM/换行；block计入 BOM/分隔符，但 CR 后的 LF 仅计入 total，因此 **block 不是全部 wire 字节的逐事件上限**。total 包含所有消费过的 BOM/CRLF/注释/空块。原 transport 的累计响应限制独立保留。配置由可信调用者提供，不是 guest 预算或网络许可。

SseLease 复用原 HTTP 请求的固定绝对期限、取消和 guard。解析缓存中的第二个事件也在交付前重新检查原权限；取消 next_event future 后保留原 pending demand、部分行及剩余数据。只接受 status200、单一 text/event-stream（可带一个 UTF-8 charset）、无编码或单一identity编码；非法响应头先取消再等待真实 worker join。finish 不排空、派发缓存或解释提供者终态。

当前保留至多一个不超过8 KiB的 transport 交付 chunk，另有有界 decoder 状态。这不保证 Bytes 的底层共享分配、reqwest/hyper/kernel缓冲或持有事件的业务调用者只有8 KiB。原 transport 的 Completion.peak_upstream_chunk 与响应总额度仍单独计量；本次不宣称全系统内存或 OS 背压资格。

## 修复和真实测试

审查发现并修复两个新收尾问题：finish首次发现半个 UTF-8 字符时，原解析错误不能被清理取消覆盖；HTTP 已 EOF 但 decoder 出错时不能报告 truncated=false。原三个 code-ready 快照及两份新增真实 localhost 回归保持独立身份。两修复先于统一执行，未编造“修复前测试失败”结果。

实际唯一命令：

```text
cargo test --offline --locked --manifest-path network_node_stream_001/Cargo.toml --target-dir <external stream-only target> --release --lib --tests -- --nocapture --test-threads=1
```

使用既有 Rust1.95.0/MSVC、批准的离线缓存和原 Cargo.lock。新的专用 target/TEMP 在源码树外；环境采用既有公开构建字段白名单，不继承 credential/MORROW/wrapper 字段。没有下载、安装或系统配置修改。编排由提权 driver 执行；本次实际测试 child token未单独采样，不继承 W10 受限用户资格。

| 实际 suite | passed | 范围 |
| --- | ---: | --- |
| lib | 26 | 21新增decoder＋5原delivery测试 |
| client integration | 10 | 原精确origin/method、headers、响应状态、limit、取消和不重试 |
| SSE integration | 13 | 新真实localhost POST、提前交付、慢消费、分块、EOF、缓存撤权/取消/到期、错误/limit、真实join |
| stream integration | 6 | 原按需transport、固定期限、撤权、取消安全读取和回收 |

真实 Cargo exit0，55 passed，failed/ignored/measured/filtered均0；不是55个新增SDK能力。34方法为新增SSE，21方法为继承传输回归。12项本crate源码/锁输入和3个Rust工具前后保持，新TEMP零文件残留。每个suite、原argv、起止时间、完整stdout/stderr、实际exit和哈希由外置 `w12-sse-transport/execution-001` 保存。只访问普通合成localhost数据；不打开 Core/Store/真实DB/protected Session/DPAPI/账户密钥。

## 仍未运行和下一步

本阶段没有公共 guest 网络源或实际 channel bridge、原三语言新网络插件、生产 UI/owner、TLS运行、外部账号、Linux/macOS/mobile/Web或完整SDK资格。HTTP EOF、decoder完整性和transport worker_joined各自记录；任何一项均不证明远端业务成功、回滚或全部底层网络task退出。

下一步 G02：独立网络源 profile 与 host-issued endpoint/request/credential/instance-generation 绑定、单次持久 dispatch claim、撤权联动、事件 envelope/frame 额度、SSE id 与可信cursor分离。在原 channel producer 的可信线程上桥接 async lease，ACK背压时仍维持原期限，实际join HTTP worker后才允许正常 producer finish。旧 SubmitHttp 语义保持；WS生命周期另行实现。

G07也已完成静态前置核验，但未开放 QueryOperation：现有恢复查询不足以替代重新批准的单operation只读grant。需绑定当前实例/slot、package digest、Store和原operation，在入口/交付重查权限；Unknown只读不重发，Observed的reconciliation digest不能伪造完整HTTP响应。它与本阶段SSE测试分别推进。

原 W07 范围表按其只读日期归档，不以它覆盖新的低层SSE实现；G02公共插件能力和其余完整目标继续OPEN。
