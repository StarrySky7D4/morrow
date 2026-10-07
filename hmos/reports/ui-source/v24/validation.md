# v24 当前分支交付验证

2026-10-07，版本仍为 `0.1.0-hmos-dev.19` / `1000019`。仅交付 `codex/ArkTsUI`，不合并或推送主线。当前源码包括恢复初始化修复、原请求持久日志及独立 ETS 会话协调基础；**完整 Flutter/Windows 对齐仍 OPEN**。

## 变化与源码身份

- 普通字段先核 lease owner 和焦点，再判选区合法性，避免未聚焦初始化事件进入未捕获分支；真实聚焦非法选区继续保留。Index SHA256 `AFF77EB7F1C90E53DDF5D64DD89FB3A037DAB98AC0B08AE0382090A2055EFEAF`。
- Todo 每次真实初建只拥有一次完整初始显示回声能力；完整 controller/ticket/value/display/control、无实际 composing、空 preview/-1、未聚焦条件均需匹配。focus、row 更新和销毁撤销，旧 ticket 不消费当前能力，其他事件保留原处理。Todo SHA256 `83B6C6A5C5811EAD9FA14EBB23F9B5DEA7F9A858FD0B5461181091E929473836`。SDK 回调缺少 origin token，不能推断任意队列事件来源或 lossless IME 资格。
- 新独立 intent journal 持久保存完整原 Submission/publication 和附件 pin，issue 使用条件更新并持久生成精确 save/inspect literal；read 分 part 返回。prepared 可以条件取消，issued 不可取消。既有原 wire/Core CAS 规则保留，严格阻止省略已注册 intent proof 的旧入口绕过。
- `EditorBusinessSession` 使用实际 native DTO 和原 literal，提供显式 prepare/issue/save/inspect、只读恢复和固定请求重试。已签发 S1 的显式恢复不消费晚到 S2。**尚未接入 Index 主页面**；业务 source0 子草稿交接与 `saved_exact` 关闭尚未实现。

完整冻结身份见 [native 输入](editor-intent-native-inputs.json)、[native 审计](editor-intent-native-audit.md)、[Session 审计](editor-business-session-audit.md)、[独立审查](business-intent-delivery-review.md)、[字段审查](source-field-review.md)、[Todo 审查](todo-initial-input-review.md)。这些审计的先前 NOT_RUN 只适用于当时阶段，后续 Root 构建结果如下独立记录。

## 实际验证

| 验证 | 结果与范围 |
| --- | --- |
| 全部 ETS/tool 实际源码检查 | **807/807 PASS，0 fail、0 skipped，11,837.3834 ms**；[完整日志](models-final-tests.log)。包含 Index 28、Todo 109、Session 30，子集不重复累计 |
| Rust 默认完整回归 | **165 library + 3 attachment binary PASS**；11 项条件测试默认 ignored；[日志](rust-tests.log)。不是全部条件测试均已执行 |
| 新 intent 条件验证 | 24 个实际 Store 子进程崩溃边界、31MiB 双重占用与反向草稿增长配额、完整 DTO 导出各自执行通过；见 [崩溃日志](editor-intent-crash-tests.log)、[配额日志](editor-intent-byte-quota-tests.log)、[DTO 日志](editor-intent-dto-fixture-release-tests.log) |
| 新原生双 ABI | locked/offline release build **PASS**，271 个完整仓库输入构建前后及采用时一致；[输入和 archives](native-build-inputs.json)、[ARM64](native-arm64-build.log)、[x64](native-x64-build.log) |
| 独立 Session API26 编译 | **PASS，9.480 s**；完整公共接口实际引用，312 复制输入和 5 生成文件前后一致；[审计](session-sdk/sdk-audit.md)。此候选使用旧 native，仅证明 SDK 接口编译 |
| 最终完整产品 API26 | **PASS，13.893 s，34 tasks 全部执行**；313 复制输入、364 个仓库输入前后一致；[日志](hap-build.log)、[复制清单](source-copy-manifest.json)、[仓库输入](build-inputs.json) |
| 最终包内 native | ARM64/x64 的 `libmorrow.so` 和 `libc++_shared.so` 四项大小与 SHA256 均与实际构建输出匹配；[记录](native-package-check.json) |

