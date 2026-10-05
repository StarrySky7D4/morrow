# C06 Windows 目录观察与分段字节 SDK

2026-10-05。新增独立 `fs-directory-v1`/`blob-transfer-v1` Rust/C/C++17 codec/state，另有可信已选择目录对象的 Windows native `DirectoryBroker`。本轮只验证普通临时合成数据与 keyless Store；G04/SDK26仍OPEN，原owner/IoWorker队列及公开guest目录能力未接通。C04/C05的dated证据保留原范围，不继承其PASS。

## 最终实际资格与计数

| 资格 | 实際范围 | 最终结果 |
|---|---|---|
| 目录库 | Release/offline/locked库内13个唯一Rust方法、own范围strict lint/格式 | 13 PASS，0失败/忽略/过滤 |
| Blob库 | Release/offline/locked库内23个唯一Rust方法：codec6/state10/FFI7、strict lib lint/格式 | 23 PASS，0失败/忽略/过滤 |
| Windows目录broker | 对持有root handle的真实有界枚举及原授权拒绝；合成Unicode/surrogate/hardlink/reparse/多批/限额/游标/清理边界 | 16 PASS，0失败/忽略/过滤；新增范围严格编译通过 |
| 独立三语言conformance | Windows native Rust/C11/C++17真正消费者；目录76（16接受/60拒绝）+blob62（15接受/47拒绝）=138 wire vectors | 共享判决通过；另有7个实际Rust方法与19个去重检查families。C++复执行8个已有C families，不另加方法 |
| 当前接线原件回归 | 原基础/依赖/映射/共享对象/owned-reader的精确parent名单，旧Wasm/包保持原件 | 实际42个parent方法PASS，0失败/忽略；属于定向suite，有过滤，见下文 |
| 当前接线网络回归 | 11个新编译实际程序：lib28/client10/managed_sse14/managed_ws10/sse13/sse_envelope3/sse_sdk2/stream6/websocket9/ws_envelope3/ws_sdk2 | 实际100 PASS，0失败/忽略/过滤；名单与C05相同，零新增方法 |

原件42不能写成“整体零过滤”：frozen-region实际7pass/14filtered，remote-reader9pass/1filtered；两个child helpers显式skip及其他不属于本次选择的方法不计PASS，三个child helper也不增加42。早期12个重复方法不新增计数。语料向量、语言复执行、检查families、库方法、后端方法与旧回归各保留独立单位，不把它们相加成一次全SDK验收。

本轮新增消费者实际在Windows native运行，没有新增directory/blob Wasm guest、静态包协商、公开import或required-feature路由。网络100只复用原C04 typed WS guests005、C05 typed SSE guests001及历史fixtures的精确原Wasm/package输入，全部只读；未重建或重封装旧包。详见 [目录/blob接口](../../docs/PLUGIN_DIRECTORY_BLOB_SDK.md)、[C04](ws-message-sdk.md)与[C05](sse-event-sdk.md)。

## Byte helper、native broker与原owner

目录version1/raw digest `ade60daee77497056a3fe618b38616331b8d5de61bdb494f703592e74ec3d5f7`。raw UTF-8/UTF-16LE名称和opaque IDs是观察数据；UTF-16LE保留孤立surrogate，拒绝NUL/分隔符/`.`/`..`。每页32条/16KiB合计名称/64KiB wire。DirectoryState固定epoch、连续页序与有限ID集合，actual wire先admission，语义失败不推进接受状态且保留费用，release关闭并释放resident IDs；不承担原lease/deadline/原IoBinding义务。

blob version1/raw digest `941db7c662815f8963b46bbb65a5c143d90f9e7217937e84c05d1b8f11024543`。Descriptor/Chunk/Receipt/End保留完整字段，nonzero epoch/ref/op固定，16MiB对象/60KiB chunk/64KiB wire；实际streaming SHA256验证唯一连续payload与whole digest。仅最近live原请求wire逐字节、digest及全部字段都相同才Existing；不同编码的相同语义、旧请求、foreign IDs/gap/overlap/错误摘要/提前End不推进语义。duplicate仍收request/response费用，不加payload、不rehash；至多保留最近64KiB原请求，没有完整对象spool。失败/cancel/cleanup不退admitted费用。VerifiedBytes只证明真实字节完整，不能解释成Store持久化、FileCreate、上传或业务commit。

