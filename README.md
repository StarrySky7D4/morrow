# Morrow · 明隙

<!-- C28-CURRENT-BEGIN -->
## 当前开发检查点（2026-10-09，C28 阶段 18）

开发分支同步启动防重放修正、收据解析及保留回归、固定 Wasm 测试夹具路径，并保存自主捕获工具与禁用的后继控制器源码。应用版本保持 `0.1.9-test.58+62`。

为避免公开本机路径，本次不附带原 session Wasm；固定夹具目录说明了另行准备和复验的前提。公开 Git 树仍缺此输入，未完成独立完整构建；本地完整载体的编译结果不能替代公开树验收。

Windows 原根库与程序已离线编译，Cargo 退出 0、双 EOF 完整；外层因新 Git checkout 的后置字节守卫失败退出 1，完整构建资格尚未通过。保存产物的来源与哈希已独立读回，未执行程序、测试或 VM。阶段 16 的 20 项合成回归、阶段 17 的 7 项读取测试与两轮桥接保留各自范围，不作为新生产载体复验通过。

原生 Start 的 Unknown 不重放；owner finish、factory release、cleanup/join、真实断连及 Windows 沙箱资格仍待验收。旧 runner/setup 依赖源码与当前载体不完全一致，三程序 manifest 仍 PENDING。后续 11 项原始库测试未运行；额外 dev 依赖未补齐。`SDK26_G04=OPEN`，`release_eligible=false`，SDK 未冻结。

下一步先核验 Cargo 对新 checkout 的转换及 helper 匹配来源，再准备有明确范围的 Windows 生命周期复验。会话层与安全执行层验收后暂停准备测试预览，不等待扩展执行层。见 [阶段 18 记录](reports/reconstruction-2026-10-09/windows-agent-sdk-c28-stage18.md)与[捕获工具源码](companions/morrow-codex/qualification/c28-autonomous-capture/README.md)。下方内容保留历史时点。
<!-- C28-CURRENT-END -->

<!-- C16-CURRENT-BEGIN -->
2026-10-06 C16 本地实验性候选：新增可信 Rust Agent start/submit/poll/cancel/recover/acknowledge
入口，移动完整原 owner，沿用同 Core/Store/runtime/连接；每 import 维护原 owner。
借用执行与 scheduler 回收有界；真实 join 后仅同步唯一 Runtime 所有权可回收，别名保留 16 有界债务。
普通无沙箱、非 ProtectedSession 的封存 Rust Wasm＋实际 Windows worker 耦合 3 项通过；各组收据见报告，
不作为生产 GUI、真实受保护库/DPAPI 或生产沙箱资格。旧 SDK327/57、合同、Linux 与 C15 工件守卫通过。
接口仍 experimental，SDK26/G04 OPEN；未提交、推送、Release 或 CI，不代表正式 SDK 冻结。
见 [原生 owner 接口](docs/PLUGIN_AGENT_NATIVE_OWNER.md) 与 [C16 报告](reports/reconstruction-2026-10-06/codex-sdk-c16.md)；下方 C15 及更早内容保留其历史范围。
<!-- C16-CURRENT-END -->

<!-- C15-CURRENT-BEGIN -->
2026-10-06 C15本地候选：工作台完整会话插件管理已接独立admin协议/owner与设置界面，
借原Manager/持久catalog，保留原busy/lost/恢复门禁和顶层native管道，不改旧Core/IO。
admin10、owner18、旧host18/14/13/1回归分别通过；Dart18（含实际Rust五对向量互验
和原管道Busy路由）通过。新库strict Clippy、Dart fatal-infos/限定格式和Windows宿主check
通过；不是新OS执行/真实桌面/ProtectedSession资格。327/57/旧合同/Linux/封存工件保持。
实际native port借原runtime/worker、生产GUI、认证/sandbox、交互控制及全SDK26/G04仍OPEN。
没有commit/push/Release/CI。详见 [C15报告](reports/reconstruction-2026-10-06/codex-sdk-c15.md)；下方C14及更早内容保留其历史范围。
<!-- C15-CURRENT-END -->

