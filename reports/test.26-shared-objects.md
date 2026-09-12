# test.26：固定共享对象、Windows只读映射与执行租约

日期：2026-09-12。应用 `0.1.9-test.26+31`，审计／工作台宿主 test.26；核心 crate 仍为test.22（内容格式6），执行后端和SDK仍为test.11，工作台guest crate仍为test.13，随包清单版本test.26。本轮扩展独立资源模块，不升级既有运行期v7或内容持久格式。第一方AGPL-3.0-only，仅本地开发，没有推送、打标签、Release或上传。

## 已完成的实际变化

- 新增预编译 Cap’n Proto 共享对象描述：固定版本及schema摘要、broker／对象／代次、长度、内容SHA-256与连续逻辑分段。非零身份、16MiB对象、64段、16KiB描述消息、整数溢出、遍历／嵌套和尾字节均有检查。描述是数据而非授权，不能持久化为可恢复权限。跨边界未对齐切片已独立验证。
- 新增真正Windows匿名分页文件映射：可信复制到唯一写视图，确认撤销该视图后才发布只读视图。指针／句柄私有，不开放写入接口，无跨scope去重或页面池。原缓冲后续改动不影响固定内容。
- 新增 SharedObjects，绑定真实HostRuntime身份；发布绑定生产者，显式授权绑定消费者、scope、对象与期限。新授权要求原生产者仍就绪，已有消费者租约独立受退休、撤权、到期和自身实例状态约束。不能凭描述、自报调用者或内容能力自动取得共享数据。
- 退休停止新访问，但旧Mapping仍持有原始只读页。旧分配继续占对象数和64KiB取整额度，最后Mapping释放后才退出记账；不凭超时或代次变更复用旧页。额度是每broker保守分配限额，不是整个进程RSS或所有broker的全局额度。
- 纯Wasm转换从固定Mapping生成唯一Invocation，按现有任务ABI复制至guest，当前输入最多64KiB。执行前与交付结果前分别检查可信单调时间、宿主／消费者绑定、租约和对象；到期或撤权丢弃新结果。不把UI输出或转换结果冒充内容已提交。

三个子代理分别交付描述协议、受控原生边界、独立Wasm／资源验收；主代理实现共享资源服务、接线、真实Rust资格工具及集成。自动审批最初拒绝放宽根级unsafe限制，用户随后明确批准仅 `shared_memory.rs` 的限定原生调用。实际代码保持其余执行模块 `deny(unsafe_code)`，仅该模块单独allow；没有借其他crate或临时harness绕过审批。

接口和精确保证见 [共享对象](../docs/SHARED_OBJECTS.md)、[描述契约](../docs/SHARED_OBJECT_DESCRIPTOR.md)、[原生映射](../plugin_runtime/SHARED_MEMORY.md)。

## 实际验证

| 范围 | 本轮结果 |
| --- | --- |
| 描述协议 | Windows 10项通过；包括错误分段、字段／摘要、边界、尾字节及1..7字节偏移的未对齐输入 |
| 可移植核心 | wasm32-unknown-unknown lib检查通过，最终无新增警告；Windows全目标严格Clippy通过 |
| Windows映射 | 测试框架5项通过，其中1项为故障子进程入口；缓冲隔离、8个跨线程Arc读者、页／容量边界和实际写保护资格已验 |
| 实际只读保护 | 隐藏故障子进程先写准备标记，再故意写只读页，确认0xC0000005退出；只在该测试进程抑制错误弹窗，10秒截止 |
| 共享资源 | 独立14项通过：描述篡改、scope／broker／host／实例错配、到期、撤权、退休、最后释放记账、租约／映射预算、纯Wasm实际输入及最后授权 |
| 执行底座 | 全特性共74项通过（含上述5＋14），严格Clippy全目标通过；文档检查通过，0个文档示例 |
| 实际Rust guest | qualify_shared_objects使用实际编译的工作台Wasm，固定abc后修改原缓冲，仍返回ABC；不创建卡片；旧Mapping持有期间新scope分配因预算拒绝，释放后分配新对象 |
| 工作台回归 | Flutter／真实宿主5项集成通过，覆盖内容、偏好、管理库恢复及在线工具；本轮未重复整个Flutter 82项UI套件 |
| Windows构建 | Release构建通过，实际程序使用新合成资料完成登记、渲染、原生背景接口、静音WAV解码／时钟／跳转、声音互斥及恢复不自动播放，退出码0 |

最终实际Windows证据目录为 `build/workbench-host/test26-final-59b5adbf2619405296554e6032ababd0/`，含result.md、result.png及合成数据；产品版本 `0.1.9-test.26+31`，活动库登记存在。已核对运行报告，未把该默认视图运行扩称新的物理交互或共享内存UI演示。

包内宿主与独立最终Release宿主SHA-256均为 `d857981d841a6ab5b18d5b672484cf9644e767eff28aa83af2730c2b59387c24`；随包插件SHA-256为 `1fd36f3299e7b32a456a073a79b23331c067bf05668414b796a8a19dad813570`。最终消除Wasm专属未用方法警告后重新构建并替换包内宿主，再用上述新目录运行确认。

首次资格示例编译因访问Document私有nodes字段失败，改为公开nodes()后全量回归及真实Rust示例通过。独立审查发现无效外来请求可以先推进broker时钟，已将身份／对象检查前置；加入未来时间戳不破坏合法后续请求的测试并通过。

复现主要命令：

```powershell
cargo test --offline --locked --manifest-path core/Cargo.toml --test shared_object --target-dir build/shared-contract
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --all-features --target-dir build/shared-broker
cargo run --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --example qualify_shared_objects --target-dir build/shared-broker -- build/workbench-host/bundle/workbench.morrowplugin
```

## 尚未完成与下一步

本轮是M3必要的固定输入／资源租约原型，不是完整M3退出。默认工作台业务还未改用共享对象传输；实际Rust资格用同一包的两个零内容能力连接，不声称两个不同插件已经自主完成依赖调用。

仍需最小权限的跨进程句柄交付、读者异常退出及无法确认释放的资源处理、平台后端、全局资源限额、原生映射系统错误与实际回收资格、三语言guest租约接口、用户／工作区策略、版本化依赖锁定与服务代次。随后贯通A→B固定输入→结果提案→核心提交→完整宿主审计→独立验证和隔离重放。

既有Mapping收到的字节无法因撤权“收回”；本模块保证新访问及新结果交付的授权边界。复制到Wasm是受控适配，不宣称零复制；没有跨进程映射交付或即时终止证明。非Windows实际映射后端返回Unsupported，核心Wasm编译不等于浏览器共享内存实现。描述、摘要和租约目前不作为持久审计／重放证据，内容库格式未变。所有验证使用合成资料，M3–M7与0.2.0仍未标记完成。
