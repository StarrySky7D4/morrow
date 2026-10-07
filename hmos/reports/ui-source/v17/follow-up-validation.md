# dev.17 系统文字粘贴后续记录

2026-10-07。此记录属于已归档 dev.17 包的后续设备观察，不修改发布时 `validation.md` 的 NOT_RUN，也不计作 dev.18 设备验收。完整 Flutter 对齐保持 **OPEN**。

## 包与设备绑定

安装来源为 `.build/artifacts/dev17/entry-default-unsigned.hap`，27,815,034 bytes，SHA-256 `D9DECC46BB0E953BB56A4CDC5DBBB71D0B863E6380C273CC677B73C36689C03F`。实际安装命令、成功输出见 [安装日志](device-final-install.log)；[bundle 读回](bundle-final.json) 为 `0.1.0-hmos-dev.17 / 1000017`。绑定是宿主归档完整哈希、安装日志和设备版本读回，不是从已安装 HAP 重新取得字节哈希。

目标为现有 Pura X View2 x86_64 模拟设备 `127.0.0.1:5555`，HarmonyOS 7 / API26，竖屏 1320×2232。未清除应用数据、重置设备或重建旧测试卡。升级前关闭现有媒体预览并保留 A 草稿，见 `device-before-upgrade.log`、`device-close-before-upgrade.log`、`device-retain-before-upgrade.log`。首次启动截图处于启动过渡，只以 [稳定主页](device-clipboard/final17-stable-before-clipboard-observed.png) 作为后续视觉观察。

## 已完成的限定检查

本轮只新建一次自有草稿 `HMOS-clipboard-20261007-C`，尚未提交业务卡片。逐阶段命令 INTENT / ACK、失败和只读核对均保留在 [原始操作记录](device-clipboard/progress-clipboard.json)；驱动源码为 `hmos/tool/clipboard-device-check.cjs`。

| 检查 | 结果与边界 | 证据 |
| --- | --- | --- |
| 系统普通文字复制 → 授权粘贴至正文 | PASS：从实际 TextInput 复制 C 标题，通过系统 PasteButton 粘贴；正文读回完全相同，无附件 | [首次驱动日志](device-plain-paste-C.log)、[不重放粘贴的核对](device-reconcile-plain-C.log)、[正文截图](device-clipboard/reconcile-plain-C-without-paste-replay-input-readback.png) |
| 系统 TSV 文字复制 → 表格转换 | PASS：实际 TextArea Ctrl+C 提供 `text/plain` 的 `Name\tValue\nAlpha\t123`，一次 PasteButton 授权后转换为完整 Markdown 表格 | [粘贴日志](device-TSV-paste-C.log)、[正文读回](device-TSV-body-readback-C.log) |
| 原始 TSV 附件 pin | 限定 PASS：UI 显示确认 1/1、`clipboard-record-1.tsv` / 20 B，并持续读回同一个 asset ID；尚未通过导出重新核验存储字节 | [附件与正文截图](device-clipboard/readback-TSV-body-C-raw-text.png)、[原始树](device-clipboard/readback-TSV-body-C-raw-text.raw.json) |
| 打开自有 TSV 的系统保存选择器 | PASS：观察到系统文件名输入框及确认控件；保存未确认 | [打开选择器](device-open-TSV-export-C-final.log) |
| 选择 Download 并导出核验 | **UNKNOWN / 未完成**：一次“我的手机”点击后未观察到 Download，驱动停止；随后只读检查仍在系统保存选择器。未猜测目的地、确认保存或重放导出 | [停止日志](device-export-Download-C.log)、[随后检查](device-export-phone-inspection-C.log) |

转换正文的精确读回为：

```markdown
| Name | Value |
| --- | --- |
| Alpha | 123 |
```

系统复制源的完整 UTF-8 为 20 bytes，SHA-256 `0912BF6C81B9669689FA2B868C500B47546216C5E2F77FA3449D6A6B9931E0CA`。确认的附件 ID 为 `asset-draft-import-6463f31711569c095b0b95e27d75a3081fc3816eb67002222634a1cdd3decc98`。这些源身份与 UI pin 读回不替代存储原件的独立导出哈希证明。

首次普通文字驱动在成功粘贴后读取已被预览替换的 TextArea，因节点不存在而停止；随后只核对现有结果、切回输入态，未再发粘贴请求，读回成功。首次打开导出在点击前因错误控件 ID 断言失败；重新读取实际控件后才打开一次选择器。两次驱动失败均保留，不能归为产品转换失败或删除失败证据。

## 尚未验证

C 的保留后跨进程恢复、业务保存及业务 ID、导出原件完整字节、附件移除均 **NOT_RUN**。当前操作记录停在未确认的系统保存选择器；没有产生拟定的 `HMOS-dev17-clipboard-C-original.tsv` 导出结果。HTML/RTF/Spreadsheet XML、Office/第三方提供者、系统二进制图/文件、PixelMap、容量边界、权限变化/后台与 Unknown 持久核对仍 **NOT_RUN**。本记录不覆盖 dev.18 的新容量策略或图片手势，亦不代表签名、ARM64 真机、HUKS 或完整产品验收。
