# Windows 启动等待与切换跟进（2026-09-21）

上一轮启动界面提前显示，但仍在窗口初始化结束后才启动宿主，且内容恢复后直接替换应用根节点。本轮消除这段串行等待，并保留启动界面直到工作台首帧提交，再淡出遮盖。

## 实现

- 窗口初始化与 `RustWorkbench.open` 并行；`Future.wait` 等待两条分支收尾后才进入错误清理，窗口初始化失败时也不会遗留已打开的宿主。
- `WorkbenchStartup` 保持同一个根状态。内容与偏好完整恢复后才挂载正式工作台，在原启动界面下完成本地化、布局与首绘；不建立可写的空白内容库。
- 工作台就绪后进行 100 ms 遮盖淡出，工作台本体不做透明度动画。启动层从首帧使用透明合成路径，不增加最短停留时间或假进度。
- 揭示开始即释放工作台的输入、焦点与语义，不要求用户等动画结束。启动层随后完全移除，已恢复的工作台、编辑状态和 Navigator 不重新创建。
- 遵循系统减少动画设置；等待期间卸载、恢复失败和切换到恢复页仍可正常清理。
- 显式启动诊断补充最终揭示帧与过渡期间的构建/光栅化时长，区分“工作台首帧”和“遮盖完全消失”。普通启动不写这些诊断。

## 测量与边界

同机 Windows Release，各三次独立进程；1 张卡片、1 个附件的隔离诊断库，每次重新复制。未触碰用户原库，未清空系统/驱动缓存。前版证据为 `build/startup-window-material/summary.json`；最终版为 `build/startup-transition-final/summary.json`。

| 指标 | 前版中位数 | 本版中位数 | 本版范围 |
| --- | ---: | ---: | ---: |
| 创建进程至窗口可见 | 443 ms | 319 ms | 318–443 ms |
| Dart 入口至工作台首帧 | 818 ms | 706 ms | 701–816 ms |
| Dart 入口至遮盖完全移除 | 前版无过渡，直接切换 | 830 ms | 827–944 ms |

工作台首帧中位数提前约 112 ms（14%）。遮盖淡出与可交互阶段重叠，不能把 706 ms 宣称为动画已完全结束。一次宿主就绪发生约 110 ms 波动，已保留在范围中。过渡采样仍有约 18–22 ms 的 Raster 峰值，因此不宣称全程满足 60/120 Hz 帧预算。工作台首次 GPU 绘制仍约 220 ms，大库逐卡读取/附件导出也未在本轮重构。

初始 140 ms 淡出版本仅用于定位，证据为 `build/startup-transition-profile/summary.json`，不作为交付结果。

## 验证与交付

- 9 项 Flutter 回归通过：七语言窄窗、内容与主题恢复、无占位保存、根状态保留、未就绪时焦点隔离、动画期间可交互、减少动画、等待卸载、恢复页与设置路由。日志 `build/startup-transition-tests.log`。
- 本轮 4 个相关文件静态分析通过：`build/startup-transition-analyze.log`。
- Windows Release 重建通过：`build/startup-transition-build.log`；三次最终启动检查均退出 0，未发现 Flutter 渲染异常。
- 产物：`build/windows-corners/x64/runner/Release/morrow_studio.exe`，使用时保留整个 Release 目录。SHA-256：`build/startup-transition-artifact-hashes.json`。

版本保持 `0.1.9-test.52+56`。本轮仅修改 Dart 启动协调、诊断与相关测试，未改动 Rust 宿主/插件协议；未调用子代理，未推送或发布。
