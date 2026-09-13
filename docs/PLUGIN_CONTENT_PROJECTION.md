# test.39：版本化内容投影与独立重建

本文冻结test.39的v1语义，内容库格式10；test.40另以[v2捕捉关联](PLUGIN_CAPTURE_PROVENANCE.md)记录实际转换、采用事件及编辑端点。默认工作台新建及全部apply写动作保存完整历史卡片和宿主事实，从实际插件观察按固定v1规则导出原命令及完整结果CardRecord。独立重建不需要最新数据库内容。这是capture→create因果链的必要基础；捕捉来源、用户编辑过程与最终新建之间的关系仍未补齐，不能据此把整个因果链或完整审计重放标为完成。

设置继续使用test.38的专用分页批次，尚未接入本文的完整Card投影。旧schema1证据仍可按原路径核对和重试，不能补出历史未保存的prior事实。

## 原始事实与固定契约

`workbench_host/schemas/projection.proto` 是预编译Protobuf契约。原字节存于TaskEvidence schema2的`batch.intent`，`intent_type`固定为`morrow.workbench.content-projection.v1`。本版只接受一条实际`workbench.command`观察；不能把额外观察或其他任务路由静默解释为当前内容投影。

| HostProjection v1字段 | 作用 |
| --- | --- |
| `schema_version` | 固定为1，未知版本拒绝 |
| `operation_id`／`target_id` | 原操作和目标身份，绑定生成命令及Commit |
| `prior` | apply前完整CardRecord原字节；Create为空。保留外层、业务正文、附件及其他未知字段 |
| `attachments` | 最终有序宿主选择事实：ID、名称、媒体类型、字节数和SHA-256；输出资产按相同顺序核对ID／名称／字节数 |
| `observed_now` | 本次宿主观察时刻，必须与原Request.now_ms一致；不是外部可信时间证明 |
| `undo` | Restore时的原修订及宿主撤销期限；其他动作不接受撤销事实 |

投影意图最多12 MiB，原卡片最多8 MiB，附件事实最多1024项，同时遵守既有字段长度和业务数量限制。解析前校验长度、字段总量、重复已知字段、固定嵌套结构和wire类型，拒绝group、截断数据及未知版本。未知可选事实保持在证据原intent字节中，不改变v1已知语义；改变投影语义的扩展必须有新版本，不能用忽略字段的方式默默启用。

事实是宿主记录的数据。单靠这些字段不能认证附件来自哪个系统选择器、原blob是否仍可取回、时钟是否准确，也不能证明prior已与前一份签名提交完整连链。摘要固定、实际任务重放、投影一致性、签名信任与运行授权分别核验。

## 从实际捕获到唯一内容命令

新操作先冻结原prior、基准修订、时间／撤销窗口及可用附件候选，执行一次实际插件任务。宿主从已冻结事实选出输出资产对应的最终附件元数据，把真实观察与投影事实封成一份单页批次；不会为了补投影重复运行guest。`prepare_intent`只记录有界事实，不授予权限，也不签发执行来源证明。

`derive(evidence)`在固定v1实现中完成以下步骤：

1. 核对批次版本、意图类型、单观察数量及处理器／输入输出类型；观察必须退出0、无故障、无核心调用且为关联成功Output。
2. Create要求无prior、空current、目标等于原proposed身份且时刻为0。apply从完整prior解码原current并逐字段比对；Edit沿用旧宿主行为，仅在送入插件的current中清空description／hypothesis／conclusion三个大字段。其余宿主默认请求字段和原观察时刻也需相符。
3. 核对输出身份及有序附件事实。Create按固定业务持久化规则建立新Card，预览保持旧行为的空值；apply以原正文保存未知字段，将description按UTF-8字符边界截到最多16,384字节作为预览，再用ContentChange.propose作用于完整prior。
4. 调用既有transaction构造器生成原CreateCard或SetContent命令，并返回完整结果CardRecord及原Request。旧卡片的关系、创建信息、预览其他字段及保留附件的未知字段通过原Card变换保留，不从UI摘要另建一个有损对象。

