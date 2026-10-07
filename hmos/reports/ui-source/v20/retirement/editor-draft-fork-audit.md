# 独立 development raw-draft fork 协调器审计

2026-10-07。此worker只新增 `entry/src/main/ets/model/EditorDraftFork.ets`、`tool/editor-draft-fork-model.test.cjs`，窄改 `EditorDraft.ets` 的暂停写入、开发fork View proof/复制/验证与已确认pin来源转换，并写本独立报告。另按root要求在本retirement目录新增隔离SDK检查脚本和证据。未改Index、Rust、其他model、已发布integration报告、原smoke脚本或旧harness，未操作Git、native采用、产品HAP或设备。

**fresh实际ETS fork模型17/17 PASS、原EditorDraft模型25/25 PASS，均0 skip；新独立API26 SDK smoke构建PASS，8.827s。** 这是独立协调层模型和SDK编译资格；controlled provider/digests不是Rust Store、跨重启、ArkUI事件、Index接线或设备证据。不宣称protected S1/S2 handoff、业务source rebase、Flutter legacy todos或TaskId业务已reconcile。

用户本次要求推送GitHub；root按 **native/ETS基础能力checkpoint** 交付，**EditorDraftFork目前未接入Index，没有设备fork验收**。下列接线顺序是后续实现合同，不是已完成的用户界面行为。本worker不操作Git，不预报远端成功，也不改main或此次manifest。

## API与顺序

`new EditorDraftForkCoordinator(parent, childScope, firstRaw, firstOperation, hooks, debounce=500)`，hooks为：

- `send(serialized): Promise<DraftRecord>`：严格封装native原wire，不在hooks中重新生成operation。
- `changed()`：发布模型状态。
- `operation()`：已确认child后续普通draft_save的operation工厂。
- `isCurrent()`：Root提供live editor/owner守卫，不以内容相等替代owner/input epoch。

构造先检查父Coordinator未disposed/saving/unknown/conflicted，confirmed active且current_generation准确；父proof必须有合法native `request_sha256`。`firstRaw`必须是构造时父current完整快照。child只改变draft_id；card_id、source_kind、source_revision、source全部原bytes继承父scope。原业务卡片列表和新修订不能成为隐式source。即使业务已提交，旧source的CAS冲突仍保留。

所选assets只能是该confirmed父pin库存的严格顺序子集，aliases逐项一致；未确认origin2 staging不是可借用的父pin。首wire把这份子集的来源转换为origin4，保留完整TextValue、asset ID、order与aliases。检查失败发生在暂停父写入和transport之前。

构造后父`pauseWrites()`取消debounce、阻止新自动/显式写；父仍允许`update`完整raw及selection/composition/assets。暂停不cancel既issued请求；既issued回执仍能准确确认其冻结baseline，后续raw不会因该回执自动发出。

Root接线顺序：

1. 明确完成原业务Unknown核对；确认父原proof，保完整最新输入，创建fork。
2. `await begin()`；异常Unknown只用`retry()`发送同一个原serialized wire，不自动重放。
3. 准确首ACK后，核对同live owner；`openChild()`建立read-only restored child。首ACK仅确认冻结firstRaw，不替换较晚父current。
4. 若`isCurrent`只认可`editorDraft === parent`，在替换writer前用`retirementCommand(retireOp)`冻结原清理wire；或让守卫明确支持同view owner的parent→child过渡。此方法只构造identity，不发请求，不资格化最新raw已确认。
5. 把父仍捕获到的最新完整current交给child.update，并先切live writer到child；随后flush直到最新完整raw确认。已确认child pins自动转origin3，保留asset subset/order/aliases，不产生持续dirty循环。
6. Root独立检查owner、input epoch、capture completeness与child当前完整confirmed值；通过后发送固定条件parent-retirement原wire。后续SDK事件继续由child捕获；不能在父退休ACK后才更换writer。

若latest assets不再是可确认的库存，不得猜pin授权或丢失原文以完成交接。Root需保持阻止清理并如实显示未确认。fork coordinator本身不处理原业务`pending`的持久intent，也不处理原未选中staging imports清理。

## 冻结wire与receipt合同

首原wire为native已冻结协议：

```text
{ action:'draft_fork', fork:{
  child:<完整原DraftWrite，expected_generation:'0'，固定first operation>,
  parent_draft_id, parent_generation, parent_save_operation, parent_request_sha256
} }
```

`begin()`只允许未issued冻结状态；并发调用等同等待同一issued Promise。Unknown的`retry()`不借用较晚raw，不生成新scope/operation。第一次明确known rejection可以显式`releaseRejectedParent()`，之后不再使用该fork实例；曾Unknown的后续known rejection不能降级或释放原请求。有效已提交结果不能释放父，也不能假装整笔未提交。

首ACK逐项验证：完整scope source bytes、child operation、generation1、historical active、全部TextValue及资产选择、exact fork_link、native canonical request digest的lowercase64hex格式。stored pins数量/index/selection/aliases/order与全raw一致，并与冻结父pin的name/media type/byte length/SHA一致。

