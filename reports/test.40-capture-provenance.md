# test.40：实际粘贴采用与编辑保存

日期：2026-09-13。应用 `0.1.9-test.40+45`，审计／宿主及随包清单test.40；核心test.22、运行时／SDK test.11、guest test.13，数据库格式10。第一方AGPL-3.0-only。本轮仅本地开发，未推送、打标签、发布Release或上传；Windows阶段验证已完成，以下结果为PASS_SCOPED，不代表完整插件系统验收。

## 实现范围

Windows原生编辑器打开时创建绑定目标、基准修订、完整prior摘要、包、宿主与连接的scope；真实转换返回关联票据。编辑器登记实际采用的片段和UTF-16替换范围后才修改控件，保存时传递原始编辑字段及附件别名。v2保存采用转换及必要父链，并以真实workbench命令收尾；重复票据不复制观察，未采用结果不写入最终批次。

宿主固定trim／默认描述／清单去重和附件URI改写规则，从原始编辑字段核对最终请求，再沿保留的v1规则导出唯一命令／完整Card。原件和内容同事务提交。已提交历史重试不需要旧scope，以当前权限核对原scope、快照、操作和基准修订后返回原结果。

作用域、燃料与字节容量及证明边界见[设计](../docs/PLUGIN_CAPTURE_PROVENANCE.md)。本版不增加unsafe、不访问用户库、不迁移test.1。记录不认证系统剪贴板来源或逐键编辑历史；Web／Android保持旧路径。

## 宿主全量

宿主Release＋fault-injection全量75项通过，全目标严格Clippy通过，独立枚举75项（子进程运行不重复计数）。涵盖原v1内容投影、旧证据历史重试、1100次连续内容封存、649页及近4MiB设置批次。本轮未改核心／运行时实现，未重跑其完整测试；审计仅版本调整并已编译检查。日志 `build/test40-host-full.log`、`build/test40-host-clippy.log`、`build/test40-host-list.log`。

## 已完成专项

- Rust真实SDK来源组14项通过（13个实质测试＋1个子进程入口），严格Clippy通过。覆盖plain／HTML／RTF／spreadsheet、17次转换合为18条观察的一份证据、父链／弃用筛选／重复票据、emoji边界、后续手工修改、真实附件blob与别名、跨编辑器／宿主／连接／包更新拒绝，以及历史重试。
- 内部scope到期／单scope与总容量／失败燃料记账3项通过。到期使用真实scope对象与内部指定Instant边界，未等待30分钟，不是生产时钟覆盖接口；容量按同生产方法记账，不是RSS测量。
- 分块传输2项通过，包括超过32KiB的emoji元数据、成功关闭后的相同保存重试、传输关联身份错误和尾随字节拒绝；拒绝时未发布内容。
- 独立CLI4项通过，包括原v1新建编辑及v2实际RTF→plain父链编辑。删除本测试合成源DB／安装目录／凭据后，固定原Commit与Evidence摘要仍可实际重放。错pin返回1；内部合法但实际fuel观察不符返回2，未把重新构造合法事实冒充原Commit。

### 提交失败的真实验证

测试仅在临时库安装自身SQLite trigger。语句ABORT测试提交前失败；可延迟外键违约使COMMIT本身实际失败，返回CommitUnknown。两种情况下检查cards／operations／outbox／evidence／refs／chunks均无残留；scope锁定原操作及输入，拒绝不同操作、改快照、新capture与新paste。移除自有trigger后同scope原操作恢复，原capture调用／结果一致，重开后的历史容器逐字节一致。

另用after-commit故障点让真实子进程退出86，重开时不具备旧内存scope，仍从已提交历史确认原结果和原件；新操作及撤权后重试拒绝。此测试没有全局测试进程环境变量竞争。

全量初次编译碰到新增测试夹具使用unwrap_err而Record没有Debug；改为取err后重跑。初始错误保存在 `build/test40-host-initial-fixture-error.log`，没有为测试增加生产Debug接口。

## Flutter与恢复修复

最终35项回归全部通过：新增编辑器10项、既有生产宿主集成5项、富内容／功能20项。`flutter analyze --no-pub`零诊断，Cap’n Proto生成绑定`--check`通过。日志 `build/test40-final-flutter.log`、`build/test40-flutter-analyze.log`、`build/test40-generated-check.log`。

