# v33 独立分支交付审查

2026-10-09，审查结论：**PASS_SCOPED_BRANCH_ONLY_PUBLICATION**。未发现本轮限定分支交付的阻断项；仅可交付 `codex/ArkTsUI`，不授权合并 `main`。`releaseEligible=false`，完整产品验收与 Flutter/Windows 对齐仍 **OPEN**。结构化逐项核对见 [release-review.json](release-review.json)。

本审查只读产品、测试、构建副本和不可变 HAP，写入本两份审查报告；没有修改实现、重跑 SDK、安装设备或进行 Git 写入。最终远端身份、暂存范围、提交和推送回执仍由 Root 核对，本报告不是已推送证明。

## 实际来源与包证明

独立逐项检查当前实际字节及 SHA256：**368/368 仓库构建输入、325/325 复制输入、169/169 模型输入、283/283 Native 输入**全部匹配，模型输入前后完全相同。副本中声明来自仓库的文件也与当前源逐一一致；当前 `build-manifest.json` 的368项来源和冻结包身份吻合。

完整回归日志实际尾计数为 **1291/1291 PASS**，53测试文件，fail/cancelled/skipped/todo均0。四个新增 Task、外观模型、Index外观和任务九语测试文件均被完整 runner 收录。日志 SHA256 `AE9AA11882ABE2731DF73FAA7527EEB764F9ABB9C87A5A48A56D9597D175B7E0` 与报告一致。这里是核对最终冻结执行证据，不是另一次新增1291项测试。

SDK日志实际 `CompileArkTS` 完成、**BUILD SUCCESSFUL /36.467秒**、34任务全部执行、0 up-to-date；日志 SHA256 `497BB606D17FCEA562CAFB44AC9C47ADA91934D3F16F80085A15F818ACF07792` 匹配。签名明确跳过，保留平台能力及异常处理等警告，不据此声称所有设备支持。

不可变 HAP：**31,564,598字节**，SHA256 `39F2E8A11E9C03A5AECF475539AE864BCA0609D97F48ABFFF4F65DD266545F88`；直接读取其 `module.json` 确认 `dev.morrow.hmos`、**dev23/1000023**、SDK26.0.0.105。包 unsigned、未安装。

直接读取最终 Zip 与实际编译缓存，确认八个音乐模块、`RecessedGlassRelief`、`AppearancePreferences`、`Index`，共 **11项**，均出现在实际 filesInfo、有非空 `.ts`/`.protoBin` 输出，并有包内 ABC 的准确 record。22个输出的实际字节/hash与包证明一致。包内 ABC为2,990,540字节，SHA256 `F6E62E4FF945A648EC20AAE4D06B8681588E1F46939DB42A29926E16C313D66B`。

四个包内共享库与本次实际 strip 输出逐项一致。ARM64/x64的 `libmorrow.so` 分别为 `A192359D…6754` / `4147A7A0…3ED2`，不能沿用旧 HAP 的共享库 hash。两个 v29静态库、production副本和本次构建副本完整字节一致；这是精确复用，**不是新Rust构建或测试资格**。

## 实现审查

Task completion=2 没有普通 Checkbox；详情和现有卡编辑器均有两个明确方向及对应菜单，0/1使用实际 bool 回调。原快照/source/revision、唯一 TaskId、文本/状态、前台、页面或编辑器归属及 input epoch约束请求；实际 raw flush后再次核对。重复动作不能替换已有 pending literal。task_toggle的每次发送/接收绑定当前 owner，迟到结果不能装入 cards或清除原请求。严格 committed、完整当前DTO与正规范u64 receipt检查不经 JS Number；历史receipt可接纳 Native最新视图，未知或坏回执保留原wire。

实际 ArkData接入仅将成功 `hasSync=false` 认作缺失；现有空串、非string、坏值或读失败禁止保存。已确认current、候选、原raw、观测值和已派发Unknown分开；保存要求原字节preflight、put、flush、exact readback全部确认。未知JSON值保留原literal，包括大整数与嵌套值，材质未知字段按stable id归属；未声称整个JSON全文逐字保持。

相同namespace的live owner保留至真实Promise终止，dispose不假取消在途read/put/flush/readback。新实例不能并发读取认定成功、写入或显式恢复；terminal Unknown跨实例保留，须显式recover。恢复只核对观察值，Unknown还需flush ACK及准确读回；不put候选、不回滚、不自动重放。旧ACK不能覆盖新的UI预览。

独立再次执行 `preservation.verify()`，只逆转声明的外观patch即重现完整Task冻结Index `619592C0…86664`，证明其他Task/business/music/paint来源保留。最终Index为356,113字节 / `57A9E5AA…5C4A8`，最终模型22,653字节 / `849ABCE1…F41E5`。61项外观模型及32项Index专项的当前来源、参考、前后清单和日志另行核对通过，**不与完整1291重复相加**。

旧跨实例flush覆盖新值的失败复现仍保留；修复后的61项与额外六向量独立证据另立。Task专项126项及早期九语12项对应各自采样时的Index，不能当最终整个Index未变的证明；最终完整回归重新执行其实际测试文件。

## 验证边界

旧 **v32/dev22** 七风格设备画面只授予所访问旧包页面的观察范围，不授予本次dev23新Task或外观存储设备资格。新包实际Task按钮/glyph/窄屏/Unknown/前后台/重启，以及真实ArkData顺序、flush、读回、重启和断电均 **NOT_RUN**。Task pending与外观namespace屏障仅同进程，不是新持久任务intent journal、跨进程CAS或崩溃journal。

真实音乐picker/grant/导入/声音/seek/EOF/后台/重启仍OPEN；字体导入只有源/SDK设计，未接入产品。全页面框线/像素矩阵、第二外侧光阴影和父容器裁剪、精确blur/density/theme/width/动画/光学效果、ARM64运行、签名/protected及完整Flutter/Windows对齐均 **OPEN**。
