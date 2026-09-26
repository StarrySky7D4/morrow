# HMOS dev.4：待办交互与参照进度

日期：2026-09-27（Asia/Shanghai）。版本 `0.1.0-hmos-dev.4`，独立目录 `hmos/`，分支 `codex/ArkTsUI`。

## 源码核对

- 实时读取指定参照任务：当前正在推进 Windows 文件 Create 的持久执行边界、临时写入和不覆盖发布。上游已报告 Core 715 / runtime 145 项通过，但这不是 HMOS 验证结果。
- 参照实际工作树 `build/io-safety-refactor`，观察 HEAD `b9225f64f6c62584ad7243e30249d8a088bcb155`，含在途修改。`reports/reference-drift.json` 记录相对原快照的 87 个变化路径；没有覆盖固定共享快照。
- 直接参考 `lib/versioned_task_panel.dart` 的 TaskId 菜单、重命名、上下移动、批量完成和确认流程。共享 `plugins/workbench/src/tasks_v2.rs` 与当前源文件 SHA-256 相同：`43849B828B2C6F4605EA8FE677CBB4E95A64FCFB68EEC4639346556E9947C70D`。
- 本目录 `flutter-reference.json` 仅记录当前源码观察。未重新渲染 Flutter；v3 并排图仍是历史证据。

## 本轮实现

- ArkTS 待办操作菜单接入重命名、上移、下移、移除；第一项上移和末项下移禁用。
- 重命名单独保留输入，确认写入后关闭；忙碌或结果未知时冻结操作并保留原请求。重命名期间其他提交被阻止，退出编辑器对未保存待办输入给出放弃确认。
- 排序发送全部 TaskId 的排列，Rust 直接调用既有 `Reorder`；不按名称匹配，不重建任务或丢弃未知字段。
- 批量完成和当前阶段通过既有 `CompleteAllAndSetStage` 原子提交，批量操作与移除均先确认。确认绑定打开时的卡片源记录，视图变化后不提交旧确认。
- 全部修改沿用完整源记录 CAS、操作 ID、历史回执核对；没有新增自动重放未知结果。
- 截图复核修正窄按钮默认内边距造成的符号裁切，勾选/菜单/加号现在完整可见；编辑弹窗增加底色，降低背景文字穿透，保留圆角与玻璃材质。

## 已执行验证

| 验证 | 结果 / 证据 |
|---|---|
| 主机 Rust 适配器 | 5 passed，0 failed；`../../dev4-rust-host.log` |
| 新增集成场景 | 同名任务仅按 ID 重命名；排序保持完成状态与阶段；重复/缺少/未知 ID 排列、空名称、未知 TaskId、类别不匹配阶段拒绝且不写入；过期 CAS 拒绝；三类操作重开后返回历史回执且保持当前视图；删除卡片拒绝编辑；Store 完整性通过 |
| OHOS ARM64 / x86_64 | release 构建通过；`../../dev4-rust-arm64.log`、`../../dev4-rust-x64.log` |
| HAP | ArkTS/C++ 编译、打包通过；`../../dev4-hap.log`，未配置签名 |
| 设备 Rust runner | API26 x86_64 模拟器独立夹具，10 项检查 PASS；`../../dev4-device-rust.log`，包含重命名、完成状态随排序保留、批量完成和重开后排序请求核对 |
| 实际 UI | 安装 dev.4，在新建 `HMOS-dev4-task-check` 卡片中添加两个 Duplicate；第一项独立完成，第二项改为 DuplicateRenamed，再上移。见 `task-menu.txt/png`、`rename.png`、`reordered.txt` |
| 确认取消 | 取消批量完成后仍是修订 6、第一项未完成；确认后修订 7、两项均完成；取消移除后两项保留且修订仍为 7。见 `complete-cancelled.txt`、`complete-confirm.png`、`completed.txt/png`、`remove-confirm.txt`、`remove-cancelled.txt` |
| 应用重启 | force-stop 后重新启动，再打开测试卡片，名称、顺序、两项完成状态与待整理阶段均保持；`reopened.txt/png` |
| 最终视觉修正包 | 再次构建、安装、启动；读回同一卡片并核对勾选/菜单/加号完整显示与正文对比度，第一项菜单上移禁用、下移可用。最终包截图为 `final-editor.png`，布局为 `final-editor.txt`、`final-menu.txt` |

除 `final-*` 外的本轮截图属于视觉修正前的功能验证构建；最终更改仅涉及这三个窄按钮内边距与弹窗底色。最终包哈希与 258 个构建输入由 `reports/build-manifest.json` 记录，历史截图不冒充最终包截图。

`dev4-device-rust.log` 的 Rust `std::env::consts::OS` 输出为 `linux`；该二进制实际由 `x86_64-unknown-linux-ohos` 构建并通过 HDC 在 HarmonyOS 模拟器运行，不代表通用 Linux 产品资格。

## 未关闭项

开发适配器仍使用未封存试验数据库。HUKS、正式宿主、持久草稿与跨重启 Unknown 恢复、附件媒体、文件 IO 和服务 SDK 资格未完成。新的 Flutter 视觉风格、材质跟随及深度参数已发现，未在这一轮移植；拖动排序、复制菜单、动态多语文案、完整 UI 对齐、宽屏和 ARM64 真机仍未验收。测试卡片与设备 runner 夹具保留供复查。

本轮推送前远端 `main` 为 `d9c043191a400df972832d398b80cb73dfc51f56`；仅更新 `codex/ArkTsUI`，不合并主线。
