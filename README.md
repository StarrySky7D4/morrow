# Morrow · 明隙

**简体中文（默认）** · [English](docs/readme/README.en.md) · [Русский](docs/readme/README.ru.md) · [Français](docs/readme/README.fr.md) · [Deutsch](docs/readme/README.de.md) · [Español](docs/readme/README.es.md) · [日本語](docs/readme/README.ja.md) · [한국어](docs/readme/README.ko.md) · [Português](docs/readme/README.pt.md)

留一点空间给明天的想法。

Morrow（明隙）是本地优先、卡片化的灵感工作台，正在向全平台插件架构演进。项目原名 daemon，代码包名为 `morrow_studio`。Windows 当前由 Flutter 提供界面、Rust 宿主与受限 Wasm 插件处理工作台业务；Web 在浏览器本地运行 Rust/Wasm，Android 暂保留旧路径。完整平台功能对齐仍在进行。

[打开 Web 预览](https://starrysky7d4.github.io/morrow/)：无需安装，工作区和附件原件保存在设备的浏览器存储中。已验证 Chrome 和 Windows Edge 的保存、刷新恢复与附件下载；完整功能边界与最新构建验收状态见 [Web 进度表](docs/WEB_PARITY.md)，发布方式见 [部署说明](docs/WEB_DEPLOYMENT.md)。

## 开发进展（2026-10-05）

当前应用源码检查点为 **0.1.9-test.58+62**。目标开发支线为 `codex/windows-sdk-convergence-20261005`，代码基于 cloud `468ef2e` 及 C02–C07 增量。完整现状、证据和下一步以 [项目当前状态](docs/PROJECT_STATUS.md) 为准；[C07 限定资格报告](reports/reconstruction-2026-10-05/directory-owner-sdk.md)保存实际 Windows 验证边界。

Windows 本地合成验证完成原 owner 通路 115 个方法（其中目录取消 7、目录 owner 14、原生目录 16，均已包含在 115 中）、原回归 42 个方法，以及网络 11 个程序的 100 个方法。原 42 方法回归保留 frozen-region 的 14 个过滤项和 remote-reader 的 1 个过滤项，子进程辅助程序不另计通过方法。这些数字不代表完整 SDK 或产品验收。

目录 capture／page／finish 已进入原受信任 IoWorker 队列，沿用原 Manager、runtime、instance、IoBinding 与 FileList 审批。时钟采样与授权验证在短原子步骤内完成，原生资源实际释放后才释放配额；Unknown 不自动重放，游标不回绕。独立版本的 WebSocket／SSE 类型化载荷库已提供 Rust、C、C++ 编解码接口及限定 Guest 验证，目录／blob 编解码与有界状态也已完成本地资格。

**SDK26／G04 仍为 OPEN，完整 SDK 未冻结。** 生产受保护 owner、GUI／工作区任务入口、系统选择器及祖先来源证明、新鲜受信任密钥工厂、新目录 request／import／feature／helper profile 协商、blob 持久历史、真实 TLS／API 和其他平台仍待完成或未运行。C08 仅为设计；本次资格没有发布新安装包，也不构成稳定 SDK。

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

完整 SDK 尚未冻结，SDK26／G04 仍为 OPEN。已接入受管 HTTP／HTTPS 请求、有限 API 服务节点与 TLS 身份管理；源码包含 C／C++／Rust 实验 IO SDK、项目模板和原包真实 HTTP 验证。当前目录 owner、类型化 WebSocket／SSE 载荷库及目录／blob 编解码的 Windows 限定资格见 [项目当前状态](docs/PROJECT_STATUS.md)，不能作为真实 TLS／API 或生产通路验收。这些源码增量不属于公开 test.56 包。新目录 request／import／feature／helper profile 协商、完整入站服务产品资格、条件 Replace、系统选择器人工验收、完整文件系统、跨重启核对与跨平台资格仍待完成。详见 [插件系统当前状态](docs/PLUGIN_SYSTEM_STATUS.md)。开库仍扫描全量历史；完整读取／计算流水线、历史分层与自动清理尚未实现。

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
