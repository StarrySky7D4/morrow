# Windows 差异对比与 tips 独立文本设置

## 对比对象与结论

对照任务：[添加中秋风格主题插件](codex://threads/01a0d6be-a8d5-77c1-82f0-eb15e642cbb0)。读取其最后完成回复及 `reports/component-color-fix-2026-09-25.md`，并对本地交付目录逐文件计算 SHA-256。

两个任务使用同一 `main` 工作区，并非两个需要 Git merge 的独立分支。当前源码同时包含主题、调色修复和右键菜单；已有未提交改动保持原样，没有重置或覆盖另一任务成果。

| 范围 | 链接任务 | 本任务及此前右键扩展 |
| --- | --- | --- |
| 主题 | 中秋主题加载/卸载、局部及完整材质覆盖 | 保留；tips 文本可在完整覆盖下编辑 |
| Windows 调色 | 关闭调色弹窗后再更新界面；稳定无障碍节点生命周期；主题启用时隐藏调色 | 保留这些修复，回归原调色与主题流程 |
| 右键 | 主题任务不以操作菜单为主 | 卡片、任务、音乐、快速记录、每日待办；新增侧栏提示独立入口 |
| tips | 内置多语言轮换文本 | 单独设置页编辑、每行一条、恢复默认、取消、失败重试与本地保存 |
| 交付 | `build/mid-autumn-v3.1-windows/Release` | 新 tips 交付使用 v3.1 原生依赖并更新界面资源 |

修改前二进制比较：v3.1 与 `build/windows/x64/runner/Release` 的文件名并集共 49 个，33 个相同、15 个内容不同，另有 1 个说明文件只在 v3.1。详细清单：`build/tips-prior-build-comparison.json`。

- Flutter 引擎相同：`61B77BC881F5C57AF5F3EEAC4A96FFD9FD87E35B77565F44C678DE105C92C2ED`。
- v3.1 界面 AOT：`10001C7CD04E4128A30DA82C73A0E38F2431AE43D62CB794507B9D07BC46D6DE`。
- 之前右键交付 AOT：`E9A96F69AE8C9C1AC5460746E59A5A607DEC15104DEFF3095E0798AB82E67139`。
- 两份业务插件包也不同：v3.1 为 `8C761DA2CCB9F464FDCAF72CB1DB93B42E903B345593AFBD76A6E24E831ED9F9`，标准构建目录为 `DDF2BBB78039F74A67D3E81528E098298161931462748B26F7316DFAEF1C262C`。原生 DLL、runner、host 亦有字节差异；字节不同不等于每项都有功能变化，无法从哈希推断具体代码差异。

因此新交付沿用 v3.1 的整套原生运行库与业务插件包，只使用当前源码重新生成的 Flutter 界面及资产，避免把不同业务包直接带入原资料库。本次没有改动业务插件协议、权限和版本。

## 使用方式

右键「侧栏提示」或底部 tips →「组件设置」，也可从外观的「组件设置」列表进入对应组件。

- 「默认提示文本」每行一条，按顺序轮换。最多 100 条、16000 字符，空行忽略。
- 点击应用后保存；取消或返回不提交草稿。
- 「恢复默认文本」后点击应用，恢复随界面语言变化的内置文案。全部清空再应用也恢复默认。
- 文本与材质独立。重置材质不会重置文本，完整主题覆盖时仍可修改文本。
- 只改文本不会触发 Rust 业务插件的材质保存；同时改材质时仍走原有材质提交流程。
- 音乐播放且开启歌词时优先显示歌词，切回 tips 后继续使用自己的文案。
- 保存失败留在编辑页，保留草稿及上次确认文本，可以重试。

数据保存在设备的 SharedPreferences 中，以资料库路径隔离；Web 使用当前浏览器的本地设置。它是独立的展示偏好，不发送云端，也不塞入旧 Rust 设置协议或业务资料库备份。旧设置没有此项时使用原内置文本。

## 验证

静态分析 7 个文件无问题；28 项 Flutter 回归通过，覆盖新文本流程、调色、主题/业务兼容、右键、音乐、响应式设置及原外观保存。日志：`build/tips-analysis.log`、`build/tips-regression.log`。

最后补充“文本单独提交不写材质”的保护后，相关静态分析及 5 项文本回归再次通过：`build/tips-final-analysis.log`、`build/tips-final-tests.log`。

真实 Windows 窗口下 6 项测试最终通过（5 项相同功能回归及 1 项真实 SharedPreferences 插件写入/重新读取），包含中文、emoji、清空恢复及测试键清理：`build/tips-windows-final.log`。这些测试数不与单元测试相加重复计数。

Release 重新构建成功：`build/tips-release-final.log`。既有 Rust dead-code 警告仍存在，不影响构建；相关 Dart 静态分析无问题。

## 最终交付与原生验收

客户端：`build/tips-v3.2-windows/Release/morrow_studio.exe`。关闭旧窗口后启动，保留整个 Release 目录，主题插件无需重装。

- 界面 AOT SHA-256：`1B0B872FE6FC8466C0A6FCEBDCF4AE4019218FF4673CDA56EF999E0516200DEF`。
- 逐文件校验原生依赖及业务包与 v3.1 相同：`build/tips-v3.2-qualification/native-bundle-hashes.json`。旧目录没有覆盖。
- 使用全新测试资料库，原生窗口合成、静音 WAV 实际解码/播放时钟、跳转、播放互斥、禁止自动播放、Rust 工作台渲染均通过，退出码 0。检查截图布局正常：`build/tips-v3.2-qualification/fresh.md`、`fresh.png`、`fresh-exit.txt`。
- 复制上一任务的 v3.1 验收资料库，在副本上运行新客户端的启动验证。宿主、存储、工作台首帧及过渡完成均成功，退出码 0，证据 `build/tips-v3.2-qualification/upgrade`、`upgrade-exit.txt`。这是测试库兼容验证，没有打开或修改用户真实资料库。
- 两次最终进程 stderr 均未出现 `AXTree` 或 `[ERROR:flutter/`。原调色无障碍修复的高频 MSAA 专项结论沿用上一报告，本次未重复宣称进行了该专项压力测试。

打包与验收脚本留在 `build/package-tips-release.ps1`；它要求新目标目录，并校验相同 Flutter 引擎及全部保留的原生文件。
