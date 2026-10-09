# v28：待办输入检查的显式恢复与分支交付

2026-10-09。基线 `e4891f89788339773f6b52f926da7215fe255afa`，只交付 `codex/ArkTsUI`，不并入主线。应用版本保持 `0.1.0-hmos-dev.19 /1000019`；v28 是源码和验证检查点，不是应用版本号。

## 变更

实际 `EditorTodosDraft` 保存失败行原已确认 old、完整 new 和已取得的剩余字数；失败时完整原文仍可进入 raw journal，业务保存继续拒绝。对应行增加“重新检查”，仅显式重复纯 formatter/count 检查，不重新捕获 raw、不生成或重放业务保存。重复按键、旧 owner/revision/version/incarnation、真实选区或原文变化均不能借用旧请求；迟到结果不能覆盖较新文字。

失败记录按行保留，另一行成功不能清除未通过的行或放行业务保存；失败行再次编辑仍沿用原已确认 old。独立复审发现重复完全相同的选区回声会撤销 retry，最终修复只对完整 aggregate 相同的回声作 no-op；真正选区变化仍撤销旧重试资格。Flutter 源码对照确认现有立即新增空行和全行共用1000 grapheme预算已存在，无另造待添加字段。

## 最终验证

- 实际全部 ETS/tool **922 /922 PASS**，0 fail/skip/cancel，36 个 suite 文件，30,246.7672 ms；126 项实际源码/测试/夹具输入前后相同。见 [最终模型结果](models-selection-final-result.json)、[原始日志](models-selection-final-tests.log)，日志 SHA256 `B4D868608FCA3E99EEF42968F22DCD9DD8C2F226764899BD23A81463052CD34C`。
- 最终四个相关 suite **161 /161 PASS**，22 项实际执行输入前后一致；19 项仅预读模块独立列出，不计执行资格。实际控制器方法与 pure worker receipt seam 保持明确。见 [待办源码审计](todo-source-parity-and-retry-audit.md)、[窄验证结果](editor-todos-retry-final-result.json)。
- 新的隔离完整 API26 未签名 debug HAP **SUCCESS /17.582 s**；34 个任务全部执行，315 项 SDK复制输入、381 项仓库输入前后逐字节一致。见 [SDK结果](sdk-final-result.json)、[原始构建日志](sdk-final-build.log)、[最终构建输入](build-inputs-final.json)、[源码复制](source-copy-manifest-final.json)。现有 SDK exceptions/capability warnings 保留在原日志；通过构建不解除设备能力边界。
- 最终包 **30,281,859 字节**，SHA256 `FF9C81704042FF053C2BCE6214E5AD56EDAD4A3906B2D66B06D2E9E273A1A2B5`。见 [包身份](artifact-final.json)。包内 ARM64/x64 的 `libmorrow.so` 和 `libc++_shared.so` 四项与该次构建输出逐字节一致，见 [包内库核对](native-package-check.json)。
- Rust/C++源码未改；279 项原生输入与 v27 证据相同，静态库复用 v27 已冻结双 ABI 字节，ARM64 `5329F277…`、x64 `EB390E7F…`。本轮**没有新跑 Rust 测试或原生 release 构建**，不能把 v27 的 191+3 或历史 crash 测试写成本轮新证据。见 [原生复用核对](native-reuse-inputs-final.json)。

## 设备与剩余范围

**v28 包未安装，新增待办失败/重试设备资格 NOT_RUN。** 本轮曾安装并启动不可变的 v27 包，在既有 API26/x64 模拟器中读回新建公开夹具的准确标题和正文；仅为 `PASS_SCOPED_VISIBLE_INPUT`。业务保存、退役和重启恢复尚未执行，完整界面框线/主题/宽度与 ARM64 真机未验收。全部原始动作、安装权限拒绝和有界结论独立保留，见 [v27限定设备观察](device/validation.md)；这些截图不计作 v28 新源码通过。

音乐目前仅完成[真实源码复用审计](music-parity-audit.md)，未新增音乐产品源/API、曲库或播放器；审计快照不当编译/运行资格。完整 Flutter/Windows UI与大部分功能对齐仍 **OPEN**。

## 阶段证据保留

`models-final-*` 的920通过和第一份 `build-inputs.json/source-copy-manifest.json/native-reuse-inputs.json` 对应重复选区修复之前的来源，不能给最终新源资格。初次窄测试 stage1 的夹具错误、source trace runner stage3 的只读 TypeScript export instrumentation 错误、pre-noop阶段均保留。首次 final prepare 引用不存在的 v27文件后在读取阶段失败，未启动SDK；错误 script 留在 `checkpoint-prepare-path-error.cjs`，修正为真实 `v27/native-build-inputs.json` 后 final prepare/build完成。历史脚本/日志不覆盖，正式接受数据只使用带 `selection-final`、`-final` 的上述最终记录。
