# Windows 阶段 Demo 验证

日期：2026-09-11。基线：0.1.9-test.10；专用展示入口，不改变正式应用数据路径。

## 交付

- `dist/morrow-0.1.9-test.10-demo-windows.zip`，17,192,290 字节，Windows x64 Release 便携包。
- SHA-256：`f32f322e860e11b0825b0f9e5234d28b3be3f0f88ee541d1d97f3d53124cc17e`。
- 解压完整目录后运行 `MorrowStageDemo.exe`；包含实际 Rust 宿主、C/C++/Rust 插件包、Flutter 运行文件、应用本地 VC 运行库、使用说明、许可说明、5 张原始展示截图和文件校验表。
- 同目录 `.zip.sha256` 校验最终压缩包；不同时间重新打包可能因 ZIP 元数据而得到不同摘要。

## 已验证

1. 优化版 Rust 宿主编译、Clippy（警告视为错误）；真实 C/C++/Rust Wasm 模块编译和不可变插件包生成。
2. 独立 Flutter Demo 分析无问题，Windows Release 编译成功。
3. 控件测试启动真实子进程与插件，覆盖 TextField 编辑、三语言会话切换、连续快速输入、明暗主题保留状态、480 像素窄屏与退出清理。全部通过。
4. 实际 Windows Release 的自检入口分别运行三种插件，标题经过 UI Event → 宿主 Session → Wasm 任务 → 独立 Document 校验 → Flutter 展示，均收到修订 2 的正确回应。明暗主题和窄屏截图来自运行中应用的 RepaintBoundary。
5. 最终 ZIP 解压至带空格的独立目录，194 个文件的 SHA-256 全部匹配；再次启动解压后的 Release，三语言交互及截图自检通过。

完整构建日志：`build/stage-demo-final-build.log`。
原始截图与运行记录：`build/stage-demo-evidence/`。
解压运行证据：`build/stage demo extracted bd2317e8c9a14e69bedbfbbcfa182bc5/runtime evidence/`。
复现入口：`tool/build_stage_demo_windows.ps1`；见 `demos/plugin_stage_windows/README.md`。

## 展示范围与限制

这是插件 UI 的阶段演示，不是完整主应用重写版。每个会话使用私有临时库，不请求内容权限，不保存正式卡片。置顶和应用动作在宿主与界面两侧均禁用，作为组件外观预览。输入切换插件后重置。

示例使用专用二进制管道接入层；不宣称正式包 UI 注册、稳定 IPC、持久草稿、完整审计、在线会话或全平台已经完成。实际 Windows 程序运行已验证，但没有物理鼠标、真实输入法、干净虚拟机或其它设备的验收证据。未制作视频；展示截图为真实程序内容，不含操作系统窗口边框。