<!-- C14-CURRENT-BEGIN -->
2026-10-06 C14本地候选：完整Agent wrapper持久catalog/approval接口已实现，原Manager撤销seam
和同原connection/freshadmission接线保持。新catalog18、Manager新3与其原6、原回归11/1/6、
旧managed/route/schema14/13/1分别通过；实际Windows新1通过（helper1过滤，sandbox=None）。
大包装不受原快照512KiB限制，static审批不恢复livegrant；保存故障/Unknown即时撤旧Core，
旧效果不重放。327/57/旧合同/封存工件保持；Workbench新协议/GUI、ProtectedSession/native
owner、认证transport、sandbox、交互控制/其他平台及SDK26/G04仍OPEN。未commit/push/Release。
详见 [C14报告](reports/reconstruction-2026-10-06/codex-sdk-c14.md)；下方C13及更早文字为各日期历史，不替代本条实际范围。
<!-- C14-CURRENT-END -->


**简体中文（默认）** · [English](docs/readme/README.en.md) · [Русский](docs/readme/README.ru.md) · [Français](docs/readme/README.fr.md) · [Deutsch](docs/readme/README.de.md) · [Español](docs/readme/README.es.md) · [日本語](docs/readme/README.ja.md) · [한국어](docs/readme/README.ko.md) · [Português](docs/readme/README.pt.md)

留一点空间给明天的想法。

Morrow（明隙）是本地优先、卡片化的灵感工作台，正在向全平台插件架构演进。项目原名 daemon，代码包名为 `morrow_studio`。Windows 当前由 Flutter 提供界面、Rust 宿主与受限 Wasm 插件处理工作台业务；Web 在浏览器本地运行 Rust/Wasm，Android 暂保留旧路径。完整平台功能对齐仍在进行。

