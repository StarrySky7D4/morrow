# 提案：将现代桌面工作台布局与声明式布局插件规划并入主线（0.2.x / 0.3.x）

- **署名**：Gemini
- **日期**：2026-11-24
- **状态**：提案草案（供主 Agent 复盘与评审；本提案本身不改变 `0.1.x` 代码与发布门禁）
- **参照原型分支**：`ui-plugin/modern-workbench-layout`（提交 `137a417b`，基于 `codex/preview-0.1.9-test.58-windows` `41a18252`，零 Rust / Cap'n Proto / 国际化生成代码改动）

---

## 1. 提案背景与架构红线

当前 `0.1.9-test.58` 仍处于预构建检查与资格收尾阶段（见 [当前开发状态](PROJECT_STATUS.md) 与 [qualification/current.json](qualification/current.json)，`source_accepted=false`、`release_eligible=false`）。为避免干扰现有 `0.1.x` 预构建门禁、既有布局测试与原生宿主资格，新版“现代桌面工作台”重构已被独立冻结在原型分支 `ui-plugin/modern-workbench-layout` 上。

本提案建议在主规划线（[未来路线](FUTURE_ROADMAP.md) / [插件 SDK 与 UI 对接设计](PLUGIN_SDK_AND_UI.md) / [UI 设计思想与架构原则](UI_DESIGN_PRINCIPLES.md)）中正式纳入 **“声明式布局插件化（0.2.x）→ 工作台插槽扩展（0.3.x）”** 演进路线，同时严格守住以下不可妥协的架构红线：

1. **零动态 Dart / JS Widget 插件**：第三方插件继续限定为 C / C++ / Rust（Wasm 纯转换或受限宿主契约），不开放任意 Flutter Widget API，不破坏跨平台与安全隔离前提。
2. **宿主保留即时交互权威**：焦点、光标、选区、滚动、全局快捷键、逐帧动画插值、玻璃/模糊合成及 Windows `ui::AXTree` 无障碍树稳定性由 Flutter 宿主独占负责，绝不跨语言逐帧往返。
3. **核心保留业务与权限权威**：所有业务状态、版本化内容修订、草稿恢复与插件权限授权继续由 Rust 核心统一裁决；布局插件不得拥有写库、网络或文件 IO 权限。
4. **卸载即无损回退**：布局插件属于纯表现层配置；禁用、卸载或校验失败时，内容、草稿、排序、音乐与外观材质偏好必须零丢失，并自动回退宿主默认布局。

---

## 2. 布局插件化的三层能力边界

| 层次 | 插件可声明的内容 | 可行性与安全边界 | 目标阶段 |
| --- | --- | --- | --- |
| **L1：声明式布局配置包** | 导航形态、检查器形态、命令栏开关、画布头部风格、卡片视图密度、底栏样式、动效档位（纯有界数据描述） | **高**：与现有 `mid_autumn`（`theme.describe`）同机制，零权限纯转换，宿主负责渲染与响应式降级 | **0.2.x** |
| **L2：标准插槽扩展与动作注册** | 向工作台标准插槽（检查器自定义 Tab、侧栏小组件、命令面板命令、卡片自定义区）注入声明式 UI 树并绑定动作 ID | **中高**：需将声明式 UI 协议（`UI1` → `UI2`）接入主应用视图会话与授权调度 | **0.2.x → 0.3.x** |
| **L3：自由布局代码 / 任意渲染树** | 插件自定义 Flutter 布局树、像素级绝对坐标、自定义帧动画或手势拦截 | **明确排除**：破坏多端响应式与无障碍，违反 [UI 设计思想与架构原则](UI_DESIGN_PRINCIPLES.md) | 不纳入规划 |

---

## 3. 原型分支 `ui-plugin/modern-workbench-layout` 验证摘要

原型分支 `137a417b` 在完全不改动底层 Rust 的前提下，验证了一套独立于旧版三栏布局的现代桌面工作台外壳，天然梳理出了可插件化的宿主区域边界：

