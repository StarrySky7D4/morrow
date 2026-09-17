# Morrow test.52 Windows 国际化预览验收

结果：**PASS_SCOPED**。版本 `0.1.9-test.52+56`，Windows x64 本地测试预览；未提交、推送、打标签或发布 GitHub Release。

当前界面提供跟随系统、简体中文和 English；每种语言 536 条类型化资源。覆盖工作台、创建／编辑、设置、颜色／玻璃面板、媒体、剪贴板／Office 提示、插件管理、内置文字工具和内容库恢复。用户正文、原始附件、歌词和第三方插件字面量保留原文。

## 验证证据

| 检查 | 本轮结果 |
| --- | --- |
| 完整应用回归 | 121 通过、13 跳过；缺少相应外部原生／浏览器或固定插件测试环境的项未冒充通过 |
| 最后增加的内置工具适配与插件管理回归 | 12/12 通过（含 4 个新增适配测试；不与完整套数相加） |
| 插件 UI 包 | 21/21 通过，包含待处理事件、输入法组合、草稿、原始插件消息跨语言保持 |
| 语言包／编译器 | 包测试 4/4、编译器 6/6 通过；坏包、长度、版本、缺词、参数及复数覆盖 |
| Rust 语言偏好专项 | 1/1 通过；无插件持久化、原操作重试、陈旧修订拒绝、重启读取 |
| 最终构建的真实 Rust／Flutter 集成 | 2/2 通过；包含 locale-only、嵌套配置提交前冻结、读快照防别名修改及备份恢复 |
| 整项目静态检查 | 最终 No issues found |
| Windows Release | 编译和封装成功；程序版本实际为 0.1.9-test.52+56 |
| 真实中英文启动 | 两次均退出 0；全新独立内容库，真实 Release 渲染无 Flutter 错误 |
| 随包媒体与窗口接口 | 静音 WAV 解码／时钟／定位／后台互斥通过，窗口模糊 0/1/12/40 接口调用通过；不等同桌面透明像素对比 |
| 产物与源码 | ZIP 的 287 个文件与实际启动目录及内置清单完全一致；976 个源码文件在编译前后及验收前核对通过 |

完整应用套完成后补入第一方文字工具显示适配，该增量已单独验证 12 项、重跑插件 UI 全包并通过最终静态检查及 Windows 构建。未据此宣称所有平台、完整插件 i18n/MessageRef、SDK 稳定或正式 IO 已完成。

Windows 两次资格运行分别位于 `build/i18n-preview-test52/zh-data` 与 `en-data`，未使用现有用户内容库。预览图片是实际 Release 截图；其中中文卡片标题是保留原文的合成测试内容。

## 交付

- [morrow-0.1.9-test.52-rust-workbench-windows.zip](C:/Users/Administrator/Desktop/CodeXProjext/morrow/dist/morrow-0.1.9-test.52-rust-workbench-windows.zip)（41,546,561 字节）
- [morrow-0.1.9-test.52-source.zip](C:/Users/Administrator/Desktop/CodeXProjext/morrow/dist/morrow-0.1.9-test.52-source.zip)（4,591,987 字节）
- [SHA-256 清单](C:/Users/Administrator/Desktop/CodeXProjext/morrow/dist/morrow-0.1.9-test.52-SHA256SUMS.txt)

```text
3eba9ec2c68ad0335970df4608d699bc050971aba9b60c726ff82f00b7db3105  morrow-0.1.9-test.52-rust-workbench-windows.zip
433458866db1f21dc8874ef74641ac31a6ee80c11485168b7a3c0d356b6a749e  morrow-0.1.9-test.52-source.zip
5df2e575d0b424e139f242fc38b8e2ca4e1fd0e464f716bd141a1ab0535d2b47  morrow-0.1.9-test.52-preview-zh.png
e5e95314cd16baaaf2de064e6596816bf6825d5092f087ce9f94ea9ce1195cab  morrow-0.1.9-test.52-preview-en.png
```

源码 ZIP 是实际未提交工作树的固定快照，基于 `0e93e49f56dd2534db5a0cd8fb31e9d5c705dfa7`，包含已验证但尚未推送的改动。它不引用不存在的 test.52 标签。此验收报告和截图由构建结果派生，生成于源码归档之后，作为独立交付附件。

## 日志与已解决问题

- `build/i18n-all-app-tests-final.log`：121 通过／13 跳过。
- `build/i18n-app-analyze-bundle.log`：最终整体静态检查。
- `build/i18n-builtin-tests-first.log`、`i18n-builtin-plugin-ui.log`：最后显示适配验证。
- `build/i18n-windows-preview-packaged.log`：最终宿主集成、Windows 编译、源码一致性及封装。
- `build/i18n-preview-test52/{zh,en}.md`：实际 Windows 资格运行输出。
- 修复英文窄屏／低高度布局溢出、标题栏 Tooltip 无 Overlay、异步语言加载下的首屏提示时序、旧测试固定中文及等待方式、配置嵌套对象异步保存别名、未确认语言变更的原操作重试。
- 首次封装使用系统旧 PowerShell，编译成功但缺少 GetRelativePath；其日志与产物保留在 `build/i18n-first-packaging-attempt`。最终改用 PowerShell 7 并增加版本前置检查，重新归档、编译与封装。压缩工具将少量依赖许可文件的早期时间戳规范至 1980；文件字节校验一致。

## 实际界面

![简体中文实际预览](C:/Users/Administrator/Desktop/CodeXProjext/morrow/dist/morrow-0.1.9-test.52-preview-zh.png)

![English actual preview](C:/Users/Administrator/Desktop/CodeXProjext/morrow/dist/morrow-0.1.9-test.52-preview-en.png)
