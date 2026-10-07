# dev20 Index 新卡待办业务接入审计

2026-10-07。**最终fresh实际Index业务16/16、字段20/20、剪贴板15/15均PASS、0 skip；三份日志读取相同冻结Index字节。** 剪贴板新增用例采用真实ETS `EditorDirectInput`，覆盖完整selection交接、两条自动标题路径和foreground stop/rebind。此worker编辑新 `tool/index-editor-todos-integration.test.cjs` 与本报告，并经root追加授权，最小同步两份既有field/clipboard测试脚手架的新输入依赖及新增剪贴板回归；未缩减既有业务断言，不修改Index、Rust、平台模型、Git、设备或历史v20报告。这是源码集成检查点，不是产品验收。

用户最新要求立即推送当前更新：root将其作为 **codex/ArkTsUI上的源码集成检查点** 交付，不合入或更新main，不继续扩大功能或升版本。AppScope仍为dev19；此次接入源码尚无完整设备或发行资格。实际Git提交/远端核对由root记录，本报告不预报推送成功或引用自己的commit SHA。

## 实际执行边界

测试沿用既有 `index-editor-field-integration.test.cjs` 的fresh Index方法抽取与installed SDK TypeScript编译方式，实际执行Index的 `command/submit/retry/save/flushDraft/retireDraft`、`inputReadyFor/newCardTodos`、`editorInputChanged`、`retirementCapture` 及字段访问/计数方法，加载实际ETS `EditorDraftCoordinator/EditorFieldPolicy/Command`。新business suite的编辑值、field worker、direct-input readiness与business/draft-discard provider回执为synthetic；不会把这些结果解释为native持久化或设备IME证据。业务测试不重新复制Rust任务整理、TaskId或Store实现。

剪贴板原13项保留，新增两项切换为真实ETS `EditorDirectInputCoordinator`，执行实际Index的 `adoptExternalInput/bindInputRules/applyDirectInput/directInputChanged/foregroundChanged` 和实际clipboard/import流程。formatter/native/import响应仍为受控synthetic，验证的是完整TextValue和准入状态在实际Index与实际Coordinator间的交接；不重复资格化Unicode算法、SDK事件时序或设备附件能力。独立 [DirectInput审计](direct-input-audit.md) 与 [最终24项模型日志](direct-input-final-tests.log) 覆盖协调器算法；本报告不以它们代替Index消费合同。

root本轮接入完整todos准入/原请求发布，并修复成功消费及discard await后的owner/epoch/fullraw检查。Unknown继续保持原serialized请求及原raw快照，显式retry不重建operation/代次或借用后来输入。已有V2普通edit与new-card原newline草稿仍保持不同接口语义。

期望成功合同是：准确create回执只消费同editor/draft/epoch及完整TextValue/资产快照，在退休准确已确认代次前保留完整raw；不先normalize或清空raw生成新代次。较晚的text/selection/composition、edit-away-back或owner变化撤销消费。discard返回期间变化同样不能dispose/close当前编辑器。不匹配或Unknown清理回执需保留原清理意图，等待显式核对。

## 测试范围

- 完整create原todos/五字段/确认pin、同一个原business请求、准确draft generation/operation；不调用task_add。
- todos composition和完整aggregate overflow仅保留raw，不提交部分或候选内容。
- direct-input与row格式状态未ready时阻止business；完整raw仍能确认保存。101原空白行不能先去空绕过行数限制。
- backend `not_committed`保留完整值/pins并释放该失败请求；后续显式保存有新的operation。
- 异常Unknown及structured unknown/committed error保持exact原wire；显式retry不借用较晚todos、不自动重新保存。
- 实际raw Coordinator的Unknown原请求retry只确认原todo快照；较晚rows需新的raw generation/operation确认后才作为create发布。
- backend等待中的新text/range/composition/edit-away-back/owner撤销消费；成功时准确退休完整raw代次，期间不写中间空todos。
- discard等待中的新输入/owner不能dispose当前编辑器；不匹配discard回执保留原请求。
- 实际SDK普通输入回调在retiring/retirement Unknown期间保存原事件文本及preview、增加epoch并置incomplete；有效preview在本地重建完整TextValue，旧Coordinator不接受draft.update。ACK之后旧草稿标为retired时flush返回false，raw和live editor保留；重复退休和business准入亦被阻止。
- existing V2 edit只发送空todos，保留待添加单条raw与已有TaskId详情路径；create的cmd/raw不一致时不准入。
- 实际paste交接完整selection/metadata后，真实DirectInput的当前全值与raw相同，`inputReadyFor('create'/'edit')` 与 `canPaste()` 为true；foreground恢复后每一body slot均恢复准入。
- clipboard自动标题与selected-import自动标题均交接完整末尾cursor和字段revision，真实DirectInput全值与已确认raw相同，create/edit/paste准入为true。新卡row readiness作为独立fixture明确提供，不以这些标题用例宣称待办行组件设备就绪。

