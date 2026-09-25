# 共享工作台与精确 64 位 Web 协议增量

本记录是完整 Win/Web 对齐目标的阶段证据。正式 Flutter Web 入口仍未接入新宿主，不能据此宣称已完成产品对齐或可安全迁移现有浏览器数据。

## 已落地

- `workbench_host` 在 Windows 与 Wasm 共用内容、草稿、插件管理和二进制协议处理。浏览器使用设备 OPFS SQLite、同一审批规则，以及共享的审计封存算法。
- `BrowserWorkbench` 接受独立选定的公钥和临时种子，校验匹配后打开本地库；关闭前封存，失败时保留所有者。没有自动替换丢失密钥，也没有云端数据库或业务上传。
- 修复共享工作台业务库链接进浏览器宿主时意外引入 guest 专用 Wasm 导入的问题。guest 入口通过默认开启的 `wasm-guest` 功能控制，宿主依赖关闭该功能；测试继续运行原工作台插件包。
- 8 组工作台生成绑定增加精确 BigInt 字段，以两个 32 位字读写，保持二进制布局。Web 的旧 int 访问遇到不可安全表示的值会报错。内容、草稿、导入决定、父子交接及提交证明编解码器使用精确字段。

## 验证

| 范围 | 结果及日志 |
| --- | --- |
| 同一工作台协议，原生与 Chrome 154 | `tool/verify_web_workbench.ps1` 通过；`build/web-workbench-verification.log` |
| 浏览器真实存储 | 创建/编辑、过期修订拒绝、分块中文长草稿及 UTF-16 选区、语言、审批、独立 Worker 重开、重复所有者拒绝、错误密钥拒绝、审计完整性 |
| Windows 审计 | keys 5、identity 6、session 6 通过；`build/audit-web-refactor-tests.log` |
| Windows 工作台 | auditing 3、content_api_protocol 1、editor_draft_protocol 7、host 2 通过；`build/workbench-browser-refactor-regression.log` |
| Windows 目录及剩余组 | plugin_catalog 10、tasks_edit 3、ui_preferences 3 通过；`build/workbench-browser-refactor-retest.log` |
| Dart 原生 | 49 项精确字段及内容/草稿/交接/导入/证明回归通过；`build/exact64-native-final.log` |
| Chrome Dart | 同一组 49 项覆盖通过。首轮 47 通过、2 失败；修复一处旧修订号比较，并让旧 int 转换明确拒绝舍入后，所属内容组 7 项全部通过；`build/exact64-browser-tests.log`、`build/exact64-browser-retest.log` |
| 生成器 | 5 项 Python 测试和绑定 `--check` 通过；未知 64 位字段默认值或生成器结构拒绝改写 |

目录测试首轮两项失败是缺少构建夹具。使用仓库既有工具生成独立升级候选及当前 SDK Rust UI 示例包后重跑通过；新示例包不作为历史 test.49 原件兼容性的证据。冻结的 SDK 原件未改写。

## 尚需完成

正式应用需统一 UI/宿主连接、把控制器修订号缓存及剩余协议调用全部迁移到 BigInt、实现本地受保护身份与旧数据迁移、浏览器文件/附件/字体/备份恢复，以及网络服务适配和发布验收。

当前浏览器工作台资格测试使用明确的固定测试身份和隔离的测试入口；生产构建不得启用 `workbench-qualification`。原生文件路径、备份及服务平台操作在浏览器尚无适配时返回明确错误。允许云端运行服务，但业务数据权威持久化继续位于设备侧。

## 共享控制器增量

内容缓存和编辑会话修订、草稿控制器、设置、审批及服务编解码已切换精确 BigInt。新增 `WorkbenchChannel`，原生适配器仍使用现有真实进程和 EOF 关闭确认；通用控制器新增显式通道连接入口，不自动创建内容库或重试失败写入。

- `build/controller-channel-native-tests.log`：15 项实际宿主测试通过；HTTP 用例当时缺少夹具跳过，未算通过。
- 补建真实 `plugins/http_forward` 后，`build/controller-channel-final-native.log`：42 项通过，包含真实 HTTP 转发/取消、关闭等待及非零退出、通用控制器和服务协议。
- `build/shared-controller-browser-tests.log` 与 `build/shared-controller-browser-retest.log`：Chrome 38 项覆盖通过。首轮共享控制器测试夹具缺少协议要求的空 operation；补齐后两项控制器测试通过。覆盖超出 2^53 的缓存与审批修订、u64 顶部的过期读取拒绝、分段帧、未知发送结果不自动重放，以及服务/HTTP/TLS 管理编解码。

