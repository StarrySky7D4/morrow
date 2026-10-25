# Windows 验收暂停与开发分支快照 · 2026-10-25

用户要求“更新到github并暂停任务”。实现、构建和验收已暂停；本次只保存现场、整理报告并发布当前开发分支快照。后续执行须等待新的恢复指令。完整 P2、Windows SDK26 / G04 / SDK_FREEZE、源码整体验收和发布资格仍未获得。

开发分支为 `codex/local-continue-20261020`，提交使用 `[skip ci]`。不包含 main 合并、标签、Release 或部署。其他平台继续按用户要求暂不验收。

## 本次保留的进度

实测所绑定的完整源码摘要为 `ab14e23a14cada9752adf36c64fc1c6a13ef4effad96ab758257bdb98e1b6aee`，当时 HEAD 为 `650720269c8de658f9a1d8ae674a300e31892042`。发布前的 76 份既有修改文件逐一匹配该源码清单。本次新增报告和证据会改变完整仓库摘要，不能把发布提交称为重新实测过的完整快照。

| 范围 | 已确认结果 | 边界 |
| --- | --- | --- |
| [原 47 步／九组门槛 case078](evidence/pause-20261025/normal078-review.json) | 当前 ab14，284 项检查，107 条原始记录；正常资源退役与同 owner 权限恢复 | 不代表完整网络隔离、全部故障或产品发布通过 |
| [helper 已建、Runner 未建 case063](evidence/pause-20261025/helper063-review.json) | 358 项检查；故障处置符合要求，Unknown 保留，权限恢复 | 不能将 Unknown 改写为干净退出 |
| [真实账户创建后 setup 失败 case079](evidence/pause-20261025/setup079-review.json) | 121 项检查；两个用户和一个组实际创建并读回 SID，随后注入错误 | 专用验收变体；非自然 Win32 API 失败，无正常清理凭证 |
| [连续 17 次拒绝 case046](evidence/pause-20261025/refusal046-summary.json) | 同一原父进程完成 17 次原生拒绝，5,729 项检查 | 保留 a5 源码绑定，不冒充 ab14 的新实测 |
| [实际登录拒绝 case048](evidence/pause-20261025/refusal048-review.json) | 实际 Win32 1326、空输出及原资源退休链 | 保留 a5 源码绑定 |
| [当前原生检查](evidence/pause-20261025/native060.json)与[SDK 检查](evidence/pause-20261025/sdk061.json) | 128 项及 14 项通过 | 有界选定检查，不是宿主 legacy provisioning 全量运行 |
| [Windows P3 交接矩阵](evidence/pause-20261025/p3-matrix065.json) | 13 组要求通过；319 份证据、345 份相关源码核对 | 仅定义的 Windows 交接范围；不扩展为所有故障、其他平台或发布 |
| [目录私有协议055](evidence/pause-20261025/directory055.json)与[正式选择器057](evidence/pause-20261025/directory057.json) | C/C++/Rust、分页、原始 UTF-16、真实 Windows 选择器、正式任务和重开 | 选择时对象身份仍未闭合 |
| [正式批准、撤销与重开077](evidence/pause-20261025/approval077.json) | 三个真实客户端正常关闭；正式界面回调触发持久策略修改 | 框架回调输入，不称为物理鼠标操作；本项没有执行目录 guest |

原产品父进程 ↔ Runner 与 v8 原生关闭协议使用 Cap’n Proto。JSON 报告只记录观察，不产生权限、关闭能力或重放授权。

case078 原始摘要里的 P3 OPEN 是原 47 步模板的旧范围外字段；P3 的当前结论以独立矩阵065为准。原始证据不因更新总体进度而重写。

## case080 未到达目标检查，验收构建被拒绝

case080 `ecee6d34757543929a44afa73d27b1cd` 原计划检查真实创建成功、原 owner 捕获输出后，延迟处理跨过原期限的行为。实际只完成前三步，在第四步 setup 停止，尚未调用目标创建流程。因此 **late-create 验收仍开放**。

[原始记录](evidence/pause-20261025/case080-original-journal.jsonl)保留上一项故障标记 `MORROW_QUALIFICATION_SETUP_AFTER_OWNED_ACCOUNTS_V1`。[构建检查](evidence/pause-20261025/case080-build-input-inspection.json)显示 S 所依赖的库被 Cargo 标记为 `fresh=true`，与前一故障版本共用同一个库输出路径。当前 S 的 `setup_provisioning.rs` 与正常版本一致且没有该标记，但生成的 S 可执行文件仍含该标记。[拒绝记录](evidence/pause-20261025/case080-summary.json)保存了对应源码与文件摘要。

既有的源码摘要、产物摘要和 manifest 绑定检查不足以排除这一库缓存串用。`roles034-rejected-build.json` 是保留的历史构建结果，其中的绑定结论不能覆盖此次实测拒绝。没有在暂停后重新构建或重跑。

恢复后的首项应修正验收变体的构建隔离：隔离目标输出或显式、可验证地重编受影响 crate，核对全部 overlay 差异，拒绝非预期故障标记。之后只能新建干净用例，不能重放 case080。

## 暂停时的现场

[VM035 保存回执](evidence/pause-20261025/case080-saved-vm035.json)确认磁盘和内存状态已保存。VM ID 为 `6ed45b94-041c-4ad7-a2b3-30f954fed66f`，状态 Saved；现有四台注册 VM，其他 VM 状态未改变。

case080 的原宿主执行会话 `98753`、宿主 PID `12624` 保留，不查询、不轮询、不接管、不终止、不重放。`case_clean=false`；同一策略 owner 已撤销自己新增的唯一权限，`policy_clean=true`。权限恢复不能证明产品资源清理，也不宣称已经签发的 token 被撤销。五份固定诊断文件只读取一次，独立读取连接正常关闭；目标 late-create 诊断文件不可用，不推导其存在或完成。

VM033、VM034 已经完整校验并离线归档，保留原磁盘。历史 Unknown 的结果不变。所有六份 VM035 单次登录包已按预定阶段消费，本次不发布凭据、登录适配器、可执行文件、VM 磁盘或内存副本。

## 磁盘与剩余工作

[清理回执](evidence/pause-20261025/cleanup075.json)记录删除 14 组此前已取消注册的旧 Unknown 归档中的 41 份大型文件，实际释放 C 盘约 402.8 GiB、D 盘约 272.1 GiB。报告和配置记录保留；已删除的大型数据不可恢复。用户要求保留的“新建虚拟机”原磁盘和归档均未删除。构建缓存继续放在 `D:\build_cache`。

- P2：修复验收构建缓存隔离，补齐迟到创建、部分／异常输出、token 派生、默认 DACL 和关闭失败等所需实机证据；保留各已有结果的真实源码边界。
- Windows SDK：选择时对象身份、持久 blob／上传、网络批准与完整产品链仍开放。现有局部通过不关闭 SDK26、G04、SDK_FREEZE。
- P3：定义的 Windows 交接矩阵已通过；后续源码变化须重新核对相关证据，不继承未验证的通过结论。
- 当前暂停，未安排自动继续、构建、VM 执行或清理。

[可视化进度报告](../../reports/project-progress-2026-10-25/index.html) · [全部公开证据清单](evidence/pause-20261025/manifest.json) · [P2 故障矩阵](evidence/pause-20261025/p2-fault-matrix.json)