脚手架追踪实际save/submit/retry/flush/retire返回的Promise，受控provider gate释放后有界等待全部业务工作与迟到异常，再清理协调器。换owner用例实际替换Coordinator/不同scope；清理竞态只要求当前live owner和完整输入保留，允许root安全handoff新的草稿，不强制仍使用旧Coordinator。

新卡路径是HMOS development adapter将未提交newline文本投影到一次V2 create的明确适配，后端原子性见 [create-todos-audit.md](../create-todos-audit.md)。实际Flutter legacy save的V1 todos/completed集合、既有V2拒绝非空editor todos及TaskId详情操作是不同业务边界；本测试不宣称Flutter V1/V2 schema、保护会话或完整UI等价。

## 最终fresh结果与字节身份

最终三个进程均读取实际Index SHA `B72ECDABCF33EEBADD5348728C0150F1EE197A68DCA7EE9434CFCCC710DB1A1C`，installed SDK TypeScript **4.9.5**。以下均为0 cancelled、0 skipped、0 todo：

| 最终范围 | 结果 | 原始证据 |
| --- | --- | --- |
| 新待办business suite | 16/16 PASS；1746.3737ms | [todos-business-final.log](todos-business-final.log) |
| 既有field集成 | 20/20 PASS；1956.7065ms；原业务断言保留 | [field-integration-final.log](field-integration-final.log) |
| clipboard集成 | 15/15 PASS；3176.8632ms；原13项保留，新增2项实际DirectInput回归 | [clipboard-integration-final.log](clipboard-integration-final.log) |

冻结时读取的测试与关键ETS字节：

| 文件 | SHA256 |
| --- | --- |
| `tool/index-editor-todos-integration.test.cjs` | `5603C9A099BC96FC73FAA189A5813F0C0F86C31F4CFFFAEEDD5D3C9EE6BBC4F1` |
| `tool/index-editor-field-integration.test.cjs` | `97514352BADA8A2C939779E2606899DAB9E46118A86332F2AB069DE53527F6C0` |
| `tool/index-clipboard-integration.test.cjs` | `C163B909DD160F0733870D3C4BC59F7F6E3D5FCAC3FB4A5B71B3A52F639C39C8` |
| `entry/src/main/ets/model/EditorDirectInput.ets` | `A56119B3DFC3EA52E83082F0654C6BD9164383A646AF503B6D72CE2F0D7E6030` |
| `entry/src/main/ets/model/EditorDraft.ets` | `CAA435F968CF5B945E463508DAF84B04D7A56C594EDF653C529DDA3F2173A0F5` |
| `entry/src/main/ets/model/EditorFieldPolicy.ets` | `10C3162E77A947335ACF0C3DF2E464C80195D0AC2BBC2922BBB09C0A8A626842` |
| `entry/src/main/ets/model/Workbench.ets` | `1A5BCBEE38CE7267B2E178FCFEFFB445AAA029AB73C9074B7593B147569269BF` |

## 历史失败与修复轨迹

首轮三个fresh进程均读取实际Index SHA `F4F91D164F48E02C86AB806E2E254BAADAC9734AD37947CC95658529C217C880`，installed SDK TypeScript4.9.5：