新增 Worker 通道尚未完成真实端到端运行验收，也未接入正式入口。用户已明确云端提供应用母本，打开网页后直接在设备浏览器内运行，无需手动下载或安装；本机 Rust 进程不是 Web 部署前提。采用浏览器本地 Wasm 的执行位置已确认，业务数据和身份不上传云端。

## 设备身份与生产 Worker 接线

- `web/workbench/device-identity.mjs`：设备随机 Ed25519 身份、AES-GCM 封装、IndexedDB 原子新增、非导出包装密钥、身份/初始化状态认证、消费结束清理临时种子。密钥丢失、损坏、未知版本不会自动创建空库或替换身份。此保护不等同于 Windows DPAPI、硬件保护或抵御同源恶意代码；备份恢复及用户界面仍未实现。
- `web/workbench/host-worker.mjs`：只接收所选库名和明确创建意图，自己读取本地身份；异步启动与命令按序执行，先封存并核对完整性，再确认身份完成初始化。已存在的 prepared 身份仅在显式打开且原内容库存在时继续核验，不在失败后重建。
- `lib/plugins/workbench_channel_web.dart`：UI 不再接触种子，启动消息发送前挂接错误观察，避免同步发送失败变成未处理 Future 错误。
- `tool/verify_web_identity.ps1`：独立 Worker 重开、签名、并发同名创建、临时字节清理、七类损坏、中断初始化及未来数据库版本的实际浏览器验证入口。
- `tool/verify_web_channel.ps1`：不启用核心 qualification/fault features，构建实际 Worker 和共享 Flutter 控制器；用真实插件验证内容、稳定任务 ID、过期编辑拒绝、长中文草稿、设置、审批、关闭及独立 Worker 重开。

本轮 JavaScript `node --check` 与 `git diff --check` 通过。首次 Chrome 在受限环境 CDP `Page.enable` 超时；提权运行首次自动审批超时，允许的一次重试又因审批服务连接中断而未执行。Dart CLI 初始化要求写工作区外的 `.dart-tool`，格式化/分析未完成；等待中的 Flutter 包装命令已终止。上述两组新浏览器验证尚未通过，未改写旧的通过记录，正式 Web 入口也未切换。

后续用户明确批准重跑后，自动审批再次因连接中断未能放行；下一次目标续行重新核验仍得到同一失败，命令均未启动。无正在等待结果的验证进程。期间静态复查修补了启动错误原因在 UI 通道中丢失的问题，以及 Worker 释放宿主异常时无法发出退出确认的问题；仍需实际浏览器覆盖，不能以静态检查代替。

上述验证阻塞随后因审批方式更新而解除，运行结果如下。正式入口、旧数据迁移、文件能力及完整产品对齐仍需继续。

### 审批恢复后的真实验证结果

- `build/web-identity-verification.log`：Chrome 154.0.8037.57 通过随机身份创建、原子同名竞争、包装密钥不可导出、独立 Worker 重开与签名、成功/失败消费后的种子清理、中断初始化、七类损坏拒绝和未来数据库版本拒绝。测试使用全新的浏览器 profile，不修改用户已有库。
- `build/web-channel-analyze.log`：新 Web 通道及验证入口的 Dart/Flutter 分析通过，无问题。
- `build/web-channel-before-checkpoint-fix.log`：真实通道测试发现初始化调用 `finish()` 会关闭插件会话，导致工作台意外只读；这不是权限或生成绑定错误。
- 增加 `Workbench::browser_checkpoint` 与 Wasm `BrowserWorkbench.checkpoint`，只封存待处理审计记录，保持插件会话运行。生产 Worker 初始化改用该接口；正常退出保留原有关闭语义。
- `build/web-channel-verification.log`：生产 Rust/Wasm、Flutter Web release 构建及 Chrome 154 完整通道测试通过。核心不启用 `workbench-qualification` 或 `fault-injection`；真实本地身份和原工作台插件贯穿共享控制器，覆盖内容创建、V1/V2 迁移、稳定任务 ID、过期编辑拒绝、长中文/emoji 草稿及 UTF-16 选区、语言与插件审批、重复所有者拒绝、关闭及独立 Worker 重开。

