# test.11 Windows 与 SDK 验证

日期：2026-09-12。所有运行数据来自独立合成目录，未覆盖 test.1 用户资料。未推送 GitHub，未发布 Release。

交付包：`dist/morrow-0.1.9-test.11-rust-workbench-windows.zip`，39346717 字节。

SHA-256：`61ce9830320e0d94db8c3aeff53abac4650848d74c5239dfcd1bf1f169886d1c`。

打包目录内 236 个文件已逐个核对 `SHA256SUMS.txt`。最终包中 `morrow_studio.exe` 已实际运行，退出码 0；检查记录与渲染截图保存在 `build/workbench-host/windows-final-74e2f6a857e5490caac6416b8a7bc4fa/result.md`、`result.png`。记录包括 Windows 原生磨砂初始化/调整/关闭、实际 WAV 解码播放时钟、跳转与音频互斥、恢复不自动播放、无 Flutter 渲染错误。截图仅覆盖 Flutter 自身绘制，不能证明窗口外桌面的磨砂像素效果。

Rust、C、C++ 三份独立 Wasm task 模块已通过 `qualify_content_sdk`：正文创建、编辑、尾片段读取、权威完成验证、撤销后的拒绝。核心与运行器完整回归通过；Flutter 全套 73 项通过，最终材质预览提交保护定向回归 5 项通过。严格分析与独立 SDK 校验通过；Web 核心通过 wasm32 编译检查。

SDK 接入见 [正文 API](../sdk/CONTENT_API.md)，全功能对照与待办见 [迁移记录](../docs/TEST1_RUST_PARITY.md)。本轮不是完整插件系统验收；大正文写入、容量清理、完整插件升级和多平台实机验证仍待继续。
