# 下一批 UI / 实现差距审查 — 2026-10-07

状态：**NEXT_PROPOSAL_ONLY / FULL_GOAL_OPEN**。本报告只读审查 Flutter、当前 HMOS 与已安装 SDK，提出下一版本的完整工作块；没有实现或运行下述新功能，没有操作设备。当前包的设备测试由 root 独占，本报告不能替代其验收。

建议下一版优先交付 **结构化剪贴板粘贴的完整本地生命周期**：同一次用户粘贴读取多种实际格式，转换正文，保留原始文件与嵌入图片，经已有验证/导入/草稿持久化链路保存，保存后及重启后仍能预览、导出、删除。这不是继续增加一个纯文本转换入口。HTML、RTF、表格、PixelMap、合法文件 URI 和 Office 原始附件都要在同一能力矩阵中得到实际结果；提供者不提供或平台不能读取的格式必须记录具体缺口，不能计为完整粘贴通过。

## 新鲜参考与跟进范围

观察到 HMOS 根工作树 `codex/ArkTsUI` HEAD 为 `ed9a68c86e001322c79273401e800a8106f7f848`。本次未 fetch、提交或推送。

| 本地 Flutter 参考 | HEAD |
| --- | --- |
| `build/io-safety-refactor` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` |
| `build/win-cloud-20261005` | `e83cdf1d3506b001d07ffceaaa9f98b688bcc150` |
| `build/windows-sdk-reconstruction` | `20669f671152972470340a65eac3458dd2f61b4d` |

三棵工作树中下列六个文件的 scoped status 均为空，当前工作文件 Git blob 一致；这不表示整棵工作树干净或远端同步。

| 文件 | 共同 Git blob |
| --- | --- |
| `lib/attachments/clipboard_import.dart` | `f23ff12b4446ac0e984ea04aab8424be21a018ab` |
| `lib/attachments/office_clipboard.dart` | `24f54bee34f7a1902a0cf45811e1d35cd07d66a8` |
| `lib/content/rich_content.dart` | `3c53039a1a863ea92d34d8b1e5ec4f03a4938044` |
| `lib/content/idea_markdown.dart` | `165a6f789755db519ca567ca847ce961f9cc1add` |
| `lib/plugins/capture_native.dart` | `7d5f158bea809e4e07f1d0f1ca4a031a15101746` |
| `windows/runner/office_clipboard.cpp` | `e0952e886cbcd724c85c7ed451b19f991b870adf` |

对源线程 `01a085bd-7a94-7f93-8a1f-1ecf417f5ee3` 使用一次 `wait_threads(timeoutMs: 0)`，取得 revision **5**、cursor `652321f3-8157-4672-ab76-c1c1442eb0fe:5`。线程仍 active，当前 turn `01a1147f-71bf-7b82-b5ad-a35161a025f6` 尚未结束；最新进度是修正重复打开插件会话可能绕过旧会话不确定状态的连接限制，并补回归用例。该快照没有报告新的完成测试数或 UI 提交，不应推定修复已完成。没有向该线程发消息。

以下 Flutter 行号均以 `build/io-safety-refactor` 为准； SDK 行号来自本机 API 26 声明，根目录为 `C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets`。

## 三项候选的实际差距

| 工作块 | 实际 Flutter / SDK 证据 | 当前 HMOS | 判断 |
| --- | --- | --- | --- |
| HTML / Office / 图片 / 文件粘贴 | `clipboard_import.dart:153–335` 读取文件 URI、plain、HTML、下载文件、RTF 和 Windows Office 补充；HTML 保留原件并转 Markdown、抽离内嵌图片；`main.dart:5598–5678` 插入后导入附件，图片单独粘贴会追加持久附件链接 | `Index.ets:469–528` 只请求 plain/HTML 两种类型，真正读取的只有 plain；HTML 仅触发警告，图片和文件不导入。`EditorPaste.ets` 只是带选择/IME 检查的字符串替换 | 用户可见能力差距大；已有 Rust 转换和附件生命周期可复用，应优先做完整链路 |
| 全篇连续选择 / 复制 | Flutter `idea_markdown.dart:24` 设置 selectable，但实际依赖 `flutter_markdown_plus-1.0.12/lib/src/builder.dart:997–1027` 为各文字块构造 `SelectableText.rich`；正文没有包在 `SelectionArea` 内。只在两个服务配置页面找到 `SelectionArea` | `MarkdownPreview.ets:42–54,87–126` 每块文字/代码/表格单元分别为 Text 并设置 copyOption；没有跨块选择模型 | 跨段、表格、图片和滚动后的连续选择仍需整体设计。Flutter 目前源码也不能证明这一能力已经成立；不能把 selectable:true 当成全篇选择验收 |
| 图片缩放 / 拖动 | `attachment_view.dart:199` 使用 InteractiveViewer；已安装 Flutter `C:/flutter/packages/flutter/lib/src/widgets/interactive_viewer.dart:77–78` 默认范围 0.8–2.5 | `AttachmentImagePreview.ets:42–109` 已包含 contain 几何、边界限制、双指缩放并保持焦点、单指拖动、双击复位；旧回调受 token/epoch 保护，按钮也用相同缩放范围 | 这里主要是实际设备手势、解码/旋转/换源资格差距，不是缺少 pinch/pan 实现。不宜重复实现后宣称完成 |

对连续选择，SDK `component/rich_editor.d.ts:2259,2407,2494` 确实有 setSelection、addImageSpan、addBuilderSpan，但这不能证明 BuilderSpan 中表格的子 Text 也进入同一文字选择/复制范围。该 SDK `:2998` 已明确一个文字实体功能不适用于 BuilderSpan 节点文字。若后续使用 RichEditor，应先做跨表格/图片的最小实际设备验证，再设计全篇偏移映射、复制顺序、跨屏手柄和链接点击冲突；不能只补“复制全文”按钮替代连续选择。

## 粘贴目标中的复用与平台边界

Flutter 的目标包含两层：本地可见内容/附件，以及捕获来源、票据和编辑谱系。`capture_native.dart:14–167` 用真正的 HTML DOM 和 XML parser 做惰性解析，拆出 data:image 字节，把节点送 Rust guest，并返回 capture ticket。节点最多 1024、源字符串最多 2 Mi UTF-16 单元、Cap'n Proto 帧最多 65536 字节；这些预算会抛出异常，上层保留原件并警告。递归深度超过 40 时却直接停止访问子树，因此此边界本身不能作为全部内容已转换的证据。引用这些源文件不等于 HMOS 已拥有同样 host 能力。

| 能力 | 可复用部分 | 仍须新增 / 实际验证 |
| --- | --- | --- |
| plain / TSV | `hmos/shared/plugins/workbench/src/capture.rs:71` 的 plain；现有 `hmos/rust/src/markdown.rs:158` 已复用 TSV 转换 | TSV 转换时保留 `.tsv` 原件；多记录顺序、每字段目标与保存结果要统一 |
| HTML | `capture.rs:108` 的 html、safe_link 和表格/样式转换；`lib.rs` 已公开 capture 模块 | HMOS 没有 HTML DOM parser。需要独立 host 解析器产生 Node，抽离 data:image；不能通过正则去标签或用 WebView 执行内容代替。原始 `.html` 必须保留；转换失败时有明确 fallback 和警告 |
| Excel XML / RTF | `capture.rs:302` 的 spreadsheet（值、公式文本、Index）与 `:352` 的 rtf | XML 需真实 namespace-aware parser 并禁用外部实体；RTF 原字节/原件与文本解码须分开，不能将所有 ArrayBuffer 直接按 UTF-8 解码。保留 XML 的合并/格式、RTF 的对象/图片原信息 |
| 图片 | 现有 VerifiedPreview / Markdown 附件图片 / image gesture；现有 FD 流式 SHA/长度校验 | SDK pasteboard 读取 PixelMap；受限 PNG 编码；原始二进制 MIME 有则优先保留原件，PixelMap 只有光栅数据时不能虚构原文件/GIF 动画 |
| 文件 / Office 文件 | `AttachmentFiles` 的 spool 验证、native.prepareFile/importFile、ImportSelection 的逐项确认和草稿 pin | 剪贴板的文件权限来源必须单独验证。不能把任意 text/uri 填进 picker grant 集合；合法文件来自系统快照并在有效期内实际打开，raw URI/FD 不作为持久重试来源 |
| Windows Office 对象 | 惰性 RTF/XML 转换逻辑与原始附件保存方式 | Flutter `windows/runner/office_clipboard.cpp:85–136,171–225` 使用 OleGetClipboard、IStorage/IStream、CF_ENHMETAFILE、GDI+，可保存 doc/docx/xls/xlsx/ppt/pptx/ole/emf/PNG。HMOS SDK 的通用 pasteboard 没有证明它能产生这些 Windows 格式；需要实际 Office 提供者的 HTML/RTF/XML/自定义 MIME/合法 URI 能力，不能称 plain 或一张 PNG 与可编辑对象等价 |
| capture ticket / lineage | frozen shared schema/converter可做协议参考 | `hmos/rust/src/lib.rs:1–21` 明示 development adapter；`editor_draft.rs:153` / `editor_draft_staging.rs:614` 拒绝 captured lineage。没有已资格化的 HMOS capture host/HUKS/audit transport，不能捏造 ticket、复用 raw draft 身份当捕获凭据或关闭此差距 |

Flutter `clipboard_import.dart:58–92,129–140` 的文件读流会检查实际字节，单文件及内存累计上限为 200 MiB，并有 30 秒下载超时。HMOS 当前准备和 durable staging 的实际上限仍是 64 MiB，最多 20 个记录（`AttachmentFiles.ets:8–15`；`editor_draft_staging.rs:20–24`）。下一版本不能只提升 UI 的数字就称容量一致，所有存储/恢复/配额都需要一并更新后才能关闭 200 MiB 差距。

Flutter 各字段插入后以 grapheme characters 检查 60 / 1000 / 5000 / 10000 / 20000；当前 HMOS 粘贴按 UTF-16 单元且 todos 为 500。需要统一输入、原始编辑、保存校验和计数展示的语义，单改粘贴上限会与现有编辑/保存边界冲突。

## 建议的下一版本完整工作块

交付名称建议：**富内容与附件粘贴**。范围覆盖下面所有路径及其保存/恢复，不将 HTML-only 或文字转换子集称为该工作块完成。保护型 capture host/Windows OLE 原生来源和 200 MiB 容量是独立未完成的全目标条件，不因本地粘贴可用而关闭。

1. **建立一次用户授权的 PasteBatch。** 保持现有 `Index.ets:2661–2666` PasteButton SUCCESS 入口，冻结 editor/draft、edit epoch、目标字段、选择和 composition，禁止重复读取/导入。采样 getChangeCount，完整检查最多 20 条记录、类型、实际数据和附件展开后的剩余槽位；所有 await 后重检 owner。使用 `getValidTypes/getData` 获取非默认 HTML、URI、PixelMap 或已确认的自定义 MIME，不用 toPlainText 掩盖类型。通用系统内容读取与文件授权是两个边界：前者不证明任意 URI 可读。[OpenHarmony 安全控件文档](https://gitee.com/openharmony/docs/blob/7be2e47c073138318f0a1b96dfb5c79e3d4e5cd7/zh-cn/application-dev/security/AccessToken/security-component-overview.md?skip_mobile=true) 说明 PasteButton 通过用户点击授予剪贴板读取能力。

2. **完成提供者能力探测，再接入文件/Office。** 本机 `api/@ohos.pasteboard.d.ts:558–582` 支持记录多类型精确读取；`:1025–1067,1560–1575` 的 getDataWithProgress 可报告/取消文件处理，不支持文件夹，destUri 可省略由应用自行复制。设备 fixture 要分别证明本地和跨应用复制出的 URI 能被受权读取，以及 Office 提供者实际 MIME/值类型。不要预先声称 getData 返回的 URI 永久有效；不要把自动复制到 destUri 当作无上限且受现有配额约束的导入。文件引用打不开时保留明确错误，不能将字符串贴到正文后把文件粘贴标 PASS。

3. **新增 HMOS host 惰性富内容规范化。** 建议新增 `model/ClipboardInput.ets`（SDK 快照/临时能力与资源寿命）、`model/ClipboardPaste.ets`（批次/插入/附件链接计划），Rust 新增 `clipboard.rs` 或 `rich_content.rs`（原始 HTML/XML/RTF/源 MIME 校验及 Node 建树）。直接复用 frozen shared `capture::{plain,html,spreadsheet,rtf}`，不改 frozen shared 快照充当平台解析器。HTML/XML 解析依赖须单独选型并实际通过 Windows host 与 ARM64 NDK 编译；Dart 的 html/xml 包不能直接链接到现有 Rust adapter。明确源长度、节点、深度、表格跨度、输出和图片字节预算，移除可执行节点并拒绝危险链接；不打开外部实体、file URL、远程图片或任意网络请求。

4. **原始数据与图片进入同一受限存储链。** 原始 HTML/RTF/XML/TSV、可访问文件、原始图像 MIME 与 HTML 内嵌图片都先得到受批次拥有的能力，再生成正常 PreparedAttachment。已有 `prepareUri` 仍只消费 picker 一次性 grant；应抽取内部“已授权源 FD / 有界字节 → spool”共用层，两个入口各自封闭能力来源，不能新增接受调用者任意 path/URI/metadata 的公开 bypass。保持整个进程共享物理 spool/sidecar 配额，原件与图片也占附件槽位；每项重算剩余额度。二进制不通过 512 KiB JSON 请求复制或 base64 传输，使用已有 FD/NAPI worker 模式；较长原始文本需要单独 FD 解析接口，而不是扩大所有通用请求。

5. **正确编码 PixelMap 并保留生命周期。** 本机 `api/@ohos.multimedia.image.d.ts:4927,4974,12926` 支持尺寸/像素字节检查和 ImagePacker.packToData；用 PNG 保留透明度，explicit bufferSize 受剩余工作内存/编码预算限制，实际输出再经流式校验。`:3049–3059` 明确 packToFile 不受 bufferSize 限制，因此不能靠该选项保证写磁盘的 64 MiB 配额。编码 promise 未完成前不得 release PixelMap/packer/源 FD；过限、页面离开和晚回调都要有明确资源回收，超时也不能假装底层编码已经取消。

6. **提交可恢复的正文与附件结果。** 正文优先 HTML/表格 Markdown，非正文字段采用相应 plain；同一记录同时有 plain/HTML/图片时按 Flutter 源码优先级去重，不能粘出重复段落/双份图像。转换成功或失败都保留能够读取的原件及具体警告。所有转换先生成本地计划，在决定修改文本前检查选择/IME/字段预算；附件按原顺序复用 ImportSelection 的 operation/generation/receipt/pin 确认。嵌入图片使用局部标识，只有确认导入后才映射成持久 `attachment:` 引用，不能用随机 display name 碰撞既有附件。部分附件失败/Unknown 停止后续项，已确认项目与请求 journal 保留，不自动重复粘贴、重放 native import 或重新读取变化后的剪贴板。新增事务应只记录还缺少的批次/文本绑定，复用现有 import journals；重启不得依据 raw URI 恢复读取。

7. **覆盖整个编辑生命周期与可见反馈。** 图片单独粘贴、文件单独粘贴和 mixed batch 都可产生附件结果；正文中图片链接、文件名补空标题、草稿更新、取消/保存、重新打开、重启恢复、预览/导出/删除是一条链路。附件/文字部分成功时显示真实数量和逐项失败/保留状态，而不是“已粘贴”笼统成功；非默认字段行为保持 Flutter 插入目标规则。各字段的 grapheme/上限差距要整体决定并验证，不靠截断内容绕过。

## 下一版本的验证与完成条件

| 验证层 | 必须验证的具体内容 |
| --- | --- |
| Rust host + ARM64 原生构建 | HTML 不规则 DOM、实体/空白、标题/列表/代码/安全链接、合并表格、嵌入图像；XML namespace、稀疏 Index/公式/合并原件；RTF Unicode 与对象跳过；UTF-16/UTF-8/grapheme 预算；malformed、超深/超节点/超表格及大图均不得静默截断。转换结果与实际 Flutter fixture 比对，并检查原始附件完整 SHA/字节 |
| actual ETS 模型 | 多类型记录优先级/顺序、非默认 getData、20 记录和附件展开额度、sparse/重复/错误值类型、快照变化、缺 plain 的 HTML/image/file、序列失败/超时、持续配额；不授权 URI不得 open/native；一次性能力不可跨 helper/重复/废弃后使用；编码中离页与晚回调正确关闭；目标选区/IME/owner/generation 变化不插错文本；部分成功/Unknown 不丢 spool、不回放 |
| 本机 API 26 HAP | 新 parser/FD bridge/ArkTS 类型必须实际构建，manifest 核对对应新 HAP 输入，不能复用 dev.15 native 二进制后算新 Rust 实现通过 |
| 实际设备提供者 | 从实际浏览器复制 HTML（带表格/内嵌或图片引用）、文本应用复制 TSV、图像来源复制 PixelMap/原图、本地文件应用复制单个和多个文件、实际 Office 应用复制表格/选区/对象。分别记录 MIME 列表、读取值类型、可读 URI、原件文件 SHA、正文效果；假提供者 fixture 只能证明机制，不证明真实第三方应用互操作 |
| 完整用户流程 | 五目标字段与选择替换、中文/emoji/grapheme/IME；raw→preview 的完整显示；图片进入预览并进行 pinch/pan/复位；保存后重开、重启、附件来源失效后仍可预览/导出；取消/删除与 quota 回收；来源变化/后台/关页/解码错误/导入 Unknown/不足槽位及字节上限 |

完成记录应逐行区分 **IMPLEMENTED / MODEL_PASS / BUILD_PASS / DEVICE_PASS / PROVIDER_UNAVAILABLE / NOT_RUN / OPEN**，保留真实 Office 对象与 protected capture 的未完成项。不能以几个 HTML 字符串转换测试、PasteButton 单击成功或 303 个 dev.15 模型通过替代新链路验收。全篇连续选择、图片实际多点手势、200 MiB 容量、ARM64 真机/签名及完整 Flutter/Windows 功能资格仍属于全目标，均保持 **OPEN**。
