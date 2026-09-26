# Drive 增量合并验收

日期：2026-09-27（工作自 2026-09-26 开始）。应用保持 `0.1.9-test.56+60`。

## 决定

此次检查点满足本地开发分支的增量合并条件：接纳 `807b9cd` 的 12 个提交，保留本地 Windows 构建与主题 1.0.1 检查点 `008db005`，携带本报告中的补齐与测试修正。目标为 `codex/io-safety-refactor`；没有更新 main、远端、标签或 Release。本地 Windows 成品用于本轮验收，不冒充已发布的 test.56 原件。

合并候选在 `build/drive-review-807b9cd` 独立工作树完成全部下述门禁后提交，再将开发分支快进至同一提交。旧工作树尚未提交的真实改动先保存为 `008db005ee4e161395f7a2960a5b2e1eee74533a`；备份补丁在原树 `build/drive-merge-preexisting-20260926.patch`。冲突仅涉及 `.gitattributes`、看板和插件状态文档，保留两侧换行规则、历史记录和开放项。

来源完整性及前轮测试详情见 [Drive 检查报告](drive-checkpoint-windows-review-2026-09-26.md)。其中“未合并”和待补项为前一轮历史状态，由本记录补充。

## 补齐与修正

- 文件任务面板移除 25 处二选一中英文文案；加入 26 个外置 ARB 条目，覆盖中、英、俄、法、德、西、日、韩、葡九语，含字节进度占位符。生成合并 ARB、Dart 消息及语言包，不更改语言回退策略。
- 新增面板测试，验证九语显示、取消选文件后保留原选择且不提交任务，以及切换语言后保留选择。
- 清理文件任务 manager、codec、session 中 9 条大括号静态检查提示。
- 修复 Python 子进程的 Windows 编码不一致，以及三处继承自共同基线的 Rust 测试 `catalog` 旧引用。
- C/C++ 原生测试程序在 Windows 将标准输入输出设为二进制，并兼容 MSVC 的 CRT 警告。未修改 SDK 核心、冻结原件或源码锁输入。
- 新增 `test/file_task_real_native_test.dart` 与 `tool/verify_file_task_windows.ps1`；脚本从仓库固定 Rust IO Wasm 临时构造 FileRead 测试包，不依赖未入库的本机专用包。

## 本轮门禁

| 项目 | 结果 | `build/local-review/` 日志 |
| --- | --- | --- |
| 文件任务及插件/HTTP/IO/服务 UI 组合 | 103 通过，无跳过 | `final-ui-regression.log` |
| 十个相关 Dart 文件静态分析 | 无问题，退出 0 | `final-file-analysis.log` |
| 独立 Dart 文件客户端/会话 | 28 通过，无跳过 | `dart-file-final.log` |
| 九语目录回归 | 7 通过 | `final-i18n-tests.log` |
| 九语生成产物 | 22 个一致 | `final-i18n-check.log` |
| 原生 C/C++ 编译 | Clang/Clang++，`-Werror` 通过 | `c-service-native-build.log`、`cpp-service-native-build.log` |
| 独立服务 codec，显式包含原 ignored 原生用例 | 4 通过，0 ignored | `native-service-codec-all.log` |
| 最终成品内容/保护存储/主题与升级 | 5 通过，无跳过 | `final-bundle-regression.log` |
| 最终成品 Dart→Windows 宿主文件任务 | 1 通过，无跳过 | `final-file-bundle.log`、`file-task-real-native-3d69f8f9.log` |
| 完整 Windows x64 Release | 构建成功 | `windows-build.log` |
| 实际 Release 应用自检 | 新临时库，退出 0；无 Flutter 错误 | `release-self-check.md` |
| 宿主绑定、SDK 契约、冻结传输清单 | 通过，17 固定原件未变 | `client-generated-check.log` 与终端结果 |

前轮同检查点的 Python 118、Rust SDK 66、宿主 lib 143、文件/runtime 37 及三语言服务原包 HTTP 6 项均已通过；本轮补跑了原先唯一忽略的原生 codec 用例。上述分组有重叠，不相加为完整产品通过率。宿主生产 Rust 路径本轮未改变。

真实文件验收使用 90,007 字节临时源：捕获后删除源文件，按 65,536/24,471 字节读取两块，对照精确 offset、EOF 和 SHA-256；Finished、worker 退出和 acknowledge 后回到 local；撤销 file-read 后再次启动被拒，待选文件保持原字节。所有操作只使用测试库。

实际应用自检覆盖原生合成 API、无声 WAV 解码及播放时钟、seek/互斥/恢复不自动播放，以及 Rust 工作台渲染。它不等于桌面像素对比或系统原生选择对话框的人工操作验收。

## 构建与兼容包

本轮 fresh 工作树构建两次等待外部依赖下载。停止经命令行核实的本轮构建子进程后，复用本机已有的相同依赖缓存：libmpv/ANGLE 对照依赖脚本固定摘要，剪贴板扩展对照相同 crate hash 目录并核对复制字节。没有修改上游校验或应用依赖版本。受影响尝试日志保留在 `windows-build-download-stalled.log`、`windows-build-cargokit-stalled.log`；最后完整构建成功。保留既有 5 条 Rust dead-code 警告和第三方 CMake 警告，不声称全仓无警告。

当前源码的 guest 和宿主均编译；安装包继续显式使用经 SHA-256 核验的兼容工作台原包，避免同版本不同字节替换。主题仍保留 1.0.1，使用既有原件完成最终成品上的升级回归。

成品目录：`build/windows/x64/runner/Release`（位于独立检查工作树）。必须保留 DLL、data、plugins 和宿主。

| 文件 | SHA-256 |
| --- | --- |
| `morrow_studio.exe` | `055fd91e7745ce0f721d6d78f41ba0f70578bb4b1cae46d5ea59a2b0530d0245` |
| `morrow-workbench-host.exe` | `eea61b415de87b84ca9fefd543586a3bf106e3227ffcad885b9a2db973a00629` |
| `plugins/workbench.morrowplugin` | `c71dc27866378894d5288053238fef9e0ef15f6b578449279bc681e7fe8b560e` |
| 原主题 `morrow-mid-autumn-1.0.1.morrowplugin` | `34230d83e4fb0d5883630973a6261e275cf0ca63a4d37e61f6b6d307b5dfacdd` |

文件任务可复现命令见 [文件任务合同](../docs/PLUGIN_FILE_TASKS.md#windows-合并验收与复现2026-09-27)。原生服务 codec 的复验需要先 `cargo build --offline --locked --manifest-path sdk/rust/Cargo.toml --target-dir build/local-review/target-sdk`，用 Clang/Clang++ 编译 `sdk/tests/c_service.c`、`sdk/tests/cpp_service.cpp` 并链接生成的 DLL import library；将两个 EXE 设为 `MORROW_SDK_SERVICE_NATIVE_C/CPP`，DLL 目录加入 PATH，然后运行 `cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --target-dir build/local-review/target-runtime --test sdk_service_codec -- --include-ignored`。

## 保留的产品边界

本轮合并不以完整 SDK 冻结为前提，也不声称已冻结。原生选择对话框人工验收、母语审校、完整目录/写入/删除能力、认证与流式网络、跨重启业务核对、完整 TLS/DPAPI 生命周期、Web/Android 与 CI 仍独立跟进。新的文件任务只在已有原生能力条件下出现，没有扩大 Web 能力声明。