首次完整产品隔离副本漏复制根 `hvigorfile.ts`，构建在 **13 ms /0 task** 退出。保留 [首次日志](stage1-hap-build.log)、[首次复制清单](stage1-source-copy-manifest.json)和 [首次输入](stage1-build-inputs.json)。随后只修正验证复制工具，在新 `retry1` 副本复制完整产品文件；未为通过构建修改生产逻辑。成功结果不能覆盖首次失败。

新原生 archives：

| ABI | 字节 | SHA256 |
| --- | ---: | --- |
| arm64-v8a | 56,517,238 | `E416EA7FBA13BB5DE0310942AA0C0F0FF725D6C3D7AFB37CAC33A3C5E6746C80` |
| x86_64 | 54,925,386 | `A99CF367DFAC3F518332CA36232688617E9D576BD349DC4E98494683C74DE0CA` |

最终完整产品 archive 为 ignored `.build/artifacts/dev24-intent-checkpoint/entry-default-unsigned.hap`，**29,381,667 字节**，SHA256 **`F6EEEAE2D2175A7EA54DCEF582EA1BD5D15E324D48E65583B752775B4EC8925C`**。未签名、**未安装**。详见 [artifact](artifact.json)、[当前 build-manifest](../../build-manifest.json)。它包含新 native 与 Session 源文件，但尚无主页面业务接入或新 native 的设备运行证据。

## 设备结果属于另一个准确候选

Root 已在 API26/x64 `127.0.0.1:5555` 上验证自有公开草稿 `HMOS-todos-20261007-D`。UI-only B62 候选 **29,107,139 字节 / `B62DBD231F188D60DED72AB10BEEBBB76E43694D7D67DE426729E95FF3F45B10`** 使用修复后的 Index/Todo 与**旧 v22 native**，不包含当前 intent 后端或 Session。其 API26 构建 **14.630 s /34 fresh tasks**。

- 恢复阶段 PASS：标题、正文和 `first 汉字 🧪 é.` 准确读回，待办 **13/1000**，不再出现此初建回声导致的未完整捕获提示。
- 保留关闭阶段 PASS：编辑器关闭，Home 为 **3 草稿 /14 业务卡**，已确认的完整输入保留；本轮业务提交数为 **0**。

见 [完整动作阶段](device-fixed/progress-clipboard.json)、[准确 UI 复制清单](ui-fixed/source-copy-manifest.json)、[UI artifact](ui-fixed/artifact.json)、[安装记录](device-fixed/installation.json)。安装记录内 `sourceManifest` 是先前诊断清单指针，不能作为 B62 源码身份依据；准确源身份来自 `ui-fixed` 清单及 archive/成功安装路径。绑定证明为 host archive hash、日志准确路径和 saved/fresh bundle 元数据，未取得设备 installed-byte 摘要。更早 FAILED_OR_UNKNOWN 原结果保留，没有重写或盲目重放。

## 仍开放的资格

Index 原请求恢复与保存入口、source0 业务子草稿、条件父草稿退役、`saved_exact` 完整关闭、详情变更后重开编辑、完整 TaskId 编辑对齐、新 native 多行输入/业务保存/重启设备闭环均 **OPEN**。阶段元数据 committed 不是业务 receipt；closed journal 历史不授予附件导出权限。单对象 Core CAS 不证明跨独立进程、不同身份创建时的全局配额原子性。

ARM64 真机、签名、HUKS/protected storage/audit、完整富内容/IME/手势/拒权/空间耗尽、插件与完整 Flutter/Windows 产品验收仍 OPEN。局部测试或候选恢复通过不作为整个产品完成依据。
