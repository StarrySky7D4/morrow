# Morrow 重命名说明

2026-09-09，项目由 daemon 更名为 **Morrow（明隙）**，寓意“给明天的想法留一点空间”。

| 范围 | 新名称 |
| --- | --- |
| 产品 | Morrow · 明隙 |
| 仓库与项目目录 | morrow |
| GitHub | StarrySky7D4/morrow |
| Dart 包名 | morrow_studio |
| 应用根组件 | MorrowApp |
| Windows 可执行文件 | morrow_studio.exe |
| Android 代码命名空间 | dev.morrow.morrow_studio |
| 分发文件前缀 | morrow- |

## 已有数据兼容

- Windows 的新应用支持目录为 `%APPDATA%/dev.morrow/morrow_studio`。首次启动且新目录没有偏好文件时，从旧 `%APPDATA%/dev.daemon/daemon_studio` 复制 `shared_preferences.json`，保留原文件；已有 Morrow 偏好文件优先，不反复覆盖。
- 原有附件、纹理和音乐使用绝对路径，新版本继续读取旧素材目录，不搬移或删除原素材。不要手动清理旧目录，除非确认不再引用其内容。新导入素材使用新目录；旧目录素材不会由新目录的清理逻辑删除。
- 偏好数据键 `daemon.studio.v1` 和 Web IndexedDB 数据库名 `daemon-media` 是持久化格式标识，继续保留，以保持现有内容可读。
- Web 同源部署可继续读取已有数据；换域名或端口属于新的浏览器来源，不会因名称兼容自动迁移数据。
- Android 安装 ID 保留 `dev.starrysky7d4.daemon`，预览版仍为其 `.preview` 变体。产品显示名和代码命名空间更新，但保持原签名与安装 ID 才能沿用升级路径及应用数据。改名不会创建或替换签名密钥。

## 历史与开发环境

- 旧版 Git 历史、发布记录、附件名称及校验值不改写。旧版 ZIP、APK、演示视频仍代表原来的产物，新构建与新生成的演示使用 Morrow 名称。
- 构建脚本、使用文档、界面标题、平台消息通道与测试导入同步更新。
- 项目目录变更后需重新运行 `flutter pub get` 并重新生成构建目录，避免工具缓存中的绝对路径指向旧目录。
- 重命名不表示 UI 设计原则文档中的独立业务核心／FFI 架构已经实现，也不改变当前功能范围。

## 更名完成与发布范围

- 本地目录已由 `Morron` 更名为 `morrow`；产品、Dart 包、Windows 可执行文件、Web 标题和平台通道已使用新名称。
- 远端为 `https://github.com/StarrySky7D4/morrow.git`。v0.1.7 对应 `0.1.7+8`，将更名后的源码和 Windows／Web 分发包保持一致；发布验证见 [v0.1.7 记录](../reports/0.1.7-release.md)。
- Windows 及 Flutter 的旧绝对路径缓存已保存在 `build/rename-backup/`，从新目录重新生成。`analysis_options.yaml` 排除 `build/**` 中的生成项目和备份，应用源码及测试仍正常分析。
- `daemon.studio.v1`、`daemon-media`、旧 Windows 数据路径和 Android 安装 ID 按上述兼容策略保留；它们不是遗漏的产品名称。
- Android SDK 尚未安装完整，未生成本次 APK，也未进行 Android 真机验证。当前发布范围为 Windows 和 Web。

## 迁移其他旧工作副本

当前工作副本已经完成目录更名，无需再次移动。仍使用 `daemon` 或 `Morron` 的其他副本可在关闭占用目录的应用后，从父目录执行：

```powershell
pwsh -File .\Morron\tool\rename_project_directory.ps1
# 原目录为 daemon 时，将上面的 Morron 改为 daemon。
# 加 -WhatIf 只预览，不移动目录或缓存。
```

脚本检查项目包名、拒绝源目录链接和已存在的目标目录；成功移动后，将 Windows、Flutter 的路径缓存保存在新目录内的 `build/rename-backup/directory-<唯一标识>/`。旧产物、演示素材和 Git 状态保留，可复用的媒体依赖压缩包复制回构建目录并由 CMake 校验。临时副本已验证实际移动、重复执行、目标冲突、缓存备份和素材保留。

重新打开 `morrow` 后运行 `flutter pub get`，再重新构建 Windows／Web。不要为修复路径直接清空整个 `build`，其中可能包含尚未归档的演示素材。
