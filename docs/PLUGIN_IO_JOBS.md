# IO-B2：有界 IO 作业与契约路由

状态：运行时作业层已有本轮修正后的限定实现，见 [整合修正报告](../reports/io-safety-refactor-2026-09-19.md)。它把**完整任务输入**、**每作业契约路由**和**有界队列／调用／字节／期限**组合成一个可 submit／poll／read／cancel 的作业，并保证停止后不把迟到结果当作成功。真实网络或文件后端由 IO-D 提供；本层自身不执行任何 I/O。

## 结构

`plugin_runtime::io_jobs`：

- `IoWorker::spawn(package, host, connection, clock, capacity, limits)`：一个执行器独占一个包、连接与核心，运行在名为 `morrow-io-job-<id>` 的独立线程上，因此内容提交不会发生在调用方事务内。
- `submit(input, router, timeout) -> JobHandle`：`input` 是**完整任务输入帧**（1..=128 KiB，例如 IO 请求帧），建立作业时立即计费，不做截断、不重放。
- `router: Box<dyn Router + Send>`：可信宿主为每个作业提供的契约路由；`route(call, request)` 按 guest 顺序收到每次调用，执行器从不重试。
- `JobHandle`：`poll()` 非阻塞观察、`read(max_bytes)` 恰好一次读取终态、`cancel()` 请求取消。
- `drain(timeout)` 停止接收并收紧所有期限，仍有效的 Ready 等待 read/drop，或等到期限才收尾；`stop()` 先撤权再取消；`try_finish()` 只在执行线程结束后归还核心所有权。

## 界限

| 资源 | 上限 | 行为 |
| --- | --- | --- |
| 队列＋运行中＋未消费 Ready | min(64, manifest.max_jobs) | 超出返回 `Busy`，不阻塞发送 |
| 每次作业调用数 | min(1024, runtime.host_calls) | 超出后该调用被拒，作业以 `Limits` 结束 |
| 每作业字节 | min(16 MiB, manifest.max_job_bytes) | 输入＋每次请求＋每次响应累计；超出即拒绝 |
| 执行器总字节 | min(64 MiB, manifest.max_bytes) | 累计只增不退，释放作业不返还 |
| 期限 | ≤ manifest.max_duration_ms（当前声明上限 30 秒） | 排队中已过期不启动；运行中超过期限的结果不作成功 |

## 顺序与外部效果

每次路由调用的顺序固定：

1. 检查真实连接撤权、取消、调用数与期限；失效立即失败。
2. **先计费请求字节**，预算不足则在路由之前拒绝，不产生外部效果。
3. 调用契约路由；`Denied`／`Limit` 映射为本地失败，`Unknown` 明确标记为“效果可能已发生，需核对”。
4. 计费响应字节；响应为空、超帧或不满足预算时拒绝，并把作业标记为 `unknown`，因为调用已经到达路由。
5. guest 完成帧必须等于最后一次路由响应；否则执行以 `TaskProtocol` 结束。

`JobReport` 因此区分三种事实：`execution.outcome` 是运行结论，`cancelled` 表示撤权、取消或期限阻止交付，`unknown` 表示外部效果未知、只能核对。

## 停止与迟到结果

- `stop()` 先撤销实例授权，再取消全部作业；已通过最终授权的调用仍可能提交，但结果不再作为成功交付。
- 作业在调用前后、发布 Ready 和最终 `read` 都检查真实连接撤权与取消期限；Ready 保留在统一受控槽位中。`read` 与本地 stop/cancel 共用锁：期限或撤权后到达的结果被替换为 `Cancelled`／`Deadline`，响应被丢弃（`response = None`）。
- 执行器结束时已有报告会被抑制为失败；没有报告才返回 `Unavailable`，不把缺失当作成功。
- `read` 的字节上限在消费前检查：超出返回 `ReadBound` 并保留结果，调用方可显式增大上限重试，结果与槽位继续计费。
- 丢弃运行中句柄只请求取消，同步路由回调真正返回前不释放容量；取消后留下的无载荷终态由 read/drop 消费，避免无限待交付元数据。
- `JobLimits` 字段可由调用方直接构造，因此 spawn 重新验证硬上限和包声明预算。此预算检查不等于资源授权；授权仍由可信 Router/broker 执行。

## 与其它层的关系

- [IO-C2](PLUGIN_IO_EXECUTION.md) 的唯一活跃执行由路由内部使用：每次 `route` 可以驱动一个 operationId 的一次与多次调用，作业层只负责调度、计费与交付边界。
- 本层没有持有 Manager 的 `IoBinding`，声明预算与连接 Ready 都不是资源批准；宿主必须把 Router 接到已经授权的 broker。spawn 的 clock 参数保留在实验性接口中，本层期限实际使用单调时钟，不能用该参数宣称完成批准有效期检查。
- IO 协议帧使用 [IO-A／IO-B1](PLUGIN_IO_DESIGN.md) 的单帧契约；本层不改变帧格式、不新增 core schema。
- 数据库格式与记录不变；作业本身不持久化，重启后由持久历史与核对决定后续。

## 仍未覆盖

真实第三方 HTTP／HTTPS 与文件系统后端、guest 发布授权路由、远端身份与凭据、异步 guest 挂起与多次作业并发、执行器池化、真实远端效果核对与录制回放、按配额退休。本层不表示任何插件已经获得资源授权或能完成真实调用。
