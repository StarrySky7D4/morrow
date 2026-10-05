# dev.8 查询接入与版本跟进验证

日期：2026-10-05。版本 `0.1.0-hmos-dev.8` / `1000008`，独立 HMOS 开发预览；本轮结果不关闭与 Windows 的完整功能对齐目标。

## 最终包与来源

- HAP：`entry/build/default/outputs/default/entry-default-unsigned.hap`，22,296,875 字节；SHA-256 `F0DD0A031358E383E099ED5E6A97901907CD550425A7CD13C65EA46708A19BD8`。未签名，包含 ARM64 与 x64 原生库。
- 构建 SDK 保持 API26 / 26.0.0.105。最终包在现有第二实例 `Pura X View2` / `127.0.0.1:5557` 安装成功，设备实际软件版本 `7.0.0.106`；`bundle-installed.json` 确认 dev.8 / `1000008`。
- 原跟进工作树为 `925fb8ca` / test.57；较新 Windows 汇合为 `77246617` / test.58，SDK 工作树为 `20669f67`。见 `upstream-audit.md`。这些工作树仍在推进，报告按观察记录，不继承 Windows 测试数字。
- 冻结 `shared/reference.json` 的全部原始文件哈希保持；相对当前参照存在 50 个变化、72 个新增路径。本轮未全量覆盖核心快照。
- 原 `workbench_host/src/query_plan_v2.rs` 按字节复制，SHA-256 `D88DC19EFE7A07C3F9D62DD7995C6E78E81BBDCA7595C585FA0C3FAA13E885E9`；来源单独固定在 `rust/query-plan-reference.json`。`query_v2` / codec / cards_v2 / tasks_v2 与三个参照工作树逐字节相同。
- 构建清单已更新为 dev.8；263 个输入的磁盘与暂存区原始字节全部匹配，HAP 大小/哈希/应用版本匹配。可用 `tool/verify-build-manifest.cjs --staged` 复核；生成库与 HAP 保存在本地构建目录，不把它们当作 Git 源码。

## 实现范围

Rust `query` 只读请求从单个完整 WAL 快照读取原始卡片属性，核对发现列表、完整 EOF、关闭快照，复用原计划的分批筛选、排序与归并及真实 128 项 / 64 KiB 编码帧预算。搜索包含标题、描述、假设、结论和附件名称；阶段、待办、收藏及附件类型语义沿用原模块。

查询返回 ID 列表，不返回重复卡片投影，不申请 grants，不写操作回执或数据库。正常卡片响应继续保留完整源记录、CAS 与原操作重试协议。默认顺序按源计划反转 ID 列表，未新增创建时间承诺；标题顺序采用 UTF-16，收藏优先保持稳定。

ArkTS 复用 Flutter 查询协调策略：文本 200ms 去抖、合并待发条件、单一串行请求通路，已发请求不取消；过期成功与失败均丢弃。新页面/搜索/筛选/排序不显示旧结果；同视图内容刷新保留最后确认的成员并更新卡片修订。加载、失败和真实空结果区分，失败可显式重试。回收站保留独立撤销视图，不伪装为正常 query_v2。

页面返回等待查询结果后恢复缓存位置，并保留“结果未到时重复点击当前页”的恢复意图。此边界通过独立代码审查修正；没有将快速设备查询当成可控延迟时序测试。

## 已执行验证

| 范围 | 结果与证据 |
|---|---|
| Windows 上实际 Rust Engine | `--locked --offline --all-targets`：12 passed / 0 failed，见 `rust-host-test.log`。包含原 5 项与新增 7 项，验证完整属性及附件元数据搜索、阶段/任务/删除、UTF-16/同名稳定顺序、150 张跨批归并、字节预算分批、单卡超预算、空/非法条件、256 容量和生产路径拒绝；前后 readpoint 不变 |
| 实际纯 ArkTS 模型 | SDK TypeScript 编译运行：查询协调 5 项 + 卡片数据源 5 项，10 passed / 0 failed，见 `arkts-model-test.log`。覆盖旧回包/旧失败丢弃、待发条件合并、去抖、同视图刷新、显式重试、失效/销毁、条件快照所有权与位置缓存通知；不等于 ArkUI 渲染 |
| OHOS 构建 | ARM64 和 x64 release 库、两架构 runner 编译成功，见 `rust-ohos-build.log`、`rust-ohos-x64-build.log`；最终 HAP 成功，见 `hap-build.log`。保留原 `Function.bind` SDK 警告，未引入签名资格 |
| 实际 OHOS x64 原生程序 | 在健康第二实例的新隔离 fixture 目录执行，13 项 PASS，见 `healthy-native-self-check.log`；包含 TaskId/CAS/历史回执/重启和新增完整属性查询、阶段、已完成待办过滤。没有读取应用或桌面正式库 |
| 最终包 440 vp 查询 UI | 8 项 PASS，见 `query-device/result.json`：仅假设搜索、仅结论搜索、同 ID 收藏修订重绘、收藏查询、实验室查询、空结果移除旧卡片、恢复结果及进程重启后查询。fixture 通过可见编辑器创建，没有直接写数据库；最终重启截图为 `query-device/reopened-query.png` |
| 最终包 440 vp 单列 | 4 张长短交错 fixture 经可见编辑器创建，标题排序、列数、14 vp 间距/无重叠、全部记录可达和重排后的身份检查 PASS，见 `layout-final/portrait-result.json` 与竖屏截图 |
| 最终包 744 vp 两列 | 5 项 PASS，并断言长短首行顶部对齐、第三张填较短列且相距 42 px / 14 vp。见 `layout-final/landscape-result.json` 与横屏截图；4 张记录全部可达，重排 ID 对应关系正确 |

第一次在原实例执行原生自检也通过；但其系统权限数据库及备份损坏，应用覆盖安装失败，仍为 dev.7。没有清理或修复其数据库，没有卸载或重启该实例。环境恢复、健康第二实例、输入法基本模式和诊断边界见 `emulator-environment.md`。旧实例或旧包截图不作为本轮 UI 验收。

横屏诊断初次断言在第三张卡片的标题尚未进入可见区域时失败：可见节点已经有该卡片身份，标题被视口裁剪。后续小步滚动的起点落入固定页脚，列表位置未变化；工具改为从可见卡片内、页脚上方开始，并确认位置实际移动。另一次诊断因截图路径已存在而停止；最终采用独立 `layout-final/` 保存通过结果，防止诊断图混入验收。应用源码没有为这些工具诊断修改。

## 仍开放的边界

开发库仍限定 256 张，UI 仍整体持有卡片投影；本轮的查询内部分批不是无限大库或 UI 响应分页资格。附件测试只构造合法元数据，没有导入、访问或物化附件文件。当前为 `development-unsealed` 本地计算，没有生产 guest 执行、捕获、审计、owner 或 HUKS 资格。

未完成正式存储会话、持久草稿/跨进程 Unknown、附件与剪贴板、插件运行/文件/网络任务、音乐和歌词。未重测全部七风格、主题/语言/字体/键盘矩阵、内存/帧时和 ARM64 真机；旧 dev.4–7 证据保留各自范围。只同步 `codex/ArkTsUI`，不合并或推送 `main`。