覆盖Create、Edit、Favorite、Todo、Stage、ToProject、Delete和Restore。Delete的观察时间进入插件输出；Restore同时要求宿主undo修订匹配、观察时刻早于deadline，并满足原deleted_at开始的8秒窗口。独立校验这些已记录事实不等于重新授予撤销权，也不刷新当前会话的undo期限。

`verify_commit(commit,evidence)`在derive基础上核对原command逐字节相同、command摘要、操作／事件／目标、结果修订、完整结果卡片SHA-256、有序附件摘要及唯一证据引用。比较的是完整结果编码，不只是标题或业务正文。调用者仍须先固定或验证原Commit容器；此接口不重编码Commit进行验签。

## 版本冻结与历史重试

实现位于`workbench_host::projection`，通过显式`VERSION=1`分派到`derive_v1`。v1冻结请求整形、业务persistence编码、未知字段保留、Create预览和Edit截断规则。将来这些规则或依赖库编码行为改变时，必须保留可解释旧证据的v1实现并引入新版本；当前实现不承诺任意未来依赖组合都能生成相同字节。

新投影证据的已提交重试先验证原Commit和原投影，再比较用户意图及原基准修订；使用当前内容授权复用原命令与原件。它返回原操作的历史结果，不覆盖后来内容、不刷新undo，也不重做暂存附件清理。旧schema1操作继续保留原兼容重试能力，但`derive`拒绝将其冒充拥有完整prior的投影证据。

## 独立内容重放工具

```powershell
morrow-content-replay.exe <commit-file> <commit-container-sha256> <evidence-file> <raw-evidence-sha256>
```

四个参数的摘要含义不同：第二个固定完整Commit容器文件字节，第四个固定证据解压后的原始Protobuf字节。工具先有界读取并核对两份外部摘要，解析原Commit与Evidence，再调用`verify_commit`；投影不一致在执行guest之前拒绝。

通过后使用`replay_batch(evidence, Limits::default(), 1_000_000_000)`进行实际纯任务重放。逐页策略为默认Limits，总燃料固定1B；通用Rust API允许可信调用方选择更高策略，不会自动放宽此CLI。工具不打开内容Store或加载凭据，不恢复原连接或权限。

退出0表示原命令／完整结果投影与实际观察均匹配；退出2表示实际重放不匹配；退出1表示格式、摘要、投影或执行策略等拒绝。标准输出显示已比较观察数，避免输出用户正文。外部摘要只固定指定输入，不建立作者身份；该工具不是签名核验器或来源授权证明。本轮新CLI的3项实际进程测试与严格Clippy通过，包括删除原库和凭据后的匹配、错误pin／自洽不同内容拒绝，以及实际观察不匹配退出2；准确记录见阶段报告。

## 格式10与容量迁移

先前4 MiB意图上限无法容纳原本合法且不可压缩的8 MiB原卡片；test.39因此将意图上限升至12 MiB，而不是截断未知字段或缩窄原卡片能力。TaskEvidence schema2布局、原始批次24 MiB和单操作16份／64 MiB上限不变。设置本身仍最多4 MiB。

内容库格式10接受扩大后的意图，格式9继续拒绝intent超过4 MiB的批次。9→10在同一事务校验旧状态、更新容量门槛并检查新状态，不重编码历史证据、Commit、签名或32 KiB共享块。旧格式4～9按既有阶段迁移，独立只读核验接受5～10，不进行写入或修复。

## 当前范围与后续

本轮实现的是给定已记录宿主事实和历史插件观察的精确内容投影。capture来源及后续编辑关系、设置和其他宿主业务的完整投影、查询／依赖图／宿主响应证据、全局保留与GC、未提交意图恢复和完整多平台验收仍待完成。

历史原件、原命令和结果即使能精确重建，也不能据此宣称捕捉来源真实、插件业务结论正确或运行授权仍然有效。实际动作、未知字段、附件、撤销边界和CLI专项结果统一记录于[test.39阶段报告](../reports/test.39-content-projection.md)；源码接入不代替验收结论。