ETS不另写protobuf编码或SHA算法；native负责把request_sha256与权威canonical原request历史核对。ETS同时验证full request fields与proof；digest单独不能成为存储权限或capture资格。旧normal-only synthetic fixtures可无digest，normal原规则保留；development fork/retirement proof必须有digest。

有效historical first ACK但current_generation已推进或current_active=false时，模型保存已证original committed record并标`conflicted`，`unknown=false`；禁止openChild或retry复活。完全不匹配或malformed receipt仍保原wire并标Unknown。owner在await期间更换时，准确record可只读保留，但openChild拒绝向另一owner安装。

普通child save继续验证不变fork_link和原source bytes，保留原generation/原wire retry规则。origin4仅允许已验证的first fork request；后继用同child prior pins的origin3。普通非fork路径仍拒绝origin4。inactive或advanced restored child保持conflict，resumeWrites不会复活。

development `fork_retirement` marker验证schema、child identity、retirement operation、原parent save operation/generation/request digest与准确inactive next generation。已具有fork ancestry的child自身退休时可以同时携带不变旧fork_link与新retirement marker；二者不能用来解除inactive状态。

## 实际验证

| 范围 | 结果 | 原始证据 |
| --- | --- | --- |
| fresh实际fork/draft ETS，installed SDK TypeScript4.9.5 | 17/17 PASS；0 failed/cancelled/skipped/todo；2307.0147ms | [fork-model-tests.log](fork-model-tests.log) |
| 原EditorDraft模型25项，原断言范围未改 | 25/25 PASS；0 failed/cancelled/skipped/todo；404.0835ms | [normal-draft-regression.log](normal-draft-regression.log) |
| 新隔离API26 SDK，实际引用fork constructor/public API | BUILD SUCCESSFUL 8.827s；CompileArkTS 2.867s；34 executed，0 up-to-date | [fork-sdk-build.log](fork-sdk-build.log) |
| 隔离SDK构建前后来源/副本/生成wrapper核对 | 两次PASS；309 copied inputs、5 generated wrappers，实际两model字节保持冻结值 | [before](fork-sdk-verify-before.json)、[after](fork-sdk-verify-after.json) |

17项覆盖父pause完整capture/自动及显式write门禁、issued请求不cancel、selected subset/order/aliases/fixed firstraw/source/proof、kind1原baseline/legacy raw rows、首ACK期间较晚complete composition/selection与child后继write、proof getter复制、原wireUnknown显式retry/不降级、owner替换、historical committed/current-changed conflict、wrong receipt/hash形状/fullmetadata/assets bytes、normal拒origin4及source变更、固定retirement identity不发送、duplicate identity/unconfirmed pins/scope失败、combined ancestry/retirement marker与inactive不复活。

在仓库根目录PowerShell可独立复跑（两个进程均fresh读取实际ETS；不操作数据库或设备）：

```powershell
& 'C:\Program Files\Huawei\DevEco Studio\tools\node\node.exe' --test hmos/tool/editor-draft-fork-model.test.cjs
& 'C:\Program Files\Huawei\DevEco Studio\tools\node\node.exe' --test hmos/tool/editor-draft-model.test.cjs
```

默认使用installed SDK TypeScript；新suite也支持 `HMOS_TYPESCRIPT_PATH` / `HMOS_TYPESCRIPT` 覆盖编译器路径。计时器为controlled，不依赖真实设备或等待网络；所有provider gate均在测试内显式resolve/reject。

此test直接fresh读取生产ETS，用installed SDK TypeScript transpileModule执行，formatter/digest/provider与clock均controlled。模型test本身不是ArkTS declarative编译；下述新隔离SDK smoke补充了真实ArkTS编译检查。两者均不代替native真canonical SHA、Store权威授权/pin读取、crash/reopen或实际Index/SDK事件证据。

| 冻结文件 | SHA256 |
| --- | --- |
| `entry/src/main/ets/model/EditorDraft.ets` | `A3AE1E99D198FCBA3CDEF741A6F138612AD16979BE42C7A8809003F00D04F965` |
| `entry/src/main/ets/model/EditorDraftFork.ets` | `2F5CC8586D2400F99EE7598DD816970E63D9E3D4B53C5DFFBC54CB4E4062263C` |
| `tool/editor-draft-fork-model.test.cjs` | `6A22502FEA43427953DC3075CC8B6A753DD343D82F8983AD92D0599898666517` |

## 独立API26 compile-only smoke

新脚本 [prepare-fork-sdk-smoke.cjs](prepare-fork-sdk-smoke.cjs) 调用原 `tool/checkpoint-sdk-smoke/prepare.cjs`，创建全新目录 `.build/checkpoint-sdk-smoke/fork-dev20-2026-10-07T09-58-58-796Z`。原prepare/verify脚本和旧harness未修改，新脚本不加入此次生产 `build-inputs`。构建与验证入口为 [build-fork-sdk-smoke.ps1](build-fork-sdk-smoke.ps1)、[verify-fork-sdk-smoke.cjs](verify-fork-sdk-smoke.cjs)。完整来源、复制与最终生成条目见 [fork-sdk-source-copy-manifest.json](fork-sdk-source-copy-manifest.json)，创建摘要见 [fork-sdk-prepare.json](fork-sdk-prepare.json)。

