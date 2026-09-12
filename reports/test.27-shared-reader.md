# test.27：Windows 跨进程只读交付与读者生命周期

日期：2026-09-12。应用 `0.1.9-test.27+32`，审计／工作台宿主 test.27；核心仍为 test.22（内容格式6），执行后端与 SDK 仍为 test.11，工作台 guest 仍为 test.13，随包清单 test.27。运行期 v7、已有内容格式及保护身份格式不变。第一方 AGPL-3.0-only。本轮仅本地开发，无推送、标签、Release 或上传。

## 实际完成

- 固定 Cap’n Proto 引导契约 v1：Offer／Reply 显式区分，版本和 schema 摘要固定；嵌套共享描述独立校验。帧最多128 KiB，回执字节最多64 KiB，校验实际长度和 SHA-256，拒绝截断、尾字节、身份为空、类型错用与未对齐输入错误。
- Windows 使用真实 Child 进程对象交付只读、不可继承的目标句柄。没有按 PID 重新打开进程，没有 SAME_ACCESS、CLOSE_SOURCE 或按远端数字关闭。实际映射保持私有，guest／SDK 不接收指针或句柄。
- 接收辅助程序对原句柄探测写入权限，只有拒绝写入且错误为 ACCESS_DENIED 时继续。使用私有只读副本与系统复制接口读取到 Vec，原句柄不被消费，不对潜在外部可变映射建立 Rust 共享借用。回执再次核验内容摘要。
- SharedObjects::start_reader 在启动前和复制句柄前检查真实宿主、消费者和活跃租约。RemoteReader 在任何可能交付句柄的操作之前建立 Child＋Mapping 所有权，回传时精确匹配 transfer 与 descriptor，交付前再次核对授权，成功结果只消费一次。
- 回传数据、管道关闭、协议失败、关闭或终止请求均不释放映射引用；只有系统确认实际 Child 已退出后才释放。Drop 请求终止后仍等待退出，观察／回收失败时保守保留所有权，可能一直保留到宿主退出。既有对象退休后继续占用额度，不据猜测复用页面。
- 读写管道独立：提前 close_input 不需要等到 Reply 才能关闭输入；如果写入本身阻塞，关闭不承诺立即打断该写入，调用者可请求 stop 并继续观察实际退出。

限定原生 FFI 均留在用户已明确批准的 `plugin_runtime/src/shared_memory.rs`。执行模块其余手写代码继续拒绝 unsafe。核心预编译生成器沿用既有生成代码许可边界，不借此增加其他手写原生模块。

接口见 [父子进程契约](../docs/SHARED_TRANSFER.md)、[共享对象](../docs/SHARED_OBJECTS.md) 与 [原生映射审查](../plugin_runtime/SHARED_MEMORY.md)。三个子代理分别完成契约、原生交付、独立进程验证；主代理完成进程所有权、租约接线、集成和实际应用验收。

## 验证结果

| 范围 | 本轮结果 |
| --- | --- |
| 新增消息协议 | Windows 9项通过，核心全目标严格 Clippy 通过；wasm32-unknown-unknown lib 检查通过 |
| 原生映射与交付 | 9项通过（含2个子进程入口）；真实只读页写异常、跨线程保留、1／4097／65536字节、不可继承、原句柄重复读取、可写句柄拒绝、无效／超限／已退出目标均覆盖 |
| 跨进程生命周期 | 10项通过（含1个子进程入口）；真实 reader 冻结输入与 Reply 后仍存活、退休保留额度、撤权／到期／退休、外来宿主／消费者／broker、坏帧后仍存活、退出码91、Drop回收、无Reply非阻塞轮询、提前EOF及启动失败 |
| 执行底座全量 | 全特性88项通过（包括上述9＋10），全目标严格 Clippy 通过；文档测试通过，0个示例 |
| Flutter／真实宿主 | 5项通过，覆盖实际内容与偏好、活动库恢复及在线文字工具；本轮无UI修改，未重复整个82项Flutter套件 |
| Windows | Release 构建通过，实际程序产品版本27+32；合成资料启动、登记与渲染通过，原生背景API、静音WAV解码／时钟／跳转、声音互斥与恢复不自动播放通过，退出码0 |

独立审查发现并修复：原单管道线程在等待 Reply 时无法响应提前 close_input，现分离 stdin 与 stdout 所有权，用实际无Reply子进程验证收到EOF并正常退出。

首次父端全量回归在坏帧测试失败：测试 harness 已输出非法帧，父端拒绝后关闭输出管道，测试辅助程序再次写入触发 BrokenPipe panic，破坏“保持存活”的测试前提。删除冗余写入，保留真实存活及额度断言后，独立目标目录全量88项与严格Clippy均通过；未弱化生产权限或回收条件。

最终实际应用证据目录：`build/workbench-host/test27-runtime-cb2e3e7c639a4f318f272f280a084b98/`，包含result.md、result.png、运行日志及合成数据，活动库登记存在。已检查运行报告；该默认工作台检查不代表新增共享传输已经接入UI，也不是桌面背景像素比较或新增物理交互验收。

包内宿主与独立最终 Release 宿主 SHA-256 均为 `0bbfeb774cabe751b92d2868798c1705245b95544e5e4171a005d4cf1f8e844f`；随包插件为 `7fed4dea2d3dc6daad1c17fec219014aaf627f7c0141068884d81dca41f11212`。Windows程序位于 `build/windows/x64/runner/Release/morrow_studio.exe`，运行需保留整个Release目录。

主要复现：

```powershell
cargo test --offline --locked --manifest-path core/Cargo.toml --target-dir build/transfer-contract --test shared_transfer
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --all-features --target-dir build/reader-review
cargo clippy --offline --locked --manifest-path plugin_runtime/Cargo.toml --all-features --all-targets --target-dir build/reader-review -- -D warnings
```

## 精确边界与下一步

本轮是 Windows 可信读取辅助进程原型。辅助程序只读取和返回固定字节，不执行 Wasm；没有接入默认工作台数据通道，也未随默认Windows程序安装为生产任务runner。接口依赖辅助进程不向其他进程转交句柄，不能把它宣传为恶意本机代码沙箱或进程树隔离。

返回字节和协议编码仍发生复制；映射额度不是整个进程RSS、辅助进程／线程数量或全部broker的全局资源限制。既有字节不能因撤权收回。观察失败、回收线程创建／等待失败路径已作静态审查，尚无系统故障注入实证；资源记账释放不等于内核总资源量测量。

下一步仍需生产任务后端接入及全局额度、三语言共享租约接口、依赖锁定与服务代次、A→B固定输入→结果提案→核心提交→完整审计→隔离重放。平台后端各自验收，Windows和核心Wasm编译不代表浏览器／移动或其他桌面共享内存已完成。M3–M7和0.2.0退出门槛保持未完成。
