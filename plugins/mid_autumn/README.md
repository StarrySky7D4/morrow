# 中秋 · 月满庭：独立主题插件

标准 Morrow `.morrowplugin` 插件，ID `org.morrow.theme.mid-autumn`，版本 `1.0.0`。颜色、文案与原创插画均随包分发，客户端没有内置中秋 JSON、图案或自动重装逻辑。无需网络、内容读写或 IO 权限。

## 安装与管理

1. 打开 [Windows 测试预览版 0.1.9-test.56](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.56) 或更新的兼容客户端，或 [Web 工作台](https://starrysky7d4.github.io/morrow/)。已安装 1.0.0 主题包无需重复导入。
2. 从[独立主题发布页](https://github.com/StarrySky7D4/morrow/releases/tag/theme-mid-autumn-v1.0.0)下载 `.morrowplugin`，在「空间外观」中的插件管理入口选择该文件。本地构建输出为 `dist/plugins/morrow-mid-autumn-1.0.0.morrowplugin`。
3. 确认导入后，插件默认停用；在该插件条目点击批准并启用。
4. 已安装主题会出现在「界面风格」列表上方，可快速切换或停用；原有的平面、纸张、黏土等风格始终可以叠加。
5. 在插件管理中停用或卸载。卸载取消登记，原有内容和插件包缓存按现有插件系统规则保留，不会自动重新安装。

主题启用状态由资料库插件登记管理，重启自动恢复。旧版 `morrow.ui-theme-plugins.v1` 状态不再启用内置主题；首次升级需要导入独立文件。材质覆盖偏好按主题 ID 保存在 `morrow.ui-theme-options.v2`，与业务数据和风格设置分离。

## 互斥、叠加与控件

- 同时只启用一个主题。登记层在一次原子提交中停用其他主题，运行时撤销旧主题实例；业务插件的启用和授权不变。
- 切换基础风格不会停用主题；主题也不会强制切回平面风格。
- 默认保留自选玻璃、圆角、组件材质和背景。开启「主题材质与背景」后使用主题材质；基础风格的轮廓和深度仍然叠加。
- 主题配色启用时隐藏全局和组件调色罗盘、组件颜色继承按钮、颜色重置、自定义色调及灰度控件，保留浅色/深色切换。已打开的调色弹窗立即停止编辑与提交，停用主题后需重新打开；原组件颜色仍保存，随主题停用恢复。
- 主题接管材质时隐藏玻璃模式、磨砂透明度、组件材质入口、组件圆角及背景设置；窗口圆角和字体等有效设置保持可用。已打开的组件材质页保留草稿并显示说明，关闭覆盖后可继续编辑。
- 停用主题后恢复原有控件和设置。插件管理页不显示面向原始字节的主题转换工具，也不显示无权限可改时的重复保存按钮。

覆盖共享 Palette、Material Theme 和应用内可主题化的控件与装饰；操作系统文件对话框、用户媒体、第三方硬编码绘制内容仍由各自所有者控制。

## 包与协议

插件是 Rust Wasm 的纯转换提供者，通过现有包校验、导入、选择、启停、卸载与执行流程管理：

- `theme.describe`：空请求，返回 `morrow.ui.theme.v1` JSON（最多 16 KiB），包含身份、名称、浅深色 token、文案和可选图像描述。
- `theme.artwork`：请求为 4 字节小端偏移，每段最多 32 KiB，返回图像字节。通用客户端限制总长 1 MiB、边长 2048，并核对 SHA-256 与实际图像尺寸。
- 每段读取和最终发布都检查登记版本。资源损坏、格式错误或授权变化会回退基础界面并显示错误，业务插件保持独立。
- `theme.describe` 的准确 handler/input/output 类型组合标识主题槽位，任意符合协议的其他主题包同样受互斥规则约束。没有为中秋 ID 写特例。

包大小 661,049 字节。SHA-256：`B9C8DD591EF88E2D14BE4EE7F4B402FF5D6563530618F71CBAF1E4D1B61D9751`。

## 构建

```powershell
./tool/build_mid_autumn_theme.ps1
```

需要现有 Rust、Wasm target 和项目依赖。脚本不修改或重打包工作台 guest。同版主题字节改变时打包器会拒绝覆盖，需同步更新主题清单、Cargo 版本和打包器版本。

`artwork/moonlit-garden-source.png` 为内置 imagegen 生成的原创源图；`artwork/moonlit-garden.webp` 为保持构图与尺寸的交付编码。最终提示词见 `artwork/PROMPT.md`。

参考案例、实现与验证见 `reports/mid-autumn-v3-2026-09-25.md`。

组件调色崩溃修复与 Windows 原生验证见 `reports/component-color-fix-2026-09-25.md`。

Web 导入、设备侧保存、Windows 回归及发布验收见 [主题支持报告](../../reports/web-theme-plugins-2026-09-25.md)。Web 当前只接收无业务权限、IO/服务声明或依赖的主题包；原始验收文件位于 `test/fixtures/plugins/morrow-mid-autumn-1.0.0.morrowplugin`。