复制后只改写该新目录中原prepare明确生成的 `AppScope/app.json5` 和 `CheckpointSdkSmoke.ets`。前者改独立bundle/vendor/versionName；后者实际import两model，构造EditorDraftCoordinator和EditorDraftForkCoordinator，引用全部公开getters、begin/retry/openChild/retirementCommand/releaseRejectedParent及pause/resume/dispose类型。provider始终抛显式Error，不调用transport、Store或native fork。入口没有运行；本检查只要求这些引用经过SDK的CompileArkTS。

五个最终生成wrapper的准确bytes/SHA（不是产品源）：

| 路径 | bytes | SHA256 |
| --- | ---: | --- |
| `AppScope/app.json5` | 256 | `c80e92811b15bf340306926d688d6a0570d74af8e4469c87c7a459754a4b86c1` |
| `entry/src/main/module.json5` | 1197 | `118a0b40cd48e7f9495b5e496e038182a70bd7193dbe3d35c7b1b1a052975d20` |
| `entry/src/main/resources/base/profile/main_pages.json` | 50 | `beaec735946f5729b91041117f79aefa42a069039a56cf65885f0669313d2004` |
| `entry/src/main/ets/entryability/CheckpointSmokeAbility.ets` | 254 | `e0a4a05120ebf3784623d7ccf3e26041b67ba84bccde3a7e7d7084a419229306` |
| `entry/src/main/ets/pages/CheckpointSdkSmoke.ets` | 3126 | `8ee07fe8925bd5fdcd5a7ebd3e1ec56d1b2ffb56ca526a6a22939c5bae06c7bb` |

改写来源也保留在manifest：AppScope原生成241bytes / `65ad55d521ed0d6bc21528c673c8d7ac98ba421175db286b8c5a120fcabbbaca`，page原生成2332bytes / `29da9117c5a3c6429704180828cc692183d8c8ba0b49af0edc77b61b1734b629`。改写前先核对原生成条目，改写后更新manifest并逐个核最终wrapper。构建前后的verify同时fresh核现源及新目录副本；EditorDraft为29143bytes、EditorDraftFork为12805bytes，SHA与上述冻结表一致。

构建使用API26、`--no-daemon`，独立bundle `dev.morrow.hmos.draftforksdk`、独立versionName `0.1.0-draft-fork-sdk-smoke.20`。无ArkTS ERROR；原生成ability存在may-throw warning，另有CMake长路径、unused compiler argument和未配置签名warning，原始日志完整保留。产物仅在新隔离目录：未签名HAP **25,719,630bytes / `7560FEFB254E3875C261E3EEDE517E9A2BCF860D3AAC47FBABF4425B40BE3E56`**，见 [fork-sdk-build-result.json](fork-sdk-build-result.json)。它不是产品HAP，未安装、未运行、未采用其native二进制。既有CPP/static archives只是随副本参与编译，不构成fork运行或native身份资格。

可在仓库根目录独立复跑；prepare始终新建目录，build拒绝覆写已有sdk-build.log：

```powershell
$forkNode = 'C:\Program Files\Huawei\DevEco Studio\tools\node\node.exe'
$forkPrepared = & $forkNode hmos/reports/ui-source/v20/retirement/prepare-fork-sdk-smoke.cjs | ConvertFrom-Json
& $forkNode hmos/reports/ui-source/v20/retirement/verify-fork-sdk-smoke.cjs $forkPrepared.project
& hmos/reports/ui-source/v20/retirement/build-fork-sdk-smoke.ps1 -Project $forkPrepared.project
& $forkNode hmos/reports/ui-source/v20/retirement/verify-fork-sdk-smoke.cjs $forkPrepared.project
```

**runtime/device/installation均NOT_RUN，Index fork接线仍未完成。** 本SDK检查没有改变生产版本或此交付范围。

## 开放边界

这是HMOS独立development adapter的raw draft lineage，不是protected业务资格。父子两个授权journal事务可以先确认子pin承接再条件清理父，但协调模型测试不证明多对象原子提交、跨进程CAS或不受中断的设备输入时序。

本模型内存保留原fork/retirement wire；首child ACK之前的新capture或Unknown原wire没有因此自动获得跨重启持久intent。首ACK以后真正的持久性由native Store提供，需其独立reopen/故障测试证明。比该ACK更新的输入仍需自己的完整child journal确认，界面不得显示为已持久。

父source原样继承，所以已保存业务操作造成的旧source冲突不会被偷偷解除。newline todos仍可完整raw保留；将其迁入已有V2 TaskId详情或拆成task_add不是本合同。原未发布但selected的父pin需native逐字节确认承接，原未选中ready/import Unknown/staging owners保留其独立协议，不能依靠物理blob或digest绕过授权。

Index业务原request Unknown持久恢复、完整UI/IME、设备安装/验收、HUKS/签名/发行和Flutter/Windows全目标仍OPEN。本worker不声明提交、推送或合入main。
