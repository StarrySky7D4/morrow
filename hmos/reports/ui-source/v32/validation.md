# v32 发布前验证

2026-10-09，源码审计基线 `62a8b964bb27840f0348bb6b8ef520142880b9b1`。用户要求推送 GitHub；仅交付 `codex/ArkTsUI`，不合并主线。完整 Flutter/Windows 追平目标仍 **OPEN**。

本轮按 Flutter 普通 Glass 的 `NeumorphicSurface(depth: 0)` 移除额外 raised 轮廓，覆盖普通 Index 面板及候选材质预览；真实 recessed 搜索保留凹陷。搜索与音乐提取共用 `RecessedGlassRelief`：搜索按自己的材质/depth/radius 及 -1 倍数，音乐子层按全局 Appearance 色盘/depth/radius 及 -0.8 倍数，音乐外层覆盖仍独立。主阴影半径/偏移经实际 `vp2px` 转为 SDK 物理 px，明确 `fill=false`；候选预览使用自身 clear/frosted/liquid 模式。具体源码与十项 Flutter/SDK 引用、设备检查范围见 [Glass/Music 对齐清单](music-inset-style-device-checklist.md)。

这些是确认源码设计差异后的修正候选，**不是所有界面未知框线已修复或完整 UI 等价声明**。第二外侧光阴影仍为 `OPEN_SDK_SCALAR_SHADOW_AND_PARENT_CLIP`：SDK 通用 shadow 只提供一个 cast，正确补齐需要独立有界外层且不被父容器裁剪；本轮没有用额外硬框线冒充这一效果。音乐业务、文件、曲库和播放器方法没有因该绘制修改而新增运行资格；v31 原 wire 持久确认、一次性 picker ticket 与完整原目标歌词边界继续保留。

## 冻结来源与结果

- [完整主机检查](models-final-result.json)：**1168/1168 PASS，0 fail/skip/cancel**，49 个测试文件，156 项模型来源前后完全一致，42378.6852ms；[日志](models-final-tests.log) SHA256 `1656638ACD9C6533A61AF2C3A567F8CFD7BABB2BA23176171177B50A4A4548AE`。执行实际 ETS/tool/Index/组件方法及 Store 产生的 DTO；provider、AVPlayer、生命周期和 Canvas rasterizer 仍为受控边界，不是设备播放或像素验收。
- [最终绘制组合 a2](glass-style-alignment-a2-result.json)：**23/23 PASS**，8 项输入、10 项 Flutter/SDK 引用哈希一致，1214.3826ms。覆盖七样式/浅深色凹陷、搜索 -1 与音乐 -0.8、局部外层与全局内层、普通 Glass 轮廓移除、候选自身材质、物理 px 主阴影及 separate footer/liquid guards。这些属于上面的全量检查，不重复累计。[前一候选](glass-style-alignment-final-result.json) 为22/22、旧 Index 哈希，保留历史，不覆盖 a2 的最终来源。两份组合的 `sdk_build=NOT_RUN` 只指该受控组合本身；最终产品 API26 构建由下一项单独证明。
- [完整 API26 构建](dev32-glass-alignment-a1-sdk-result.json)：**SUCCESS /40.621s**，34/34 tasks 执行、0 up-to-date；[构建日志](dev32-glass-alignment-a1-sdk-build.log) SHA256 `E51E662995681F9EEF5D6002F809B808E290416197032B249DB33371B6674580`。[324 复制文件](dev32-glass-alignment-a1-source-copy.json)与[367 仓库来源](dev32-glass-alignment-a1-repository-inputs.json)记录精确构建身份。使用实际完整产品入口，九模块均检查并 emit：MusicFiles、MusicLibrary、MusicPlayback、MusicUi、MusicWorkbench、PlatformMusicPlayer、MusicPanel、MusicFooter 和 RecessedGlassRelief；见 [包内检查](dev32-glass-alignment-a1-package-check.json)。检查说明中的“八音乐模块”指既有音乐模块，列表另明确包含第九个共用绘制模块。保留 SDK 平台能力与异常处理警告，不声称所有设备支持。
- [Native 精确复用](dev32-glass-alignment-a1-native-reuse.json)：283 Rust/C++ 来源与 v29 完全一致；ARM64 静态库58,110,214B / `DD86DF956795BC04CDF98BC3184625C6FAD9D0382E041E86DEAF99C44028744A`，x64静态库56,515,718B / `E9C66A4974D32CD9B6C68559504F56EBA7D10CC27F1DB3CCFDC31B4FDDA4BF1E`，`freshlyRebuilt=false`。本轮没有新 Rust 测试或构建资格。四项包内 .so 与本次 compiler stripped outputs 一致，不声称包内 .so 与旧包逐字节相同。
- [未签名 HAP](dev32-glass-alignment-a1-artifact.json)：**31,442,946B**，SHA256 `06342B5CFB5EDA93493BEDA13B76B6DB9D0AF32C4F95394C280B1B6448E1C4FB`，**0.1.0-hmos-dev.22 /1000022**。本地不可变归档 `.build/artifacts/dev32-glass-alignment-a1/entry-default-unsigned.hap`，unsigned、**未安装**，不是签名发布。

