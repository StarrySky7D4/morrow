# dev.10 Markdown 与授权粘贴

日期：2026-10-05。版本 `0.1.0-hmos-dev.10` / `1000010`。继续追平 Windows/Flutter，当前仍是独立开发预览。

## 最终包与来源

- 未签名 ARM64/x64 HAP：`entry/build/default/outputs/default/entry-default-unsigned.hap`，23,545,787 字节，SHA-256 `1988A4A87CA28102F53CB1A748C94AD644E7B122AD3D0B7F08CE8F0FA0344C44`。
- 原生 ARM64 库 SHA-256 `AB0AC992BFBD41B59578360D9A69A6EDAAA567ACB29FBB6AF64A17B453EF7D99`；x64 `2FF21151E19F99F6485F11634E8DC5862C8867E8C4041C0EAFF4CC704EC25703`。
- API26 / SDK `26.0.0.105`；健康设备 `127.0.0.1:5557`，x64 Pura X View / 7.0.0.106，1320×2232 px、density 3 / 440 vp。覆盖安装成功、版本回读见 `device-final-install.log`、`bundle-final.json`。ARM64 仅编译，未签名发布和 ARM64 真机均未验收。
- 构建见 `rust-arm64-build.log`、`rust-x64-build.log`、`hap-final-build.log`。首轮组件方法 size 与 ArkUI 基类冲突，已更名后通过；早期 `hap-arkts-build.log` 仅是 ArkTS 集成检查，不代表最终原生库。保留既有 Function.bind / 控件 system capacity 警告。剪贴板静态权限提示仍存在，运行时通过 PasteButton 临时授权；没有申请受限 READ_PASTEBOARD 权限。
- 三参照工作树、当前 blob、原字节哈希与设计定位见 `upstream-audit.md`。共享快照和 `shared/reference.json` 没有整体替换。原 `capture::plain` / `safe_link` 直接复用；新 Markdown 解析采用固定 `pulldown-cmark = 0.13.4`，关闭默认功能，仅启用表格、任务列表与删除线扩展。[解析器官方文档](https://docs.rs/pulldown-cmark/0.13.4/pulldown_cmark/)

## 实现与边界

Rust 的 markdown / paste_plain 动作在任何业务 Store 读取或授权前返回，`effect=not_committed`。Markdown 投影为段落、标题、引用/列表层级、完整代码块、分隔线、表格和行内样式。原始 HTML 是可见文字。链接复用原安全规则，并拒绝控制字符、编码控制字符和凭据；附件地址不外部打开，图片不能读取 file/data/mailto。20,000 UTF-16 单位、深度32、事件8192、块2048、runs4096、单元格2048、投影512KiB及链接展开64KiB均有明确失败，原正文不被裁剪。此预算不代表所有20k复杂文档都会生成预览。

ArkTS 预览按完整正文/编辑会话隔离，150ms去抖、串行合并、丢弃旧回包，等待时清除旧文档；错误可显式重新生成。关闭预览独立于文字 journal。原生 Span 显示强调/删除线/行内代码/链接，代码块可复制，表格可横向滚动，列表续段保留对齐。远程图片只在点击读取后创建 Image，旧文档图片失败不能污染新文档。INTERNET 权限仅为这条显式图片通路声明。

粘贴使用可见的系统 PasteButton，只有 SUCCESS 才读取剪贴板；读取前后核对 change count，并逐条读取明确的纯文字类型。冻结目标 TextValue、选区和编辑会话，拒绝组合输入、目标漂移、恢复/清理窗口、字段超限及错误回复。Unicode、前导空格和末尾LF不做 trim；标题60、待办500、假设5000、结论10000、正文20000以当前控件的 UTF-16 长度核对，与 Flutter grapheme 字符计数仍有区别。正文 TSV 调原 capture::plain，预检全部500行/80列和完整输出，超限拒绝整次转换；其他字段保持原文。原生延迟光标恢复核对原 TextValue，新的文字/选区会取消旧恢复。

完整 HTML/Office/RTF、剪贴板图片/文件与持久附件导入尚未实现；有 HTML 的条目只插入可读纯文字并显示说明。跨多个块连续选择、原 IME 会话/方向 affinity、captured S1/S2、正式请求跨进程未知恢复、生产 HUKS/owner/封存仍未完成。[PasteButton 授权指南](https://github.com/openharmony/docs/blob/master/zh-cn/application-dev/basic-services/pasteboard/get-pastedata-permission-guidelines.md)

## 已执行验证

| 范围 | 结果与证据 |
|---|---|
| Windows 实际 Rust Engine | 36/36 PASS，`rust-host-tests.log`，包含原26项和9项投影/链接/预算/TSV专项、1项 Engine JSON 契约及业务/草稿不变检查 |
| Windows 实际 Engine runner | 28项 PASS，`rust-host-runner.log`，作为本地链接证明单独保留 |
| OHOS x64 runner | 最终源码28项 PASS，`native-final-self-check.log`；使用新隔离 fixture `/data/local/tmp/hmos-dev10-native-20261005a`，不读取应用或桌面正式库 |
| 实际 ArkTS 新模型 | 22/22 PASS，`arkts-preview-paste-tests.log`：14项预览协调和8项插入/UTF-16/选区/组合/漂移/拒绝测试 |
| 实际 ArkTS 既有模型 | 26/26 PASS，`arkts-existing-model-tests.log`：文字journal、查询协调和数据源；不等于真实IME或全主题渲染 |
| 最终 HAP 七项实际流程 | 7/7 PASS，`device/result.json`；实际原生显示标题/格式/列表/引用/代码/表格、HTML文字和本地图片占位；真实 PasteButton 将中文/emoji TSV完整替换选中正文；目标标题粘贴保持原文字；61单位标题拒绝且原值不变；已确认表格草稿重启恢复；正式保存一个业务身份并清理journal |
| 最终 HAP 只读查看 | 2/2 PASS，`device/readonly-result.json`、`markdown-readonly-device.log`；查看和切换已有业务卡正文预览不创建文字草稿，重启后仍无该journal。合计最终包9项实际流程 |

唯一新业务 fixture 为 `HMOS-markdown-20261005-A`，身份 `060b5cd8-de23-45e6-95a1-4571ddbf41a6`。复制与粘贴通过当前可见控件、Ctrl+A/Ctrl+C、系统按钮完成，不写应用数据库。截图 `device/formatted-preview.png`、`code-block-preview.png`、`code-table-preview.png`、`tsv-body.png`、`restored-table-preview.png`、`existing-readonly-preview.png` 与最终包同范围；已人工查看标题样式、列表续段、引用和表格截图。

验收辅助先后停在只包含可视内容的UI树、HTML末尾LF和被UI树省略的父容器三处断言；修改辅助后，先检查当前控件/已记录阶段再继续，没有重新seed或重发正式建卡请求。证据保留为 `markdown-device.log`、`markdown-device-continuation.log`、`markdown-paste-device.log`、`markdown-title-device.log`，阶段通过合并于最终result；失败本身不计通过。

本轮未执行宽屏分栏、远程图片网络读取/权限失败、外部浏览器/邮件处理器、真实IME候选、七风格完整矩阵、性能与ARM64真机。平台的显示参数检查只读，没有改模拟器密度、旋转锁或系统参数；旧版本宽屏截图不充作本轮验收。

仅提交并推送 `codex/ArkTsUI`，不合并/推送主线。后续仍需阅读详情/卡片菜单、每页自定义顺序、附件/媒体/富文本、正式宿主与平台保护等工作；完整持续目标保持开放。
