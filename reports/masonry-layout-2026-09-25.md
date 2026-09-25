# 宽屏双列瀑布流修复与 Windows 验收

交付：`build/masonry-v3.6-windows/Release/morrow_studio.exe`。请保留整个 Release 目录。旧交付目录和真实用户资料库未覆盖。

## 原因与修复

原瀑布流会用保留卡片的旧列号、旧纵向位置初始化两列起点。即使首张卡片已经回到索引 0，另一列仍可能沿用后面某张卡片的位置，因此重排或宽窄切换会出现空洞、连续卡片落在同一列的异常。之前仅清除被移动卡片的列号，没有清除未移动卡片及已回收前缀的旧布局历史。

增加独立坐标回归后，旧实现可复现两项失败：重排后的卡片列位置偏差 357 像素；深滚动重排后的纵向位置偏差 34 像素。证据：`build/masonry-before.log`。

- 首张卡片为索引 0 时，两列统一从零初始化，后续卡片按当前较短列排布。
- ID 顺序、列数或卡片宽度变化时，清除旧位置历史并重算；保留仍附着的 keyed Element/State。
- 内容未改变顺序的普通重建不会触发顺序缓存失效。
- 深滚动重排与宽度变化需要重新测量前缀，过期卡片每累计 32 张就回收，避免整个前缀同时常驻。此类重排仍有与前缀长度相关的计算成本；不是常数时间布局。

实现：`lib/stable_masonry_grid.dart`、`lib/render_stable_masonry_grid.dart`、`lib/workspace_viewport.dart`。渲染器基于 flutter_staggered_grid_view 0.7.0 做本地修补，保留原 MIT 授权；未修改全局 Pub 缓存。

## 验收

- 相关静态分析无问题：`build/masonry-analysis.log`。
- Flutter 回归 25 项全部通过：`build/masonry-regression.log`。包括坐标、筛选空结果、恢复列表、拖拽即时更新与保存失败回滚、卡片 tips、退出等待。
- Windows 实机 17 项全部通过：`build/masonry-windows.log`。覆盖连续 20 次重排、单双列反复切换、宽度导致的高度变化、深滚动重排、14000 像素跳转与返回、筛选、1/2/3/6 张卡片恢复、稳定卡片状态和 300/1000/10000 张卡片的按需加载。
- 坐标测试按独立的最短列算法核对每个已挂载卡片的横向位置、纵向位置及宽度；不是仅检查无异常或有无卡片。测试文件：`test/masonry_geometry_test.dart`；实机入口：`integration_test/masonry_windows_test.dart`。
- Release 构建通过，保留已有 Rust dead-code 警告：`build/masonry-release.log`。
- 最终包新库自检、上一版 v3.5.1 测试库副本启动通过，退出码均为 0，无 AXTree 或 Flutter 原生错误：`build/masonry-qualification.log`、`build/masonry-v3.6-qualification/`。
- 已目视检查最终包 `fresh.png`：宽屏两列卡片从相同起点排布。
- 最终包真实 WM_CLOSE 复验：内容服务约 68 ms 退出，应用约 104 ms 退出，退出码 0，服务无残留。测试使用隐藏启动，耗时是本机进程退出结果，不代表可见窗口隐藏动画耗时。证据：`window-close.json`。

测试集合存在交叉覆盖，数量不相加。发布脚本 `build/package-masonry-release.ps1` 替换本次编译的 UI data 和包含先前退出修复的运行器，逐文件验证其余原生运行库与 v3.1 业务包哈希，保留原资料库兼容性。证据中的 `native-bundle-hashes.json` 和 `ui-build-hash.json` 记录最终交付哈希。
