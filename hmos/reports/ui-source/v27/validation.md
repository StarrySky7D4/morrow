# v27 当前全文重新编辑与当前子草稿恢复

2026-10-09。基于 `4724f01f2c73f6289165696b28b3a8600840e2f5`，仅更新
`codex/ArkTsUI`，不并入主线。应用版本保持
**0.1.0-hmos-dev.19 /1000019**，v27 是记录编号。**完整 Flutter/Windows
对齐仍 OPEN**。本轮实际源码模型、Native、完整 API26 与包内原生核对
均限定通过；新包未签名、未安装，设备验收 **NOT_RUN**。不借 v26 的
成功结果证明本轮源码。

## 本次源码

普通 `CardView.content_kind` 从实际当前完整 `CardRecord` 的类型、format2
和完整 `tasks_v2::decode` 得出：无 Origin 为 `v2`，经完整验证的 V1 迁移
Origin 为 `legacy`。两者都是真正的当前 V2 源，普通重新编辑使用
`current_v2`；未迁移 format1、未知类型、坏 schema 或迁移证明拒绝。任务
数量、TaskId 前缀和编辑 marker 不产生分类资格，历史回执仍保留原 14 键。

实际 Index 详情重新编辑先读取最新完整卡片，再建立按该全文 source 和
revision 冻结的 source0 草稿。正文窗口保持 legacy LF todos 为空，保留
实际 TaskId、完成状态、顺序、退役身份、分类/阶段、收藏及未知字段。
元数据或 TaskId 操作后的新修订通过新读取进入正文编辑；旧请求签发后
source 再变仍按原 CAS 冲突，不自动重基。关闭后的旧 own LF 不被投影成
另一份新基线。

Index 已接当前子草稿重启恢复：从原严格确认的 Session 读取固定 handoff
计划、准确不可变 S2 父历史和真实当前完整子记录，安装同一个已暂停的
实际 writer 后再按页面 owner/input 边界恢复输入。推进的完整 S3 保留；
父已缺失或退役不伪造 active parent writer。读取不提交业务、不生成新
child/retirement/close；后续退役或关闭仍需显式核对固定原步骤。原 Session
已知结果、原 wire 与各项 Unknown 独立保留，active own continuation 仍按
原 `continued_todos` 合同处理。

普通卡片完整性门禁核实际 CardView/TaskView/AssetView 字段。发布前只读
复审发现附件字段误用 `display_name`，Root 已修为真实 `name`，并核 `kind`；
新构建须使用修后源码，旧或中间副本不证明该修正。

## 最终验证

| 检查 | 结果和范围 |
| --- | --- |
| 全量实际 ETS/tool | **903/903 PASS，0 fail/skip/cancel**，28,731.1981ms；36 个 suite 文件，125 项实际仓库模型/页面/tool/fixture 输入前后完全一致；[结果](models-final-result.json)、[日志](models-final-tests.log)、[前清单](models-final-inputs-before.json)、[后清单](models-final-inputs-after.json)。控制的生命周期/transport/render seam 不计作设备验收 |
| SDK 窄修后 Recovery 及相关模型子集 | **143/143 PASS，0 failures/skips**，11,476ms；6 个实际源码 suite、18 个模型/测试/真实 native DTO 输入前后一致；[结果](editor-business-recovery-model-sdk-retry1-result.json)、[前清单](editor-business-recovery-model-sdk-retry1-inputs-before.json)、[后清单](editor-business-recovery-model-sdk-retry1-inputs-after.json)。包含于全量，不重复累计；该子集不单独证明页面、SDK 或设备 |
| Rust 完整默认检查 | **191 library PASS /0 fail /17 conditional ignored /82.34s**，附件 binary **3 PASS /6.56s**，self-check0/doc0 exit0，整个命令 exit0；4 个本轮输入前后匹配；[日志](rust-tests-final.log)、[结果](editor-card-source-validation.json)、[清单](editor-handoff-native-inputs.json) |
| 新普通来源实际 Store 检查 | **4 PASS /1 exporter ignored**，1.94s；真实 V1 seed→Core ContentMigration、native V2、元数据/TaskId 新修订、改变正文、历史键集、reopen/retry 与 8 个拒绝情况；[原生审计](editor-card-source-native-audit.md) |
| 完整实际 Store DTO | 精确 exporter **1 PASS /0.52s**；[两类来源 fixture](editor-card-source-store-fixture.json) **84,863B**，SHA256 `409C9ADF3FA6297AA60F9EA7130B8D39C451B37516965DA48D7C2548895449FF` |
| 双 ABI 原生 | Root 新 locked/offline release **PASS 并采用**，279 项完整 native 来源；[输入和采用](native-build-inputs.json)、[ARM64 日志](native-arm64-build.log)、[x64 日志](native-x64-build.log)。归档身份见下表 |
| 最终完整 API26 SDK | fresh retry1 **SUCCESS /28.199s**，34/34 tasks 全部执行；315 复制输入、378 仓库输入前后核对 **PASS**；[日志](hap-build.log)、[复制清单](source-copy-manifest.json)、[构建输入](build-inputs.json) |
| 包内四项 Native | ARM64/x64 的 `libmorrow.so` 与 `libc++_shared.so` 字节和 SHA256 均匹配最终构建输出，**PASS**；[记录](native-package-check.json) |
| 最终 HAP | **30,267,399B**，SHA256 `059504B9BD20EB38B5DFDEC2FC75D185B41F19B86E261F0415882F6C49D26C44`；unsigned/uninstalled，设备验收 **NOT_RUN**；[artifact](artifact.json) |

