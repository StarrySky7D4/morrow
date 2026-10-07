# 已发布 UI 集成包的有限设备观察

2026-10-07。此目录记录提交 `eeca59f85227939449b72c45dae28d90545fcfd8` 的 UI 集成包，版本 **0.1.0-hmos-dev.19 /1000019**，未签名 HAP SHA256 **4769015976A302099D4CDC6ABB49DE3F7A836AE3559FED1AD6F56A677F6C460E**。运行环境为 Pura X View2 / HarmonyOS API26 x64 模拟器，屏幕1320×2232；不提供 ARM64 真机资格。本轮草稿 fork 源码在此包构建之后新增，不得借用这些设备证据。

安装前先明确保留原公开 C 草稿，观察原表格文字及 TSV 附件 pin 后关闭编辑器。记录见 [原请求与动作日志](preinstall/progress-clipboard.json)、[保留前](preinstall/preserve-c-before-integration-before.png)和[保留后](preinstall/preserve-c-before-integration-after.png)。这不证明安装后的 C 草稿重启恢复。

原包只安装一次，安装、版本读回和启动均得到成功回复，见 [安装动作记录](installation.json)、[安装原始输出](integration-install.log)、[包信息](bundle-after-install.json)和[启动输出](start-after-install.log)。原始 HAP 保存在 `.build/artifacts/dev20-integration-checkpoint/entry-default-unsigned.hap`；没有再次安装新 fork 包。

[安装后首页截图](current/integration-home-initial-observed.png)与[原树](current/integration-home-initial-observed.json)显示14张业务卡片、2份草稿；该视图未观察到此前的贯穿框线。此结论仅覆盖这张已检查的首页，不能扩大为所有主题、页面和尺寸。

在自有公开新卡草稿 `HMOS-todos-20261007-D` 输入正文 `Public Flutter todo UI fixture: 汉字 🧪 é.`，观察标题21/60、正文39/20000、零附件；没有提交业务 create。见 [文字输入截图](current/todos-d/own-todo-ui-new-body.png)与[动作记录](current/todos-d/progress-clipboard.json)。滚动后实际显示空待办区、`＋ 添加一条` 和0/1000，[实际截图](current/todos-d/own-todo-ui-add-row-seek-7.png)中未观察到贯穿框线；正文上部处于正常滚动裁剪范围。

添加行阶段的驱动误查 `'+'` 而不是实际按钮完整文案，在一次已确认滚动后停止，**没有点击添加行**。日志保留 `FAILED_OR_UNKNOWN` 和断言错误，未重放该阶段。当前 D 是未业务提交的公开草稿，不作多行输入、1～3行高度、拖动、原子保存或重启恢复的通过证据。系统 IME、全篇选择、图片双指、签名和完整 UI 对齐均继续 OPEN。
