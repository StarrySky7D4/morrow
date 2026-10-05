# C02–C07 源码与文档同步检查点

2026-10-05。本次按用户授权将已验证 Windows SDK 增量与当前项目说明保存到 `codex/windows-sdk-convergence-20261005`。源码基线为云端 `468ef2e912ac74e5f97f0016a8729b7d5c1f5399`，应用仍为 `0.1.9-test.58+62`。本次同步不改变已有 Release、main、tag 或原 Windows 资格分支，不运行 CI。

## 源码与资格

C02 Windows 复验修正、C03 changes discovery、C04 WebSocket payload、C05 SSE / 独立源码分发、C06 directory / blob codec 以及 C07 原 owner 目录队列随本检查点保存。最新定向结果为原 owner 115 方法、原件42和网络100；115 中已经包含 cancel7 / directory owner14 / native16。42 保留定向过滤和独立 helper 范围。五文件格式与严格 library 编译通过；整库 Clippy exit101、10既有诊断 / 0本轮 owned 诊断继续开放。

本次仅同步文档、核验源码身份并保存提交，不重跑上述测试。原327 SDK与57冻结输入保持，不重新构建、重打包或替换原插件/provider。不可把本检查点的提交或版本号当作生产产品资格、全平台通过或完整 SDK 冻结。完整门槛见[项目状态](../../docs/PROJECT_STATUS.md)和[SDK执行顺序](sdk-next-gates.md)。

## 原阶段可恢复证据

| 记录 | 身份 |
|---|---|
| C07 代码资格 | `773e36fa49013d79071a6cb9f9de4500cb74d99c5aec8042b70e39290ccdea7f` |
| C07 最终资格 JSON | `1da84f0e00016456673c836c96205f8adfeab9720f384d8dd524cf187c1e0c5b` |
| C07 两基线恢复树 | `dacac682a9e341a4931508021a1c815929f42dea`（树，不是提交） |
| 本地有界交接包 | `windows-sdk-c07-directory-owner-handoff-20261005.zip`；39,578,461 bytes；SHA256 `2bfd8fdede87fae5bcc6eed0f637d11b42b8f8df9889cee2ffe1a75fc170ea51` |

包中3,180成员的 SHA / CRC 和安全名称已独立核对；实际展开大小为170,319,311 bytes，低于256 MiB预算，从云端468与合格C06两基线恢复相同树。原封存记录、源码快照和本地 ZIP 均不因本次文档同步而改写；这些摘要仍指向封存时的原始资料。本次提交更新后的文档身份由新的提交记录确定。

## 文档维护范围

根目录中文与八种语言 README、项目状态、看板、路线、SDK使用说明和当前模块入口按同一事实更新。全部项目自有活跃文档纳入核对；仍准确的接口说明按原样保留。[文档导航](../../docs/DOCUMENTATION_INDEX.md)记录更新前1,759份Markdown的分类及维护入口。公开说明中两处实机用户路径改为通用占位，不改变其历史结果。原始阶段报告与证据记录、冻结SDK/夹具、vendor/upstream许可与归档源码保持其原身份。私有备份标识、凭据、原始操作日志、截图和演示视频不进入本次新增发布内容。

最新下载入口仍是[test.56 Windows测试预览](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.56)。test.57/test.58为源码检查点，不生成安装包。下一阶段 C08 的可信 secret factory 仅完成设计审查；picker/祖先证明、公开guest协商、blob耐久后端、生产批准链、普通用户token与各平台仍 OPEN。原失败、Clippy失败、过滤、Unknown及未运行项均保留。