## 本轮设备观察：已安装 dev21，未安装 dev22

[安装与启动回执](device/installation-dev21.json)记录既有 Pura X View2 UUID `01fc19c8-444a-42f1-9f70-79321a2502b3`、HDC `127.0.0.1:5555`：实际安装的是 v31 不可变包 **31,438,704B /92258D5D2F38C195383FF7549D58C3876ABA1986C53F69B6BBF32A12D0AE081F**，读回 **dev21/1000021** 并启动。此记录只提供主机包身份、安装确认和 bundle 版本，不是受保护的设备包哈希，也不授予 v32 新绘制资格。dev21 首页/设置/空曲库观察保留在本目录 device 证据中；旧图不得当作 dev22 全样式像素验证。

两次 DocumentPicker 流程均观察“浏览/我的手机”后点击 Download，并随后关闭回到应用；第一轮见 [Download 动作](device/action-dev21-picker-download.json)及 [未选择回到应用](device/dev21-picker-ended-no-selection.json)，第二轮见 [Download 图标动作](device/action-dev21-picker-download-icon.json)及 [返回后的原始 UI](device/dev21-after-download-icon-raw.json)。没有选中音乐文件、没有已确认音乐导入或播放，因此不能把 picker 打开/关闭、空曲库界面或按钮点击算作音乐运行通过。此相关观察也不证明确定的 provider 缺陷。

自生成公开 WAV/LRC 的存储转移与哈希核对只是准备工作；例如 [长 WAV 传输](device/HMOS-v31-10105badbf93-long.wav-storage-transfer.json)确认1,920,044B原件一致，不证明 DocumentPicker 返回该文件的授权 URI、音频解码或声音输出。较早传输/路由尝试与动作记录原样保留；不将另一轮成功的准备步骤改写为音乐导入成功。

## 仍开放的资格

**dev22 设备渲染与播放均 NOT_RUN**。全部页面未知框线、七样式/材质/浅深色/宽度/局部覆盖的像素矩阵、真实 shadow blur/裁剪、高对比度/过渡/光学折射仍未验收，第二外侧光阴影方案 **OPEN**。实际 provider/grant、OHOS fsync/close、导入原请求中断/断电恢复、声音/codec/seek/EOF、后台互斥、拖拽/上下首、重启部分清理残留仍未取得设备资格；unknown 和未确认对象不盲认、不删除。

在线歌词、解密、metadata/封面、原件 GC/protected、HUKS/正式宿主、ARM64 运行、签名、完整 Flutter/Windows parity 保持 **OPEN**。图标/文本或测试项数、构建成功与 dev21 安装不会关闭这些产品资格。

[后续差异审计](next-ui-parity-audit.md)和[待办方案](task-pending-decision-design.md)是后续设计/检查清单，不是本轮已实现或设备通过的功能。完整目标继续跟随原功能线程；本轮发布范围仅专用分支，无主线合并。
