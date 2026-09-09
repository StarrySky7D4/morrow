# 液体玻璃与富内容灵感

## 使用

- 顶栏左侧箭头收起／展开侧边栏；设置按钮收起／展开设置区域。桌面横向、手机设置纵向过渡，状态自动保存。系统减少动画设置会取消过渡。
- 「玻璃质感」可选磨砂、超透、液体玻璃。「背景画布」四种背景选项下方有独立「液体玻璃效果」开关，默认、纯色、纹理、透明均可开启；切换背景类型时保留开关状态，且与面板玻璃材质独立。
- 三种玻璃材质互切使用 360 毫秒过渡，模糊、折射强度、遮罩、阴影和边缘高光同步变化；快速切换从当前画面继续，输入焦点和子控件状态保留。系统减少动画设置会直接切换。
- 新建／编辑灵感采用宽屏双栏编辑与预览，窄屏可在正文和预览之间切换。眼睛按钮控制预览。保存后详情直接渲染 Markdown。
- 在相应输入框按 Ctrl+V，或点击「粘贴内容」。正文优先接收可编辑 Markdown；标题、待办、实验字段接收文字。粘贴替换当前选区，不覆盖其他字段。
- 附件保存独立副本。取消编辑会清理本次新导入附件，不修改源文件。

## 剪贴板能力

| 来源 | 正文呈现 | 保留内容 |
| --- | --- | --- |
| 纯文本、链接、Markdown | 原文／Markdown 标题、列表、表格、代码、链接 | 正文 |
| HTML 富文本、Word 普通选区 | 标题、强调、列表、链接、表格 | 原始 HTML；可用时附加 RTF |
| Excel 单元格 | HTML 或制表符转换成表格；Windows XML Spreadsheet 可保留显示值及公式文字 | 原始 HTML／XML、可用的嵌入工作簿 |
| Word 公式、Excel 图表、PowerPoint 图形／SmartArt／幻灯片对象 | 来源提供位图或 EMF 时展示图片预览 | Windows EMF 矢量原件及来源提供的 OLE 嵌入对象 |
| 资源管理器文件、虚拟文件 | 图片／音视频可预览，其他文件作为附件 | 原始文件副本，含 Office、PDF、CAD、Blender 等 |
| HTML 内嵌图片 | 导入图片并用附件引用渲染 | PNG／JPEG／GIF／WebP 数据 |
| 仅提供 RTF | 文字、Unicode 和段落回退 | 原始 RTF |
| 不可识别的应用私有对象 | 提示没有可读内容或部分导入失败 | 需在源软件保存文件后导入 |

Windows 专用通道读取 Rich Text Format、XML Spreadsheet、CF_ENHMETAFILE 和 OLE Embedded Object／Embed Source。OLE 复合文档保留为 .doc/.xls/.ppt；含 OOXML Package 流时保存为 .docx/.xlsx/.pptx；不能识别的对象保留 .ole。不实例化、执行嵌入对象，不计算 Excel 公式。能否取得某种表示取决于来源软件实际提供的剪贴板格式，不承诺所有 Office 私有对象均可还原或在 Morrow 中编辑。

HTML 合并单元格转成普通阅读表格并保留占位；原始格式在附件中。复杂排版、图表、公式、SmartArt 的原生编辑仍需相应软件。浏览器只能读取浏览器允许的文本、HTML、图片和文件，不能访问 Windows OLE 通道。浏览器授权失败时使用 Ctrl+V 或导入文件。

外部 Markdown 图片需要点击加载；本地路径图片不会自动读取。正文渲染不执行 HTML 脚本；链接仅允许 HTTP(S)、mailto 和内部附件引用。

## 限制

每条记录最多 20 个附件；单文件／单次剪贴板文件总量 200 MB。HTML、XML 和普通文本读取上限 2 MB；原生 RTF 8 MB、EMF 16 MB、OLE 对象 64 MB。表格呈现最多 500 行、80 列，XML 最多 8 张表；超出的原始表格保留在附件中。正文最多 20,000 字符，标题最多 60 字符；粘贴不会绕过字段限制。读取期间剪贴板发生变化会要求重试，防止混合两次复制的内容。

## 材质实现

这是参考 Apple Liquid Glass 光学观感的跨平台实现。面板采用背景放大折射、轻微模糊、随指针移动的高光和多层边缘反光；编辑弹窗增加遮罩保证文字可读。支持 ImageFilter.shader 的 Impeller 后端使用圆角边缘逐像素折射；当前 Windows／Web 后端使用矩阵背景放大与高光回退。

默认、纯色、纹理画布开启液体效果时，对应用内背景叠加轻微放大折射、柔光和边缘高光。Windows 透明画布开启液体效果时仍保持原生透明模式，画布中央不绘制遮罩或背景滤镜，只绘制玻璃边缘高光。这样不会因 Aero 模糊或全画布着色而遮住桌面。Flutter 面板的局部折射只作用于应用内背景；不采样其他 Windows 窗口像素。Web 透明仅透出宿主网页，不透出系统桌面。

## 验证与复现

- 标准检查：flutter analyze、flutter test。
- 构建：flutter build windows --release、flutter build web --release --no-web-resources-cdn。
- Windows 原生内存测试：用 CMake 配置 tool/native_clipboard_test，构建并运行 morrow_clipboard_test。覆盖 COM 流、Office 复合存储、大小限制、EMF 转 PNG；不改写用户剪贴板。
- Windows 字体视觉审查：flutter test tool/feature_visual_test.dart，输出到 build/feature-review。
- 测试使用 HTML／XML／RTF 固定样本和模拟剪贴板。真实 Word／Excel／PowerPoint 各版本的复制交互仍需人工验收；原生内存测试不等于所有 Office 版本互操作验证。