| 归档 | 字节 | SHA256 |
| --- | ---: | --- |
| 新 ARM64 `libmorrow_hmos.a` | 57,055,756 | `5329F277B3F24A0AF7801FB82D5DF7D0FCDCE0FA5659BF427D3CE608B8DEEA09` |
| 新 x64 `libmorrow_hmos.a` | 55,469,756 | `EB390E7F3BF4DB73F21F3AE4148CFD76294A0D3F7B7701C673966F20FB5EEFCE` |

Native 四个本轮输入均与冻结清单匹配。冻结清单和 `-after.json` 的 SHA256
均为 `BFC532932ED3C43E06ED3924C1897CC49FCBEA7743511CDEF055A392AF76BEEB`。
v27 没有更改事务实现，因此 crash 重核 **NOT_RUN**；原 v25 的48个和 v26
的7个实际故障边界保持历史资格，不标作本轮新哈希的 crash 结果。

最终全量模型日志 SHA256
`26C31B55D54A4A385585EDA05473D47B5EFE9FE66956B8E025ACB4C8691E9A3C`。
最终 Index SHA256
`0C1620875C82C517BEA1D8C4A3029DC851452978A323667FED8AF91E483696B6`；
SDK 窄修后的 Recovery 为 **10,792B**，SHA256
`6B02C274CB3B6795FB89C8457096AA2396ED22CB6F50ACDB86A1B728B92533EC`。
最终未签名包保存在 ignored
`.build/artifacts/dev27-page-recovery-retry1/entry-default-unsigned.hap`；未安装。
见 [当前构建记录](../../build-manifest.json)。

## 保留的中间结果

首轮 SDK **BUILD FAILED /14.384s**，Recovery 四处任意捕获值直接重抛不符
API26 limited-throw 规则。Root 仅窄改为 `throw error as Error`，不改变
原固定请求/Unknown 或运行语义。首轮未生成最终 HAP；原日志、输入、复制
清单和构建/核包脚本保存在 [sdk-stage1 映射](sdk-stage1-retained.json)，
失败日志为 [sdk-stage1/hap-build.log](sdk-stage1/hap-build.log)。最终使用全新
retry1 完整副本，不能借旧副本证明修后源码。

首轮全量模型 903 项中 **902 PASS /1 fail**，失败为原旧页面文案断言，且
该测试迁移期间观察到输入 drift；[首轮结果](models-stage1-result.json)、
[首轮日志](models-stage1-tests.log)与前后清单保留，**不作为资格**。最终
125 项输入一致的 fresh 全量结果单列，不覆盖首轮失败或 drift。

窄修前 Recovery **143 PASS /11,975ms** 对应旧 **10,756B /062A7A50…**，
保留于 [原结果](editor-business-recovery-model-result.json)和
[原准备层审计](editor-business-recovery-preparation-audit-pre-sdk.md)，只作历史；
本轮最终修后身份以 SDK-retry1 子集和完整全量结果为准。

## 尚未完成

新 Native/Index 的实际设备输入、保存、接续、条件退役、关闭与进程重启
闭环，真实渲染问题复核、连续选择、系统 IME/未交付输入保全、完整富内容
提供者、权限/空间耗尽、ARM64 真机、签名、HUKS/protected storage/audit、
插件/网络/备份和完整 Flutter/Windows 产品验收仍 **OPEN**。旧 B62 UI
候选及主机 Store/模型/SDK 构建不替代新包设备资格。GitHub 推送身份和
远端确认由 Root 报告，本文不把正在准备的工作称为已经推送。
