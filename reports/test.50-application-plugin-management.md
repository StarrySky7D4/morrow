# test.50：主应用第三方插件管理

日期 2026-09-14。应用 `0.1.9-test.50+55`，核心、宿主、审计、运行时和 SDK 源码包 test.50；工作台 guest 源码仍 test.13。数据库格式 14；guest ABI2／runtime7／task3／UI1／dependency-call1 及 guest-v1-rc1 原件不变。仅主应用私有管理协议扩展并重新生成 Dart 绑定。第一方 AGPL-3.0-only，无新增 unsafe。本轮为本地开发，未推送或发布 Release。

## 完成范围

Windows 主应用新增统一风格的扩展插件组件，可预览包的摘要、能力、依赖及处理器，再明确导入。新包默认禁用且没有内容对象授权；相同摘要重新导入保留原状态。列表每页返回两条完整条目，后续页绑定首个登记修订；不将部分列表冒充完整状态。

宿主对修改校验插件身份、摘要和登记修订；同版本异包及降级在撤权前拒绝，高版本升级重新批准。保留内置工作台包身份，拒绝外部导入冒用。已选包缺失或损坏时仍能禁用、卸载；卸载保留卡片及已安装归档。批准和启用是两次写入，失败后刷新权威状态，不宣称原子回滚或自动重复提交。当前管理界面不能配置必需依赖，宿主明确拒绝启用这类包。

独立转换用无内容对象授权的临时实例执行，验证已登记处理器、输入输出类型、字节限额和真实完成结果，输出只预览。新增标准 ui.form／ui.edit 外部表单，和内置表单使用不同实例根；修改内置设置不再关闭无关外部会话。外部 UI 校验随机进程代次、会话代次、事件序号及修订，关闭成功后保留最多 64 个进程内确认，以支持同一关闭请求的显式重试而不关闭新表单。

使用与复验入口见 [插件管理](../docs/PLUGIN_APPLICATION_MANAGEMENT.md)。

## 验证结果

| 检查 | 实际结果与日志 |
| --- | --- |
| 新增宿主管理回归 | 10/10，通过预览无修改、摘要/修订冲突、版本检查、升级撤权、缺失包处理、内置与外部生命周期、关闭确认和内容保留；`build/test50-plugin-catalog-control.log` 同时含既有 9 项控制回归 |
| 完整宿主 release＋fault-injection | 全套通过，清单 115 项；`build/test50-host-full.log`、`build/test50-host-list.log`；子进程故障测试输出不能重复计数 |
| 宿主 strict Clippy | 通过；`build/test50-host-clippy.log` |
| 新增 Flutter 界面回归 | 8/8，通过真实点击与模拟后端，包括冲突不重试、完整分页、关闭失败后显式恢复；`build/test50-library-widget-tests.log` |
| 新增真实原生链路 | 2/2，无跳过；实际 Rust 宿主完成三语言原包的精确二进制转换、业务失败拒绝、重开后内容保留，以及真实导入/批准/表单编辑/关闭/卸载；编辑等待宿主响应并断言 ready、serial=1、revision=2；`build/test50-external-native-confirmed.log` |
| 既有 Flutter 回归 | 9 个文件、41/41；`build/test50-flutter-regressions.log`。本轮 Flutter 合计 51 项，不重复计算新增测试 |
| Dart 分析与绑定 | 无问题；`build/test50-dart-analysis-final.log`、`build/test50-codegen-check.log` |
| 冻结 SDK 兼容 | 14 项 Python、9 项基础原包及 3 项依赖原包检查通过；36 个固定文件、13 对原 Wasm/包，无重编译或重打包；`build/test50-sdk-compat.log` |
| 独立升级候选生成 | `tool/prepare_plugin_management_fixtures.py` 前后检查冻结清单，在 build 下生成或逐字节复核独立升级测试包；`build/test50-upgrade-fixtures-gate.log` |
| Windows Release | 最终顺序构建通过，Flutter 构建 53.1 秒；`build/test50-bundle-production.log`、`build/test50-windows-production.log` |
| 实际 Windows 自检 | 四项通过：桌面合成 API、静音 WAV 解码时钟、seek/互斥/恢复不自动播放、Rust 工作台实际渲染；`build/test50-windows-production-qualification.log` |