| 区域 / 模块 | 原型实现文件 | 可抽象为布局原语的形态 |
| --- | --- | --- |
| **可折叠图标导航栏 + 窄屏悬浮 Dock** | `lib/ui/workbench_rail.dart` | `classicSidebar` / `collapsibleRail` / `bottomDock` |
| **顶部命令栏** | `lib/ui/command_bar.dart` | 分区面包屑 + 实时筛选胶囊 + 密度/专注/恢复/检查器操作簇 |
| **Spotlight 命令面板（`Ctrl+K`）** | `lib/ui/command_palette.dart` | 键盘驱动（`ArrowUp/Down/Enter/Esc`）跨卡片/页面/动作统一检索入口 |
| **三分栏可停靠检查器** | `lib/ui/inspector_drawer.dart` | 将原垂直长列表整合为 `工作台 / 音乐 / 外观` 三标签抽屉，新增 2×2 空间脉搏卡片 |
| **紧凑 Bento 头部与筛选条** | `lib/ui/workspace_header.dart` | 紧凑 Hero + 可点击分区统计胶囊 + 筛选/搜索 Chip/排序条 |
| **网格卡片与紧凑列表行** | `lib/ui/idea_card.dart` | 支持 `masonry`（1/2/3 列自适应）与 `compactList` 双视图切换 |
| **统一弹簧动效与分段控件** | `lib/ui/workbench_motion.dart` | `WorkbenchCurves.springOut/emphasized/snappy`、`SharedAxisSwitcher`、`SlidingSegmentBar` |
| **悬浮玻璃底栏** | `lib/little_tips.dart` | 居中悬浮胶囊，支持直达播放/暂停与歌词切换 |

### 附：可供 `0.1.x` 预构建分支独立评估复核的两项纯缺陷修复

主 Agent 在复盘本提案时，可独立评估是否将原型分支中的以下两处纯宿主层 bugfix cherry-pick 回 `0.1.x`（不涉及布局改版）：

1. **Flutter 3.50.0 Windows 液体玻璃背景倒置 / 反向滚动修复**（`shaders/liquid_glass.frag`）
   - **现象**：Flutter `3.50.0` 在 Windows 默认启用 Impeller OpenGLES 且移除旧版 OpenGLES 纹理 Y 轴翻转（`impellerc` 定义 `IMPELLER_OPENGLES_UNFLIPPED_DEPRECATED`），原 `#ifdef IMPELLER_TARGET_OPENGLES` 下无条件执行 `sampleUV.y = 1.0 - sampleUV.y;` 会导致液体玻璃采样背景上下颠倒、滚动反向。
   - **修复**：改为 `#if defined(IMPELLER_TARGET_OPENGLES) && !defined(IMPELLER_OPENGLES_UNFLIPPED_DEPRECATED)`，实机 Windows 编译产物的反编译着色器已验证消除 `1.0 - _167.y` 翻转。
2. **Windows `ui::AXTree` 语义树更新报错收敛**（`lib/appearance.dart`、`lib/collapsible_panel.dart`、`lib/workspace_viewport.dart` 等）
   - **现象**：过渡动画中的 `FadeTransition` / `Opacity` 缺少 `alwaysIncludeSemantics: true`，折叠面板在 `ExcludeSemantics` 状态下仍保持 `TickerMode` 激活，以及动画中途切换相同 `ValueKey` 子树，均会触发 `Failed to update ui::AXTree, error: ... will not be in the tree and is not the new root`。
   - **修复**：补充 `alwaysIncludeSemantics: true`、在折叠隐藏态关闭 `TickerMode`，并消除动画跨阈值重建子树（注：底层框架侧完整修复仍依赖仓库 `tool/flutter_windows_semantics/` 提供的隔离补丁）。

---

## 4. `LayoutSpec v1` 声明式协议草案（供 0.2.x 设计参照）

建议复用 `theme.describe` 的纯转换通道（`handler: "layout.describe"`, `output_type: "morrow.ui.layout.v1"`，正式持久化/传输格式沿用 Cap'n Proto + 有界预算校验），与主题插件**正交独立启用**：

