# test.39：版本化宿主内容投影

日期：2026-09-13。应用 `0.1.9-test.39+44`，审计／工作台宿主 test.39，内容库格式10；核心crate仍为test.22，运行时／SDK test.11，默认工作台guest test.13，随包清单test.39。第一方AGPL-3.0-only。本轮仅本地开发与提交，未推送、打标签、发布Release或上传。

## 已完成的内容路径

默认Windows create／apply保存完整历史CardRecord、操作／目标、宿主附件元数据、观察时刻及Restore撤销事实。保留一次真实插件捕获，封装为schema2单页批次；宿主实际提交的命令唯一来自固定v1投影，不再在提交处另行拼一套正文。所有动作沿原行为处理未知字段、Create空预览、apply的UTF-8预览截断、附件映射及撤销窗口。

新增预编译Protobuf `HostProjection`，作为 `morrow.workbench.content-projection.v1` 原意图保存。`derive`在无Store／凭据的情况下重建原命令和完整结果卡片；`verify_commit`核对命令原字节、摘要、操作／事件／目标、修订、完整结果卡SHA、有序附件SHA及唯一证据引用。历史重试核对原投影与原用户意图，用当前权限交付原结果，保留旧schema1重试能力。

这是捕获来源到最终创建之间的必要基础，尚未记录capture来源及后续用户编辑关系。设置继续用test.38专用批次，未套用本轮完整卡片投影。版本v1要求后续语义或依赖编码变动保留旧实现并新增版本；不承诺任意未来依赖组合都复现旧字节。详情见[内容投影设计](../docs/PLUGIN_CONTENT_PROJECTION.md)。

## 容量与迁移

完整原卡片原本允许8MiB，旧4MiB批次意图不能容纳所有合法原卡。意图上限因此增至12MiB，保留原卡未知字段；批次原PB24MiB、每操作16份／64MiB总量保持不变，设置自身仍为4MiB。数据库升级格式10，格式9继续拒绝意图超过4MiB的批次，不能把新容量冒充旧格式兼容。

9→10在同一事务校验旧状态、更新版本并复验；旧格式4～9按阶段迁移，只读接受5～10。原证据、Commit、签名和32KiB物理块不重编码。不新增unsafe，不访问用户资料库，不宣称test.1数据迁移已完成。

## 实际验证

| 范围 | 本轮结果 |
| --- | --- |
| 核心完整Release＋fault-injection | 254项通过；全目标严格Clippy通过；清单254项 |
| 审计完整dev＋fault-injection | 57项通过；全目标严格Clippy通过；清单57项 |
| 宿主完整Release＋fault-injection | 55项通过；全目标严格Clippy通过；清单55项，子进程不重复计数 |
| 运行时纯观察／CLI相关专项 | 18＋6＝24项通过；全目标严格Clippy通过，未重跑运行时所有测试 |
| Flutter与生产Rust宿主 | 5项集成通过 |
| 核心Wasm | 普通及web-storage两种lib配置编译通过，未代替浏览器运行验收 |
| Windows | 生产插件包构建、Release编译和实际应用自检通过 |

日志：`build/test39-core-full.log`、`build/test39-audit-full.log`、`build/test39-host-full.log`及对应`-clippy.log`；独立枚举`build/test39-{core,audit,workbench_host}-list.log`。专项`build/test39-runtime-replay.log`，最终运行时严格检查`build/test39-runtime-clippy.log`。集成与构建记录`build/test39-flutter-integration.log`、`build/test39-bundle.log`、`build/test39-windows-build.log`。未将dev配置写成Release。

### 核心迁移与大原卡

共享证据存储组15项（14实质场景＋1子进程入口）及批次格式8项通过，均已包含于核心全量。新增保存4MiB旧意图的签名库，验证格式9只读保持原版本，迁移后原容器／原PB／Commit／签名／块不变；超过8MiB的合成意图在格式10可完整往返，放入格式9后只读和可写入口均明确拒绝。9→10提交前／后实际子进程退出86，分别留下完整9／10；重开不改变历史原件。

真实SDK内容投影8项通过，覆盖全部Create／Edit／Favorite／Todo／Stage／ToProject／Delete／Restore。大原卡使用8MiB减8192字节的不可压缩噪声作为外层未知字段，并包含正文未知字段；实际guest编辑后，证据中的prior与原Card逐字节相同，结果完整保留未知数据。另验证预览按UTF-8边界截为16383字节，未切开汉字。