[打开 Web 预览](https://starrysky7d4.github.io/morrow/)：无需安装，工作区和附件原件保存在设备的浏览器存储中。已验证 Chrome 和 Windows Edge 的保存、刷新恢复与附件下载；完整功能边界与最新构建验收状态见 [Web 进度表](docs/WEB_PARITY.md)，发布方式见 [部署说明](docs/WEB_DEPLOYMENT.md)。

## 当前开发检查点

<!-- C13-CURRENT-BEGIN -->
当前 C13（2026-10-06，本地未提交）：会话/进程新入口已接原 Catalog/Registry/Manager，
共用原实例限额、Control 与连接；基础包与完整 wrapper 仍独立批准，预算取交集。
新增 managed host 14、Manager 6、HostIdentity 3、真实 Windows managed Wasm 2 分别通过；
原 host 1+13、Manager 11+1、strict 6、process 18+5 回归另计，重复不累计。
修正提交前取消、effect 后 Unknown 与错配收尾；新 host strict Clippy/限定格式通过。
旧 327 SDK／57 冻结输入、wire/schema 与封存工件保持；新候选不继承旧冻结 pin。
完整 wrapper 持久审批、Workbench/ProtectedSession owner、sandbox/认证/其他平台仍
OPEN/NOT_RUN；SDK26/G04 未冻结。见 [C13 接线与实际边界](reports/reconstruction-2026-10-06/codex-sdk-c13.md)。
下方 C12 及更早检查点保留历史结果与当时身份；本轮现状以 C13 为准。
<!-- C13-CURRENT-END -->

<!-- C12-CURRENT-BEGIN -->
当前 C12 接口增量（2026-10-06，本地未提交）：已补会话 Rust/Wasm 客户端、类型化进程控制、
严格 single-import 组合运行入口和原 R2 执行权实时复验。process 合同 23、R2 客户端 10、
旧入口真实 Wasm 6、process 客户端 10、组合 host 14、运行入口 6、定向 runtime 回归 21、
原 R2 回归 106 分别通过，重复方法不累计。Windows 原生实际执行 7 项、纯注册表 3 项、真实 proposal Wasm→Windows→process Wasm 耦合 3 项分别通过；封存 Wasm 未重建，Unknown 不重放。
327 SDK／57 冻结输入及旧封存归档保持；新候选不继承旧冻结 pin。
原生 close-input／PTY resize 尚 Unsupported；完整 SDK26／G04、产品 GUI、受保护内容库、
OS sandbox、认证 transport 和其他平台仍 OPEN／NOT_RUN。详见 [C12 接口与限定证据](reports/reconstruction-2026-10-06/codex-sdk-c12.md)。

下方 C11 及更早日期的文字保留当时的身份、结果与未运行范围；本轮现状以 C12 为准。
<!-- C12-CURRENT-END -->

当前C11检查点（2026-10-06，本地未提交）：Windows R2接口106通过；原Linux R2归档已用新外层校验器通过封存核验，旧pin不变。真实新Rust/C/C++目录Wasm在原managed owner上12方法通过；原件与目录回归70、只读映射7分别通过。四Python组99通过/3 FIFO跳过，新构建门禁15通过。SDK327/冻结57保持；完整SDK26/G04、产品批准链、ProtectedSession、picker、真实Windows执行器与其他平台仍OPEN/NOT_RUN。详见[C11限定资格](reports/reconstruction-2026-10-06/windows-sdk-c11.md)。

### 历史C10检查点（2026-10-05）

本次开发分支更新收录C08–C10。C10已新增独立 `fs-directory-request-v1`、严格单import、包feature及只读discovery，复用原已批准selection和owner，不导出路径、句柄或新授权。Windows新Rust34（codec12/helper4/profile9/owner9）、既有回归191及frame5分别通过；Python33和两个原生C/C++消费者另计。新Rust Wasm仅编译通过，真实新Rust guest、C/C++ Wasm、Workbench产品／GUI、受保护Session与其他平台仍未验收。旧Core IO FileList保持Unsupported，SDK26／G04仍OPEN，无新Release。 [C10](reports/reconstruction-2026-10-05/directory-request-sdk.md)

## 开发进展（2026-10-05）

当前应用源码检查点为 **0.1.9-test.58+62**。目标开发支线为 `codex/windows-sdk-convergence-20261005`，代码基于 cloud `468ef2e` 及 C02–C07 增量。完整现状、证据和下一步以 [项目当前状态](docs/PROJECT_STATUS.md) 为准；[C07 限定资格报告](reports/reconstruction-2026-10-05/directory-owner-sdk.md)保存实际 Windows 验证边界。

C07历史限定验证：Windows 本地合成验证完成原 owner 通路 115 个方法（其中目录取消 7、目录 owner 14、原生目录 16，均已包含在 115 中）、原回归 42 个方法，以及网络 11 个程序的 100 个方法。原 42 方法回归保留 frozen-region 的 14 个过滤项和 remote-reader 的 1 个过滤项，子进程辅助程序不另计通过方法。这些数字不代表完整 SDK 或产品验收。

目录 capture／page／finish 已进入原受信任 IoWorker 队列，沿用原 Manager、runtime、instance、IoBinding 与 FileList 审批。时钟采样与授权验证在短原子步骤内完成，原生资源实际释放后才释放配额；Unknown 不自动重放，游标不回绕。独立版本的 WebSocket／SSE 类型化载荷库已提供 Rust、C、C++ 编解码接口及限定 Guest 验证，目录／blob 编解码与有界状态也已完成本地资格。

## 历史 C08／C09 限定记录

**SDK26／G04 仍为 OPEN，完整 SDK 未冻结。** 生产受保护 owner、工作区任务产品验收／GUI、系统选择器及祖先来源证明、生产受信任选择与秘密生命周期、新目录 request／import／feature／helper profile 协商、blob 持久历史、真实 TLS／API 和其他平台仍待完成或未运行。C08阶段原worker秘密factory已完成限定Windows资格（factory14／69filtered）；原owner115、原件42、网络100分别fresh通过，详见 [C08记录](reports/reconstruction-2026-10-05/directory-secret-factory.md)。C08新增尚未commit／push，已有文档提交772466保持；本次资格没有发布新安装包，也不构成稳定 SDK。

C09本地可信目录相对选择已获Windows限定验证，新20方法及原回归范围见 [C09报告](reports/reconstruction-2026-10-05/directory-selection-owner.md)；它仅保留原opened root至相对后代的句柄链，不证明picker、root以上来源或生产资格。Workbench宿主入口仅通过编译检查，工作区任务产品验收／GUI仍未运行，公开guest FileList和conditional Replace仍Unsupported。完整SDK26／G04保持OPEN；已推送检查点为772466，C08/C09改动仍本地未提交、未推送、未发布新版本。

## 下载与兼容性

最新公开下载版：**0.1.9-test.56+60**，桌面安装包提供 **Windows x64 测试预览版**，不是稳定版。下载 Windows ZIP、对应源码 ZIP 和 SHA-256 清单；完整解压后运行 `morrow_studio.exe`，保留所有 DLL、宿主、`data`、`plugins` 与许可文件。

[下载 test.56](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.56) · [test.1 兼容测试版](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.1)

`test.1` 是 `0.1.x` 最后一个兼容原有数据类型的测试版。后续 test 版本推进重写，可能包含破坏性变更；架构与数据模型锚定并完成验收后发布 `0.2.0`。旧 test.1 数据不会自动导入或覆盖。升级前备份内容库与原保护文件；保护文件绑定 Windows 用户，仅复制数据库不能完成跨账户迁移。

## 可以做什么

- 卡片与内容：灵感、项目、实验记录、收藏、搜索、清单与删除撤销；Markdown 编辑与预览，附件保留独立副本。
- 富内容捕捉：文字、截图、文件、Office 富文本与表格；复杂或私有格式可能降级为预览／附件，不保证复现原软件的全部排版。
- 外观：磨砂、超透、液体玻璃；默认、纯色、纹理与透明画布；主题色彩色罗盘与每个组件／卡片的独立设置，支持减少动画。
- 媒体：图片、GIF、视频背景，本地音乐、歌词与悬浮提示。格式支持取决于平台与解码器；Web 透明背景透出网页背景，不是系统桌面。
- 布局与语言：响应式内容布局、独立设置页；中文、英文、俄文、法文、德文、西班牙文、日文、韩文和葡萄牙文。
- Windows 内容保护：Rust 核心统一保存、审计封存、内容库快照、原身份备份与恢复；同身份并发占用受限。

## 已发布 test.56 的优化与验证

test.56 增加卡片、小组件、任务与音乐列表的右键操作、卡片排序与拖拽、tips 逐条编辑及独立主题插件；修复宽屏瀑布流重排、筛选原位更新与安全关闭。发布记录包含 60 项 Flutter 回归、4 项原生集成及 17 项 Windows 实机布局回归。正式界面的完整自动保存尚未完成，详见 [test.56 发布说明](reports/0.1.9-test.56-release.md)。

### test.55 历史优化与验证

test.55 新增七种外观风格与深度调节，完善控件动画和组件间距；修复设置保存与字体应用，改进安全后台关闭，并提供草稿恢复基础。

### test.54 历史优化与验证

test.54 合并重复开库校验、以有界并行计算证据，并在同一校验事务内复用证据大小和归档摘要。每次开库仍完整校验；不改变存储格式或内置插件包，不引入跨启动验证缓存。

最后一轮读取优化与上轮并行版各进行四次交替启动：同机约 100 MB 库副本的加载遮盖移除中位数从 2.222 s 降至 1.299 s（Dart 入口起）。副本注册可能预热文件缓存，这不是清空缓存后的冷启动保证。Core 568、Audit 97、真实宿主集成 2 项通过；Core 另有 1 项既有忽略。具体条件与限制见 [test.54 发布说明](reports/0.1.9-test.54-release.md)。

## 从源码运行

需要 Flutter 3.44／Dart 3.12 或兼容版本；Windows 还需要 Visual Studio C++ 桌面构建工具与 Windows SDK、Rust 工具链，以及加入 PATH 的 Cap’n Proto 编译器。

Windows Rust 工作台完整构建、集成检查与打包：

```powershell
flutter pub get
rustup target add wasm32-unknown-unknown
pwsh -File tool/build_rust_workbench_windows.ps1
```

打包产物不可覆盖；使用新版本或新的输出目录，`-RefreshArtifact` 不再允许覆盖。分发时必须保留完整运行目录。Web／Android 的构建和验收范围见平台文档，不以 Windows 构建结果替代。

开发与基础检查：

```powershell
flutter run -d windows
flutter run -d chrome
flutter analyze
flutter test
flutter build web --no-web-resources-cdn
```

## 架构、SDK 与后续工作

目标架构为 Flutter／Dart 界面、可移植 Rust 核心及可替换插件执行后端。运行期边界采用固定契约；持久化与自有交换采用 Protobuf＋LZ4。C／C++／Rust SDK 与声明式插件 UI 正在推进，暂不支持 TS／JS 插件，也不要求动态 Dart 插件。

完整 SDK 尚未冻结，SDK26／G04 仍为 OPEN。已接入受管 HTTP／HTTPS 请求、有限 API 服务节点与 TLS 身份管理；源码包含 C／C++／Rust 实验 IO SDK、项目模板和原包真实 HTTP 验证。当前目录 owner、类型化 WebSocket／SSE 载荷库及目录／blob 编解码的 Windows 限定资格见 [项目当前状态](docs/PROJECT_STATUS.md)，不能作为真实 TLS／API 或生产通路验收。这些源码增量不属于公开 test.56 包。新目录profile的真实三语言guest与产品资格、完整入站服务产品资格、条件 Replace、系统选择器人工验收、完整文件系统、跨重启核对与跨平台资格仍待完成。详见 [插件系统当前状态](docs/PLUGIN_SYSTEM_STATUS.md)。开库仍扫描全量历史；完整读取／计算流水线、历史分层与自动清理尚未实现。

## 文档

- [项目当前状态（统一入口）](docs/PROJECT_STATUS.md)
- [本版发布说明与验证](reports/0.1.9-test.56-release.md)
- [开发看板与后续任务](docs/DEVELOPMENT_BOARD.md)
- [未来架构路线](docs/FUTURE_ROADMAP.md)
- [功能迁移与容量边界](docs/TEST1_RUST_PARITY.md)
- [C／C++／Rust SDK](sdk/README.md)
- [插件 UI 对接设计](docs/PLUGIN_SDK_AND_UI.md)
- [富内容捕捉与格式范围](docs/RICH_CAPTURE.md)
- [Android 构建](docs/ANDROID.md)
- [重命名兼容策略](docs/RENAMING.md)
- [历史 README 与阶段记录](docs/history/README-before-test54.md)

详细设计和验证报告目前主要为中文；各语言 README 提供一致的当前版本入口，译文尚未经母语使用者全面审校。

## 许可证

从 `0.1.9-test.2` 起，第一方代码、SDK、文档、配置与素材采用 **AGPL-3.0-only**。test.1 及更早发行版保留原 Apache-2.0 许可；第三方内容保留各自许可。预构建包附带许可证、版权声明与对应源码入口。

[AGPL-3.0-only](LICENSE) · [NOTICE](NOTICE) · [Third-party licenses](packaging/THIRD_PARTY_NOTICES.txt)