该入口仍是验证程序，不是正式应用入口；没有切换旧 LocalStorage、迁移用户数据或部署网站。备份/恢复、浏览器文件适配、完整 UI 与平台服务边界仍未完成。

## 正式 Web 入口与兼容性修补

本节更新前述阶段状态：`lib/main.dart` 现在按平台选择 `bootstrap_web.dart`，Web 默认进入生产 Rust/Wasm 工作台。云端只需提供静态资源，设备上的 Worker 执行原工作台插件，OPFS 保存工作区，IndexedDB 保存本地身份。没有部署网站或迁移用户旧数据。

启动时同时检查旧 LocalStorage、设备身份和 OPFS 数据。已有旧内容提供独立打开入口；新旧内容并存时明确选择；身份缺失但 OPFS 有数据时禁止创建替代空库。失败不会切换成 MemoryStorage。完整旧库迁移与备份恢复仍待实现。

真实页面验证暴露并修复了三处问题：

- 保存按钮禁用导致 Web 焦点退回标题框并改变选区，被误认为用户产生了新草稿。按钮保留焦点，`_save` 继续阻止重复提交；真实输入或选区变化仍保留后续草稿。
- 上游 Float64 编码经 Uint64 执行默认值掩码，dart2js 不支持。生成器改用两个 32 位字处理相同线格式，保留负零、小数与非零默认值，不修改本机依赖缓存。
- Web 原本拒绝独立查询快照。现在从固定读事务复制出只读 SQLite 快照，保留源身份、读点、原始记录与查询审计校验，释放源事务后才执行查询。临时副本在设备内存中，权威数据仍在 OPFS；当前数据库快照上限为 128 MiB，超限明确失败。大库查询尚未达到 Windows 的 WAL 读取能力。

构建入口：先运行 `tool/vendor/enter_dev_environment.ps1`，再运行 `tool/build_web_workbench.ps1 -BaseHref /仓库名/`。输出 `build/web` 包含正式 Flutter 入口、Rust/Wasm、工作台插件和本地资源。脚本使用绝对输出路径并检查必要资源，避免 Flutter 清理旧相对路径产物时误删新资源。浏览器验证入口为 `tool/verify_web_app.ps1`，所有场景使用独立测试 profile。

本轮已确认的回归：

- `build/web-app-accepted-verification.log`：Chrome 154 的正式页面三组验收通过。新库在 UI 创建和保存卡片后整页刷新恢复，身份不变；旧内容单独打开和新旧库并存选择均保留旧原始字节；OPFS 有内容而身份缺失时禁止建库并保持原文件不变。输出截图为 `build/web-bootstrap-fresh.png`、`build/web-bootstrap-legacy.png`、`build/web-bootstrap-orphan.png`。
- `build/web-channel-ui-fixed.log`：生产 Worker、真实插件、共享控制器的查询与 RustStudioStorage 主题保存/读回通过，并通过原内容、草稿、审批和独立 Worker 重开场景。
- `build/web-float-native-test.log`、`build/web-float-chrome-test.log`：两平台各 15 项字段测试通过，覆盖独立线格式字节、Float64 默认掩码及精确 64 位整数。
- `build/web-snapshot-native-test.log`：有界快照复制、原始字节和成员冻结、源库继续写入、外国库拒绝通过。
- `build/card-snapshot-regression.log`：Windows 原有 18 项快照回归通过。
- `build/web-save-focus-regression.log`：20 项编辑器测试通过，保留选区/草稿、未知结果原请求重试及重复提交保护。
- `build/web-bootstrap-final-analyze.log`：正式 Web 入口、主页面及验证代码分析通过；生成绑定和九语言资源 `--check` 通过，生成器 6 项测试通过。

尚需完成：完整文件/附件/字体/媒体适配、旧库迁移、备份恢复、多页/崩溃/配额与大库验收、插件管理完整 UI、网络服务平台边界和 Pages 发布及升级/离线验收。不能将本阶段的可运行工作区视为 Windows 全功能对齐。