附件实测使用真实合成blob和宿主映射，核对有序metadata及SHA；Restore分别检查宿主undo修订／deadline和deleted_at的8秒业务窗口。测试区分内部矛盾与自洽替换：某个合法替代哈希不必被derive认作伪造，但必须与原Commit不匹配，不能假装摘要本身证明来源。

独立动作资格完成后删除合成源库及原安装目录，9份历史任务均可隔离回放并精确重建命令／结果。旧schema1实际观察在独立受审计库提交后，可由新宿主历史重试，仍明确拒绝当作拥有完整prior的投影。原committed_evidence的11项历史／崩溃测试继续通过。全量中的649页与近4MiB设置回归也通过。

### 独立内容回放CLI

`morrow-content-replay <commit-file> <commit-container-sha256> <evidence-file> <raw-evidence-sha256>` 先有界读取、固定外部摘要及校验投影，再用默认逐页Limits与固定1B总fuel实际回放。匹配退出0、拒绝退出1、实际观察不匹配退出2。两种摘要分别针对Commit完整容器与Evidence原始PB，不能混用。

新增3项实际进程测试通过：先关闭并删除合成源DB／catalog／凭据，再验证新建和编辑；错pin及自洽但不同的最终内容拒绝；对燃料观察作合法但不符合实际执行的更改，重新绑定合法Commit，返回不匹配2。此组验证流程，未用手工PASS替代实际进程。专项日志`build/test39-content-replay-cli.log`及`build/test39-content-replay-cli-clippy.log`，也已纳入55项宿主全量。

CLI不打开内容库或取回授权，输出不含正文；外部pin固定输入，不认证作者。签名信任、附件原来源、blob可取回性、外部时钟真实性及prior与前一签名提交的完整连链仍需独立证据。

三名子代理分别完成版本化投影及文档、默认内容提交／历史重试集成与独立审查、真实插件／兼容性／损坏测试。主代理负责DB10及容量门禁、迁移测试、独立CLI及进程测试、全量验证、版本和Windows产物。所有数据均为临时或build下合成样本。

## 最终Windows产物

实际应用版本 `0.1.9-test.39+44`，退出0，活动库登记存在。自检目录 `build/workbench-host/test39-final-c59fe9a2b867463db07fbed7743410aa`；哈希及数据库统计保存在`build/test39-final-verification.json`。

实际Release通过Rust工作台渲染、背景blur 0／1／12／40 API与关闭、静音WAV解码及时钟、跳转／互斥和恢复不自动播放。背景项是API验证，没有桌面像素比较。

自检后只读SQLite完整性ok、格式10，6份原件／6次操作引用，34个独特块／144个有序块引用，无缺块或孤立块。配方5670字节、块容器1023148字节，完整数据库1249280字节。该统计来自应用自检合成样本，不是大原卡专项样本。

| 产物 | SHA-256 |
| --- | --- |
| `build/windows/x64/runner/Release/morrow_studio.exe` | `9fb198c57d0f670a75e2a72e9cf19295d2bc94977490538449ec04a08f14044d` |
| `build/windows/x64/runner/Release/morrow-workbench-host.exe` | `ca68bdf3a20f41a8d5f1dc552c4bb68f6ba0d417ce25c49341d8d35647607318` |
| `build/windows/x64/runner/Release/plugins/workbench.morrowplugin` | `ca67654631e3403888a8660e6b9277572bc8ed09ad3c1613794854a327c9827c` |
| `build/workbench-host/release/morrow-content-replay.exe` | `f030f3145b6d2129195159da319663a1f314b4c011ea44d07f2298676c8c1789` |

应用运行须保留完整Release目录。独立内容回放工具位于上表的workbench-host/release目录，不声称它已发布或加入GitHub附件。

## 继续推进

下一步补齐capture来源与编辑后的create关联、查询证据、前后提交间的因果校验，并扩展设置／其他宿主投影、依赖图与响应证据、全局容量／保留／GC、未提交意图及异步恢复、检查点和六平台运行验收。完整M3–M7与0.2.0退出门槛保持未完成；本阶段为Windows版本化内容投影PASS_SCOPED。