实际Flutter客户端与生产Rust宿主验证票据→emoji选区→粘贴登记→人工改写→保存；覆盖NBSP／BOM／NEL的trim、清单顺序、附件URI编码和空白边界、RTF父链与inputPlainText选择、相同操作历史重试和不同意图拒绝。真实NewIdeaDialog完成快捷键粘贴和保存；输入来自受控ClipboardReader及合成TSV，不宣称实际Office应用剪贴板验收。

独立审查发现并修复三处恢复问题：附件导入临时源不再充当已提交内容缓存，首次物化从核心导出独立文件；缓存缺失时重新导出，完整物化／整批load成功后才推进修订；保存请求发出前的确定准备错误允许解冻并改正草稿。提交已发出后的未知结果继续冻结原请求。关闭编辑器重新读取权威内容，读取失败明确提示状态未确认。

真实恢复测试用本测试合成文件占据preview目录，使实际Edit已提交而附件export失败：旧表单修订不能获得新scope；恢复目录、刷新后显示已提交正文并重新导出原blob。另验证删除导入源不破坏独立导出文件，超64KiB请求在未提交前失败后可缩短并用同scope成功保存。旧通用apply按asset保存有限原路径别名字符串，与读取缓存分离；v2只使用本次snapshot别名，避免隐式历史事实影响独立投影。

验证过程中对话框测试在已成功保存后跨FakeAsync／runAsync队列等待而超时，阶段日志确认卡在后续load；调整测试为同一测试zone发请求并有界pumpHost驱动后通过，没有增加超时掩盖问题。保留 `build/test40-native-dialog-diagnostic.log`、`build/test40-editor-complete-timeout.log`。缓存分离曾使旧原路径别名回归失败，修复并重跑35项，初始证据 `build/test40-existing-native-alias-failure.log`。

## 最终验证与Windows产物

生产bundle构建通过，Windows Release编译通过（73.2秒）。实际应用ProductVersion为 `0.1.9-test.40+45`，新建管理库自检退出0，活动库登记存在。四项实际检查通过：Rust工作台渲染无Flutter错误、背景blur 0／1／12／40 API及关闭、静音WAV解码与播放时钟、跳转／互斥及恢复不自动播放。背景检查不是桌面像素比较；本轮未进行Android／Web／macOS／Linux／iOS运行验收。

合成自检目录 `build/workbench-host/test40-final-f973fd2be2d8430595e084e638081c47`。只读SQLite完整性ok，格式10，6份证据原件／6次引用，34个独特块／144个有序块引用，无缺块或孤立块；配方5670字节、块容器1023143字节、数据库1249280字节。该统计来自应用自检样本，不是17次capture专项样本。

构建日志 `build/test40-bundle-final.log`、`build/test40-windows-build.log`；完整路径、哈希、实际退出和数据库检查存于 `build/test40-final-verification.json`。实际Release内宿主及插件包哈希与冻结后的生产bundle一致。

| 产物 | SHA-256 |
| --- | --- |
| `build/windows/x64/runner/Release/morrow_studio.exe` | `f235491f24ecb0be2e8b51d02b690f637906f68b3ea7a508f030a2a4bed8af4d` |
| `build/windows/x64/runner/Release/morrow-workbench-host.exe` | `3cfa335c8628d97a185f1793e67f45e614cf98f562d3e5258487a84c0c52e279` |
| `build/windows/x64/runner/Release/plugins/workbench.morrowplugin` | `13e6565dc73d4d4a5137da5c24931ee5a26023402ff4282047e94e4f9dfd27e1` |
| `build/workbench-host/release/morrow-content-replay.exe` | `05613b36983a68165abe01ea4a846066ea572c03d7cb0f41685838480e51abe2` |

运行应用需保留整个Release目录。独立CLI仍位于workbench-host/release目录，本轮未创建发布附件或上传。

三名子代理分别实现Rust scope／v2及恢复审查、Flutter编辑器接入与真实宿主验证、独立转换／提交故障矩阵。主代理负责传输协议、真实CLI离线测试、宿主全量、Windows构建／实际应用检查与阶段文档。

## 后续

继续推进查询证据、签名提交之间的连链、设置及其他通用宿主投影、依赖图与响应记录、全局保留／容量／GC、未提交意图恢复与检查点、六平台运行验收。完整M3–M7及0.2.0退出门槛保持未完成。