blob borrowed decode无allocator解析原wire，payload经过原字节范围checked mapping；固定capnp unaligned避免未对齐native引用。C opaque owners、小型views、C++ move-only wrappers及error输出保持均遵循有效native memory/尺寸/对齐/alias/生命周期合约，不提供pointer sandbox或普通OOM恢复。Debug限定类型/长度/序号/offset/阶段，不打印payload/opaque IDs/digest数组；owned/borrowed及Snapshot/Acceptance嵌套输出有真实回归。

Windows DirectoryBroker消费可信宿主已打开的selected directory File，并用原Manager/HostRuntime/ManagedInstance/IoBinding执行FileList admission/预算与边界复核；实际查询root/child metadata而不以name重开文件，不递归、不跟随reparse。hardlink只形成不同目录项观察，不授权链接目标。root object身份并不证明其祖先路径或picker来源，命名空间变化也不能升级为祖先证明；并发目录编辑不保证原子快照。同步OS查询不能由合作式取消抢占。

原Core FileList仍Unsupported，公開guest目录import absent。Ticket独立取消、original IoWorker typed owner queue/idle cleanup、trusted picker/ancestor adapter与fresh unpredictable secret factory缺失。DirectoryBroker::new(secret)只接受可信调用者给出的bytes，不生成或保证fresh/unique secret；不能用新helper/字面secret/布尔now代替原authority。下一限定子任务应接原IoWorker的private typed queue、单页pending/预算/取消/未领取Unknown/真实owner join，再独立补trusted factory/provenance；读源码研究没有实现或测试这些接线。

## 失败、源码身份与兼容边界

完整runtime strict Clippy真实exit101，保留既有style诊断，不能标记整个runtime lint clean。两处新增范围诊断作窄修正后，当前原生16重新编译/执行及该范围strictcompile通过。目录FFI在opaque handle借用之前检查已知storage/输出重叠并增加回归。Blob初期保留两次编译前fence失败、schema换行与raw hash不一致、测试字面量语法和一次strict lint失败；后续独立审查发现derived Debug暴露正文，修正仅限新Debug并新增1个回归。原22方法候选与失败不被最终23覆盖。原件回归初期driver失败及重复12结果另存，最终42独立再执行。网络fresh compile/execute均exit0，原测试warning完整保留。

协调的四个已有parent输入仅为 `plugin_runtime/Cargo.toml`、`plugin_runtime/Cargo.lock`、`plugin_runtime/src/lib.rs`、`network_node_stream_001/Cargo.lock`，接独立目录path dependency/module；registry旧版本/来源/checksum及其他依赖边保持。新增独立库/测试/native module按各自ownership检查。SDK327/冻结57和旧guest原件不变，旧Core/schema/前端/app版本没有因本轮codec而变化；未commit/push/CI/发布。

各资格绑定自身指定source/tool/产物范围。目录库最终只证明其own9输入，nested记录中的较早root Rust formatter hash不冒充最新conformance身份；root独立最终run003及formatter refresh另有当前证明。network实际821输入前后恒同，新的241包公开离线cache/10,223文件核验，旧cache不写。C06当前index身份（75…）本轮保持；不能声称它与C05 index字节相同，历史index-byte drift actor仍NOT_YET_LOCALIZED。Git index不重置，不把历史差异归因给未证明的主体。

## 剩余门槛

真实whole-blob backend、upload/Store effects、watch/rename/delete和conditional Replace仍OPEN；conditional Replace继续Unsupported，禁止退化成check-then-overwrite或先delete再create。原owner生产批准、trusted selection/provenance、fresh secret factory、公开guest协议/协商和真正平台接线需独立实现/资格。Unknown不凭receipt、VerifiedBytes、EOF、cleanup、新helper或reopen自动重放。

Linux、GUI、账户/真实数据库/DPAPI、protected owner/session/IoWorker、公开服务、CI/Release及完整SDK26均未由这次普通synthetic资格覆盖。完整26目标不缩成已通过的子集；后续门槛见 [sdk-next-gates](sdk-next-gates.md)。
