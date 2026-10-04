# GitHub SDK 进度核对与 Windows 后续任务

核对日期：2026-10-04（Asia/Shanghai）。本记录核对 GitHub 提交、分支关系、五份云端阶段报告和本地源码状态，更新进度文档。没有新增构建、运行测试、修改生产源码或执行 CI；下列运行结果分别归属于对应的既有验证阶段。

## 当前分支与发布状态

| 开发线 | 核对到的提交 | 当前关系与含义 |
| --- | --- | --- |
| Windows `codex/windows-sdk-qualification-20261003` | [`513811bc`](https://github.com/StarrySky7D4/morrow/commit/513811bc172af570c41cb39a0f2b0f63e8318b3b) | 本轮开始时本地与远端一致，工作区干净；本轮进度文档以此为父提交 |
| 云端 `codex/cloud-sdk-convergence-20261004` | [`468ef2e9`](https://github.com/StarrySky7D4/morrow/commit/468ef2e912ac74e5f97f0016a8729b7d5c1f5399) | 直接继承 Windows 检查点，领先 5 个提交、落后 0，共 69 项文件差异；生产源码尚未应用到本地 Windows 工作区 |
| 原 Linux `codex/linux-sdk-reconstruction` | `63f38d4a` | 原 stage15 基线，最新云端推进在新的汇合分支上 |
| `main` | `adccf2a6` | 最近提交是 2026-09-29 的 CI 证据日期文档修正；未包含此次 SDK 云端汇合 |
| HarmonyOS `codex/ArkTsUI` | `7adcdce6` | 保持独立开发线；不能替代 Windows、Linux 或其它平台的 SDK 资格 |

核对时源码版本为 `0.1.9-test.58+62`，最新已发布的应用测试预览仍是 [v0.1.9-test.56](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.56)。源码版本与公开下载包不同。GitHub 当前没有打开的 Pull Request；本轮不创建新的应用版本、标签或 Release。

## 云端新增的五个阶段

下表为**已读取的云端阶段报告**，不是本轮在 Windows 上重新执行的结果。阶段之间包含回归和子集，计数不能相加为整个 SDK 的通过总数或完成率。

| 阶段与提交 | 已交付的源码能力 | 报告中的限定验证 | 仍需完成 |
| --- | --- | --- | --- |
| LWS01，`5d9e9f2` | 修补 WebSocket 开启、续租、取消、Close 原因与饱和发送的生命周期；新增可复现的 Rust／C／C++ 测试夹具 | Linux 网络 96 个方法、原可移植 runtime 49 个方法、夹具准备 6 个方法；宿主测试为 Debug，新 guest 为 Release | 当前 Windows 执行、受保护／生产 owner、WSS／真实外部端点、GUI |
| D01，`eba50eea` | 原只读 SDK 发现描述符补充 IO、service、service-resources、mutation 的身份、操作、限制、路由和前置条件；保持旧描述符兼容 | Rust 7、Python 20、原 channel tooling 6；实际旧／新宿主与消费者四种组合 | 发现信息不授予权限、不激活后端；Windows 与生产路由资格 |
| P01，`8ba1aa76` | `morrow_plugin.py check PACKAGE --host HOST` 先发现选定可信宿主的诊断能力，再进行静态包准备；旧宿主保守拒绝新入口 | Linux CLI 8、SDK 单元 8（包含 D01 的 7）、Python 150；命令矩阵另列，不追加为测试方法 | 静态准备不证明 guest 执行、授权、安装或可用业务路由；Windows 管道与进程清理复验 |
| X01，`ffbc30b7` | 独立六文件诊断源码包，复用原解析器与验证逻辑，可在仓库外使用显式可信宿主查询 profiles／preflight | Python 176 为一次去重运行，包含 P01 的 150 与新增 26；真实准备矩阵为 20 个 prepared、7 个预期拒绝 | 分发／准备不等于整个 SDK 冻结或任意原生宿主隔离；Windows 独立使用资格 |
| G06，`468ef2e9` | 新独立 `changes-metadata-v1`，固定卡片集合、有限窗口、显式批准、严格 codec、原 Store／durable ACK，以及独立 Rust／C／C++ 扩展 | Core 集成 18；Core 单元 76 通过／1 忽略；runtime 注入构建 32、生产构建 30（为前者子集）；既有 network 96 复验；共享向量每种语言 209 个判定 | Windows、Workbench 目录／GUI接线、完整 changes／cursor／长期 watch、公开 protected owner 与完整同步 |

阶段报告固定在所核对云端提交：

- [WebSocket 实现与 Linux 资格](https://github.com/StarrySky7D4/morrow/blob/468ef2e912ac74e5f97f0016a8729b7d5c1f5399/reports/reconstruction-2026-10-04/cloud-websocket-validation.md)
- [SDK 发现](https://github.com/StarrySky7D4/morrow/blob/468ef2e912ac74e5f97f0016a8729b7d5c1f5399/reports/reconstruction-2026-10-04/sdk-discovery-validation.md)
- [选定宿主静态预检](https://github.com/StarrySky7D4/morrow/blob/468ef2e912ac74e5f97f0016a8729b7d5c1f5399/reports/reconstruction-2026-10-04/sdk-preflight-validation.md)
- [独立诊断工具分发](https://github.com/StarrySky7D4/morrow/blob/468ef2e912ac74e5f97f0016a8729b7d5c1f5399/reports/reconstruction-2026-10-04/sdk-standalone-diagnostics-validation.md)
- [有限卡片变更元数据源](https://github.com/StarrySky7D4/morrow/blob/468ef2e912ac74e5f97f0016a8729b7d5c1f5399/reports/reconstruction-2026-10-04/changes-metadata-validation.md)

## Windows 现有证据与资格边界

[C01 原件复验](windows-convergence-recheck.md)仍是现有 Windows 检查点的限定证据：dependency 3、base 9、映射 7、共享对象 14、owned reader 9，共 42 个唯一方法通过。SDK327 与冻结57原件保持不变。此结果绑定 C01 验证源码，不能直接转为五个新云端提交的 Windows 通过记录。

云端差异未改写旧 SDK 目录、旧 `.capnp`／`.proto` 合同或已有依赖锁；新 metadata 扩展有自己的规范与锁。源码身份不变仍不足以替代平台、运行期权限、业务结果和生命周期资格。

旧 Windows 分支中的“WebSocket 草稿未验证”继续描述该分支；云端 WebSocket 已有 Linux 限定实证，不再列为全项目完全未实现。SDK 发现、静态预检、独立诊断已在云端实现；G06 从完全缺少授权内容变更 source 推进到显式批准的有限元数据切片。长期 watch、完整 cursor／retention、原窗口崩溃恢复、正文／附件同步及生产路由仍未闭合。

受保护 Storage／IoWorker／DPAPI 的 9 项、当前新增能力的普通用户 token、生产 owner／catalog／GUI、多平台与完整 26 需求／10 组冻结门槛继续 OPEN／NOT_RUN。云端报告与当前平台资格分开，历史失败、忽略与未覆盖项不升级。旧丢失 stage16 不因本轮进度核对而恢复。

## 更新后的推进顺序

| 优先级 | 下一项工作 | 完成条件 |
| --- | --- | --- |
| P0 | 以精确云端 `468ef2e9` 建立独立 Windows 验证候选 | 保留当前工作区；确认 5 个提交／69 项差异、SDK327／冻结57／旧合同与锁；只有验证合格的切片才作为 Windows 当前资格 |
| P0 | WebSocket 生命周期、真实三语言新夹具与原件回归 | 实际 Windows 开启／续租／撤权／期限／饱和取消、Receive／ACK／Close／双方 join；原 dependency／base／映射／对象／reader 重新绑定新候选 |
| P0 | 选定宿主发现、静态预检与独立诊断 | Windows 实编宿主、旧新消费者兼容、结构化拒绝、输出／管道期限与清理；不把 prepared 当作执行或安装成功 |
| P0 | 新 metadata 扩展的 Windows 普通合成数据验证 | 先审查测试数据及 owner 路径；验证过滤、批准交集、总额度、精确 ACK、重开与终态；受保护数据仍遵守独立授权边界 |
| P1 | 原 committed-tool bounded runner、生产 channel／catalog／GUI 与 owner | 以准确已提交工具身份运行，分别记录产物、真实执行、权限与清理；原 SDK014 历史失败不能由库测试替代 |
| P1 | 完整网络、文件与异步组合 | Account／OAuth／WSS／真实 API，directory／blob／可靠条件 Replace，异步组合与跨重启 Unknown 的逐能力核对；保持不重放与显式 Unsupported |
| P2 | 全平台矩阵与完整冻结 | OS×arch×profile 的实际存储、映射、权限、生命周期与 UI 资格；第三方接入独立验收；CCswitch／Codex产品集成继续后置 |

本次取回云端 Git 历史并同步公开进度文档，未将云端生产改动合入本地 Windows 源码。后续源码合入与 Windows 复验按上述顺序推进；本报告自身不关闭任何运行资格或冻结门槛。