| 首轮范围 | 结果 | 原始证据 |
| --- | --- | --- |
| 新待办business suite（当时15项） | 13 PASS、2 FAIL、0 skip；2130.1319ms | [todos-business-stage1.log](todos-business-stage1.log) |
| 既有field集成 | 20/20 PASS、0 skip；2640.6825ms；业务断言未缩减 | [field-integration-stage1.log](field-integration-stage1.log) |
| 既有clipboard集成 | 13/13 PASS、0 skip；3259.3031ms；业务断言未缩减 | [clipboard-integration-stage1.log](clipboard-integration-stage1.log) |

首轮失败为：旧create成功回执仍无条件将replacement editor的selected切回旧card；discard await期间新raw仍会被dispose/close。root修复后，第二轮Index `66BDDB35376A252BC98611E611AC729BCB60BBBF899ED9DE64BE31EAEDF68977` 的 [business16项](todos-business-stage2.log)、[field20项](field-integration-stage2.log)、[clipboard原13项](clipboard-integration-stage2.log) 均PASS；分别2776.6265ms、2689.289ms、3740.3932ms。后续完整外部输入交接和退稿原事件保留继续演进，因此最终资格取上表fresh日志，不以此旧阶段替代。

新增真实DirectInput自动标题回归的 [focus原始失败日志](clipboard-real-direct-focus.log) 为2项中1 PASS/1 FAIL。根因是实际 `EditorDirectInput.bind` 在stop后重bind多字段时，第一字段令全局active恢复，后续相同值的字段被重复早退留在stopped阶段。root增加 `previous.phase !== 'stopped'` 条件并加入实际多slot恢复模型回归；最终clipboard15项保持原标题/准入断言通过，独立最终模型24/24 PASS。此失败不归为fixture差异，也未删除原始证据。

最终business前一轮 [stage3日志](todos-business-stage3.log) 为14 PASS/2 FAIL、2079.014ms：测试按旧`title`/`description`键读退休事件Map，而当前生产按`title:text`/`description:text`区分原文本与selection事件，导致JSON.parse(undefined)。仅修正测试读取实际键并增加完整preview重建断言，生产字节保持上述B72 SHA；最终16/16 PASS。此轮是fixture失配，不把它列为当前生产缺陷。

## 构建及开放边界

root执行的 [最终SDK构建日志](hap-final-integration-build.log) 为 **BUILD SUCCESSFUL，10.732s**；[构建capture](build-freeze.log) 与 [磁盘manifest核对](build-manifest-disk.log) 为 **314输入、PASS**，AppScope仍 `0.1.0-hmos-dev.19`。最终 [artifact记录](build-archive.log) 为 `.build/artifacts/dev20-integration-checkpoint/entry-default-unsigned.hap`，**28,533,476 bytes**，SHA256 **`4769015976A302099D4CDC6ABB49DE3F7A836AE3559FED1AD6F56A677F6C460E`**，**signed=false、installed=false**。本worker只读取这些root证据，没有执行构建、native copy、签名或安装。

上述方法测试与构建均不是设备交互、完整Flutter视觉/操作等价或发行资格。全tool suite的最终冻结、staged manifest、提交与远端核对由root另行记录；本报告只声明上表三个限定suite结果。源码检查点历史报告保留当时Index未接入事实，本报告独立记录接入后的fresh源码SHA、实际日志与剩余差异，不追改历史证据。完整Flutter/Windows目标保持OPEN。

**退稿确认后的迟到输入接续合同仍OPEN。** 实际Index回调已经保留原事件、preview和本地完整值，增加epoch/incomplete；旧scope退休后本地flush/business/重复retire被阻止，界面如实显示未持久。限定测试证明晚到raw/live editor不被旧回执清空或关闭，旧Coordinator不写入这些事件；不证明服务器discard旧代次后，新输入已经以新draft/source/CAS接续并跨重启恢复。实际attachDraft sender的旧scope网络守卫仅经源码读取，本suite的synthetic sender不资格化该closure。此项durable接续、SDK事件的真实设备行为及发行验收仍未完成。