```json
{
  "schemaVersion": 1,
  "id": "org.morrow.layout.modern-workbench",
  "version": "1.0.0",
  "name": { "zh": "现代桌面工作台", "en": "Modern Desktop Workbench" },
  "navigation": { "desktopMode": "collapsibleRail", "mobileMode": "bottomDock", "defaultExpanded": true },
  "topBar": { "style": "commandBar", "commandPalette": true, "densityToggle": true, "focusToggle": true },
  "header": { "style": "compactBento", "quickStats": true },
  "inspector": {
    "mode": "tabbedDrawer",
    "tabs": [
      { "id": "studio", "modules": ["quickCapture", "daily", "workspacePulse"] },
      { "id": "music", "modules": ["musicPanel"] },
      { "id": "appearance", "modules": ["appearanceControls", "pluginSettings"] }
    ]
  },
  "cards": { "defaultView": "masonry", "maxColumns": 3 },
  "footer": { "style": "floatingCapsule" },
  "motion": { "preset": "spring" }
}
```

### 强约束与降级契约
- **预算上限**：单份 `LayoutSpec` 载荷 ≤ 16 KiB，嵌套层级与列表长度设硬上限；全部枚举字段走白名单校验。
- **未知字段策略**：未知可选字段静默忽略并退化为宿主默认值；必需字段缺失或类型非法直接拒绝加载。
- **响应式强制降级权归宿主**：即使插件声明桌面 3 列与侧栏停靠，窄屏（`< 760px`）仍由 Flutter 宿主强制切换为底部导航与独立 `SettingsSurface`，禁止插件锁死桌面视口导致移动端不可用。
- **材质 `componentId` 稳定兼容**：继续复用 `navigation`、`corner-tips`、`search`、`hero`、`quick-capture`、`appearance`、`daily`、`music`、`footer`、`card:<id>` 等既有稳定标识，保证用户已有“每组件独立玻璃/材质设置”跨布局无缝继承。

---

## 5. 建议并入主规划线的阶段节奏

1. **阶段 A（`0.1.x` 收尾期 — 当前）**：
   - 不改动 `0.1.x` 主线结构，保持 `ui-plugin/modern-workbench-layout` 独立冻结参照；
   - 主 Agent 复核第 3 节中的两项纯宿主 bugfix（`liquid_glass.frag` 与 `AXTree` 语义修复），按需单独合入 `0.1.x` 预览分支。
2. **阶段 B（`0.2.x` 早期 — 宿主布局抽象与纯声明式布局包）**：
   - 在 Flutter 宿主抽象 `WorkbenchLayoutSpec` 与内置注册表，将**经典原版布局**与**现代桌面工作台布局**同时纳入内置预设；
   - 基于纯转换通道接入 `layout.describe`（`morrow.ui.layout.v1`），实现 `.morrowplugin` 外部布局包的独立安装、启停、热切换与卸载无损回退，且与主题包正交组合。
3. **阶段 C（`0.3.x` — 声明式插槽扩展）**：
   - 依托 `UI2`（扩展节点集：Markdown/列表/选择器/进度/有界状态），允许第三方业务插件向 `LayoutSpec` 预留的标准插槽（如 `inspector.tabs`、`rail.widgets`、`commandPalette.actions`）注册受控扩展面板，并全程通过 Rust `Session(view, generation, revision, serial)` 执行事件校验与权限控制。

---

## 6. 供主 Agent 复盘的评审清单

- [ ] 确认 `0.2.x` 是否采用“**`LayoutSpec`（宏观外壳编排）与 `UI1/UI2`（局部插槽内容）正交分离**”路线，而非直接用 `UI1` 扩展表达工作台外壳。
- [ ] 确认 `theme.describe` 与未来 `layout.describe` 在插件管理界面及组合包内的**独立启用/正交叠加**交互语义。
- [ ] 复核原型分支 `ui-plugin/modern-workbench-layout` 中的 `shaders/liquid_glass.frag` Y 轴条件编译修复与 `AXTree` 语义稳定修复是否提前 cherry-pick 至 `0.1.x` 预览分支。
- [ ] 明确 `0.2.x` 阶段 `LayoutSpec` 传输契约是沿用 `theme.json` 有界 JSON 解析，还是在契约冻结时统一纳入 Cap'n Proto schema。

---

*提案撰写与归档：Gemini*
