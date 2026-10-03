# Windows 受限 medium token 原产物复验（W10）

2026-10-03，以 Codex 默认 sandbox 运行已封存的 W05 原始 native fixtures：Clang 7、MSVC 7，实际 14 run exit0，build 0。测试均使用原 argv、cwd、exe/DLL/importlib、向量与普通合成数据；67 项保护输入前后哈希保持。四个 guard-page fixture 各报告 27 项拒绝检查。

直接使用每个实际 child process HANDLE 查询 token，14/14 均 `elevated=false`、`restricted=true`、integrity RID 8192。查询前后实际 GetExitCodeProcess 均为 STILL_ACTIVE 259，子 PID 与 driver 区分；没有以 parent token 代替 child 观察。独立复核 363 项检查通过。

此资格是受限 medium sandbox；普通桌面用户、Flutter/GUI/生产 host、Windows symlink 和整个 SDK 仍没有本轮运行证明。执行过程没有 token/权限/系统设置修改。原始 75 个输出由 TEMP 逐文件 SHA 核对复制至外置证据，TEMP 原件保留；复制过程权限不代表执行 token。

GUI 只读前置发现现有 Flutter 3.44/Dart 3.12 桌面 runtime 可用，但项目依赖配置及严格离线构建闭包未齐；Flutter cache 更新、media_kit CMake 下载与 Cargokit 非 offline pub get 入口仍需明确前置。Developer Mode 当前值 1 只是只读观察，未创建链接或运行 pub get/GUI，不能把 `--offline` 单参数等同于全部构建绝无网络。

v2 记录器 KeyError 在任何 native fixture 启动前；独立 reviewer 首轮错误要求 exit.txt 的 LF，而实际 Windows 原字节为 CRLF。两项记录器/审核失败保留，修正只在后续记录器/审核结果，不改任何测试输出。最终审核 SHA256 `a8c7431a179024e5e6d3082059e96f4361372e58ce8aca9b057cf93f0e95e5f0`。
