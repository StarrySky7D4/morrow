# J-00 / G0 联合验收

本目录仅包含联合验收索引、检查工具和证据，不定义宿主 wire Schema，也不修改宿主、插件或计划原件。

实际宿主目录为 `C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor`，基线 `codex/io-safety-refactor` / `88557916aabf2e10619b1022110035178498898a`。原计划位于旧根目录下的 `Codex_Morrow_Plan_v1.1`，只读。

## 已交付内容

- `index.json`：从原计划实际解析的 31 工作包、7 门槛、84 验收和 W01–W28 映射。保留原编号、原文、文件与行号。
- `coverage.csv`：84 条验收的覆盖矩阵。新增 36 项责任按原表解析；原 48 项的工作包和门槛路由是审查推导，明确标记，不能冒充原计划新增要求。
- `check_joint.py`：Python 标准库检查器。校验数量和精确编号集合、重复、依赖引用/循环、分册与总计划逐行一致、来源定位、证据摘要、六份文件 SHA256、15 个旧契约/边界文件及实际分支/HEAD。
- `runs/<run-id>/`：每次新运行的输入快照及结果，拒绝复用已有运行目录。
- `g0-checklist.json`：G0 逐项检查与阻塞。清单验证与产品验收分开记录。
- `host-kit-002-review.md`：宿主正式 kit 的第二轮限定消费复核；因宿主报告最大批次遍历预算问题，002 暂停签收。
- `review-round2-2026-09-28.md`：第二轮结果；修订 003 最小 kit 与插件原 ready 交接的限定复核通过。
- `review-round3-2026-09-28.md`：第三轮结果；完整固定源码、依赖补丁/解析图及真实 Responses 入口的 2 拒绝场景/15 断言已独立离线构建重放通过，源码获取阻塞解除。
- `review-round4-2026-09-28.md`：第四轮；batch002 的5文件补丁/来源与生产编译证据通过，原 exe 独立复跑 LiveThread 6案12断言、Core prepared unified-exec 3案27断言通过；未独立编译Core。
- `review-round5-2026-09-28.md`：第五轮；Core网络batch003的5文件391行补丁/生产编译证据已验，原exe独立运行6案47计数断言通过，包括选定Core自然426回退与跨session状态。
- `review-round6-2026-09-28.md`：当前结果；batch004同一源码/锁/构建/exe集成三接缝，12处693行补丁与实际编译feature通过，原exe独立复跑24案109计数断言通过，未独立编译。选定入口守卫与对象释放不等于生产授权、writer释放或完整无旁路；P-02/J-00/G0仍blocked，两产品图0/2、84not_run。
- `review_p02_batch.py`：限定第三轮复核与独立 consumer 构建；仅在 joint 内写入。与不构建的索引检查器不同，本工具明确执行离线 Rust 探针构建/运行，不重复旧入口/kit测试。

## 运行

从上述实际宿主目录运行，每次使用新的 `--run-id`：

```powershell
python reports/codex-morrow-v1.1/joint/check_joint.py check --run-id review-001
python reports/codex-morrow-v1.1/joint/check_joint.py self-test --run-id negative-001
python reports/codex-morrow-v1.1/joint/check_joint.py check --run-id gate-001 --require-g0
```

`generate` 首次解析/更新本目录索引；不要以重新生成覆盖人工验收结果。退出 0 只表示目录/引用一致性通过；退出 1 表示检查错误；带 `--require-g0` 时缺失 G0 证据退出 2。所有输出限定在本目录。工具不会构建 Rust/Flutter、联网、访问个人账号或执行模型请求。

## 结论规则

`not_run` = 未执行完整验收；`blocked` = 已识别缺少前置；`verified` = 有本次范围内匹配证据；`failed` = 实际检查发现不符合。产品验收的 `verified` 需要完整当前场景证据，普通脚本、静态检查、fake 回执及历史报告不够。

G0 要求 M-00/M-01 最小 kit 加 fake，以及 P-00–P-02 的真实源码替换探针（03 分册第 69 行）。J-00 明确连接 M-01/P-02（第 77 行）。因此最小 kit、来源锁和两构建图即使分别通过，也不能关闭 G0。首轮没有任何一个 84 产品场景通过，不宣称 SDK 冻结。

本机 Windows 检查不代表 Windows 产品全流程、原生隔离、真实账号/付费路由、macOS/Linux/Android/iOS/HMOS/Web、设备掉电、签名或最终安装资格。KX-05 的故障模拟与真实设备掉电分开。

## 冻结边界

12 个 `sdk/rust/contracts/` 原件，加 `workbench_host/schemas/host.capnp`、`sdk/rust/src/protocol.rs`、`core/src/plugin_package.rs` 与指定基线对比。快照同时记录磁盘精确 SHA256、基线 Git blob SHA256 和 Git 归一化比较；字节一致不是旧 Wasm 运行回归通过。

宿主七类契约中最后一类包含 native-package 与 bundle 两个名称；不能误报为七类/八名称矛盾。最小 NativeSession/Stream/Event/Tool kit 不等于完整七类后端。插件消费只读宿主 kit，不得自建第二份权威 Schema。

## 初始检查环境

启动前已检查从磁盘根至实际工作树祖先及 reports 子树的 AGENTS.md，未发现适用文件；启动时指定工作树干净。后续快照会包含其他会话正在交付的变化，只记录、不回滚。Python 3.14.5；不读取个人 MCP/登录配置，不调用 SubagentBridge。