首次 Dart 分析发现花括号样式问题，修复后通过。原生 UI 测试先后因未等待展开动画及未等待异步宿主编辑完成失败；分别改为等候动画结束和实际控制器响应，保留 `build/test50-external-native-initial.log`、`build/test50-external-native-final.log`，最终通过日志为 confirmed。未通过降低断言或只检查本地输入框来消除失败。

最终哈希核验发现并发测试清单构建在采集后重新链接了辅助重放工具；保留原元数据 `build/test50-verification-before-production.json`，停止该并发后顺序构建正式产物并重新自检。最终五项哈希独立重算一致，Windows 随包宿主和插件与当前构建逐字节一致。

## Windows 产物与内容库检查

实际证据目录：`build\workbench-host\test50-final-962cd0e031b244cf9117693f678e2aab`。版本 `0.1.9-test.50+55`，退出码 0，活动库登记存在。

内容库格式 14，SQLite 完整性 `ok`；证据原件 6、操作引用 6、唯一块 34、块引用 144，缺块/孤块均为 0。录制归档 1、片段 10、操作根 1；容量账本 1 档、1588270 逻辑字节，与实际成本表一致。详见 `build/test50-final-verification.json`。

| 产物 | SHA-256 |
| --- | --- |
| `build\windows\x64\runner\Release\morrow_studio.exe` | `fcf4df90d95032a29871631b4e93a256a0905c2089d4525a296d2864ce127610` |
| `build\windows\x64\runner\Release\morrow-workbench-host.exe` | `85bb6c624b545dd4117c5593291fa6125ed8df56e730074a6f1acc010518e9a7` |
| `build\windows\x64\runner\Release\plugins\workbench.morrowplugin` | `14f2fc208631a606aff7ab11f472cf60f4f722068380e89a47b34d581416c4e1` |
| `build\workbench-host\release\morrow-content-replay.exe` | `ceb8db69b6b231f879db40f890e9f39e59bed0c5c3765b354c823bcdc62985d1` |
| `audit\target\release\morrow-audit-check.exe` | `26f117675ae1a0421f5c360de59ee0b7433d36218eccfce92d6f30535739007a` |

## 验证边界与下一优先项

Windows 截图只证明默认工作台真实渲染，不是外部插件表单全部交互的录像；后者由真实原生测试覆盖。磨砂检查是 API 检查，非桌面像素比较。关闭失败恢复由模拟故障测试及源码审查覆盖，尚非真实进程故障注入。上述结果是本阶段限定验证，不代表 M6/M7、全平台或完整插件系统验收。

**插件网络与通用文件系统接口仍未实现。** 本轮已补 [IO 设计](../docs/PLUGIN_IO_DESIGN.md)、[路线](../docs/FUTURE_ROADMAP.md)与 SDK 稳定范围；这些文档不是可调用实现。优先独立 io-v1 契约、C/C++/Rust 类型化接口、真实实例授权下的选中文件分块读取与可取消 HTTPS GET，再贯通录制响应和断源重放。创建/替换/删除文件及 HTTP 写请求须有持久意图、结果查询和崩溃未知状态处理，不能直接接入现有纯转换回调或向所有插件开放 WASI 权限。

完整 SDK 仍不能宣布稳定；基础 guest 兼容候选继续验证，新增 IO 独立演进。必需依赖的管理界面、长期任务、通用 UI 扩展、历史材料清理、完整依赖证据、真实旧资料迁移及各平台接入仍保留在完整任务中。本轮无网络和文件 IO 实际执行证明。
