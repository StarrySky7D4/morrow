# 插件主动依赖调用：test.30

这是已批准依赖锁之上的单层同步转换接口。默认工作台界面尚未接入依赖配置或这条执行路径。依赖声明和同快照批准规则沿用 [依赖锁](PLUGIN_DEPENDENCY_LOCKS.md)，完整插件系统与0.2.0门槛仍未完成。

## 契约与SDK

包须使用guest ABI v2并声明`transform-handlers-v1`、`dependencies-v1`、`dependency-calls-v1`，Manifest字段17携带dependency_call.capnp的LF标准化SHA-256。未声明新特性的旧执行入口拒绝新import；新包必须走带依赖路由的任务入口，不能退回普通run_task。新特性不自动授予提供者选择或内容权限。

固定导入是`morrow_dependency_v1.call(i32,i32,i32,i32)->i32`。请求仅包含call ID、已声明slot、有界输入，固定Cap’n Proto版本1及schema摘要。提供者ID、摘要、处理器、类型、共享scope和期限由可信宿主和当前批准锁确定。响应携带输出类型、字节、call ID及完整实际请求帧的SHA-256。不能重编码请求后替代原帧做关联校验。

Rust使用`dependency_call::Request::new`和`wasm::call_dependency`。C使用`morrow_plugin_dependency.h`的请求编码、原帧响应解码、opaque output，以及`mp_wasm_dependency_call`；C++用`dependency_request`与可移动的`dependency_output`。这些是同一固定协议的三种SDK，不安装TS/JS guest，也不要求Dart插件。SDK是便利接口，宿主独立实施检查。

请求帧和完整输出缓冲均不超过128KiB，两个缓冲必须不重叠。当前共享输入是1..64KiB，空输入明确拒绝；成功输出允许0..64KiB。调用只能发生在任务输入读取后、完成前；每任务call ID不可重复。默认8次，可信宿主可明确设置1..16次，包与Runner的host_calls预算仍可能更小。普通exchange和依赖call共用该预算，但这不是内容事务计数。

## 运行期授权与最终保存

`dynamic_dependencies::run`先验证当前Manager归属、原始连接、包摘要、启用闭包及取消信号，即使guest最终零次调用也不能跳过这些检查。可信宿主显式提供已经建立的真实provider实例；每次slot请求重新核对对应锁，缺失、重复或替代实例拒绝。

每次调用冻结输入、创建caller授权映射，再经现有Dependency交付provider执行。发布成功后立即接管清理责任，grant/map/执行/响应失败均退休新对象并释放临时映射。完成后资源回收不等于撤回guest已收到的字节。

本阶段caller和provider均为纯转换。尝试普通内容exchange会使整个任务失效，即使guest忽略返回码也不能形成有效结果；任何依赖拒绝、失败、陷阱、取消或超限都会拒绝最终输出。递归使用新依赖特性的provider拒绝，自动启动与多层异步调度待后续实现。

调用者可以再次加工提供者输出。最终`RoutedOutput`只能由运行时构造，保留调用者最终字节及每一次Dependency和DependencyOutput；保存到EditProposal后仍保留全部撤权状态，不能只检查最后一个提供者。预览后的实际保存再次验证全部端点、取消、单调时间及期限，并在核心最终提交guard中检查所有存活状态。目标卡片、操作ID、预期修订等仍由可信工作流决定；实际写入仍须调用者的原有内容能力和对象授权。

提交成功后停用插件不会回滚已提交内容。操作回执支持显式幂等检查，失败不自动重跑。哈希关联只证明当前帧关联，不是完整签名审计、批准历史或重放证据。

## 额度与平台边界

每个guest独立受fuel、内存和导入次数限制。最多16次provider执行与caller一次执行的预算相加；没有宣称全链只消耗一份fuel。保留的响应受调用次数和每响应64KiB限制，共享对象还受broker的已有额度约束。同步可信回调不是OS抢占，执行不得放在Flutter输入线程中。

Windows真实只读共享输入和三语言guest是本阶段验证目标；其他原生共享后端、Web应用接入、自动图启动、崩溃传播、多层等待环、全局跨任务额度、完整审计与隔离重放仍待实现。具体实际结果见 [test.30记录](../reports/test.30-dynamic-dependencies.md)。
