# GitHub Pages 发布与验收（2026-09-25）

目标站点：https://starrysky7d4.github.io/morrow/

## 发布配置

- 公开仓库 `StarrySky7D4/morrow`，主分支 `main`。
- Pages 使用 GitHub Actions，HTTPS 强制开启，`github-pages` 环境允许 `main`。
- 最终发布源码：`a4fa217fc2333e81048e0bd1f77e5867fa44a6a2`。
- 成功构建及部署：https://github.com/StarrySky7D4/morrow/actions/runs/36094914475
- 工作流固定 Flutter 3.44.4、Rust 1.96.0、LLVM 22.1.7、Capn Proto 1.4.0、wasm-bindgen 0.2.128；使用 Chrome for Testing 154.0.8037.57。
- `/morrow/` 基路径，静态产物包含匹配的 Rust/Wasm、工作台插件、九语言包、源码提交引用、许可证及 SHA256 清单。

## 本地正式路径验收

全部使用独立临时浏览器配置，未访问用户现有浏览器数据。

- 创建工作区、选取文件、保存卡片、刷新后重新打开通过。
- 附件下载与选取的原件逐字节一致；刷新后身份未变化。
- 新旧存储并存时均可访问，旧 LocalStorage 字节不变。
- 单独存在旧内容时不创建替代工作区。
- OPFS 有内容但缺失身份时拒绝创建新库，保留已有文件。
- 加载完成后断网创建、导入附件与保存通过，联网刷新恢复内容通过。
- 页面网络记录均为静态资源 GET，没有 POST、请求正文或查询参数。
  资源来自本站及 Flutter 按需加载字体的 `fonts.gstatic.com`。

本地证据：`build/pages-local-fresh.log`、`build/pages-local-legacy.log`、
`build/pages-local-orphan.log`、`build/pages-local-offline.log`。

## 线上验收

最终云端构建、三场景验收、产物上传及 Pages 部署均成功。已修正 PowerShell
场景参数展开和 JSON 测试准备页面的不稳定行为，使用正式 HTML 页面初始化
隔离测试数据；CI 日志分别记录 fresh（含断网编辑）、legacy、orphan 三项 PASS。
失败的中间构建没有替换已上线版本。

- HTTPS、`/morrow/` 路径、发布源码提交均正确。
- 36 个关键资源 SHA256 一致，包括 Dart 应用、Worker、Rust/Wasm、插件、
  CanvasKit、九语言包及许可文件；JavaScript/Wasm 响应类型正确。
- Chrome 154：加载后断网创建/导入附件/保存，联网刷新后读回并下载原件通过；
  新旧库并存访问、旧库独立访问、缺失身份的孤立数据保护通过。
- Windows Edge 153：加载后断网创建/导入附件/保存、联网刷新、原件下载通过。
  测试使用 CDP 固定语言，避免 Edge 跟随系统语言造成英文按钮定位假失败。
- 线上页面请求仅包含本站和 Google 静态字体资源的 GET，未观察到业务上传。

证据：`build/pages-http-acceptance.json`、`build/pages-live-chrome-fresh.log`、
`build/pages-live-legacy.log`、`build/pages-live-orphan.log`、`build/pages-live-edge.log`，
以及 `build/pages-live-chrome-network.json` / `build/pages-live-edge-network.json`。

最终部署后再次核对发布提交及 36 个资源摘要，并重跑 Chrome/Edge 断网编辑、
刷新恢复和原件下载：`build/pages-final-live-chrome.log`、
`build/pages-final-live-edge.log`。两次发布间的应用 JavaScript、核心 Wasm、
Worker 和插件原包摘要相同；变化为 Flutter 启动脚本和源码版本说明。

## 范围

云端提供程序静态资源，浏览器本地运行 Rust/Wasm；工作区和附件原件存于
OPFS，设备身份存于 IndexedDB。断网编辑验证不表示首次打开网址可离线加载。
本次验收不等于完整 Windows 功能对齐；旧库迁移、备份恢复、持久草稿附件、
字体/媒体全场景、大库与配额、网络服务适配仍按 `docs/WEB_PARITY.md` 跟踪。
