# dev22 七风格可见区域独立图片审查

限定结论：已实际逐图查看16张原分辨率图片；dev22七种风格首页/设置的已看可见区未发现新的明显视觉阻断。明确前后改善仅限新拟态Hero及首卡可见部分，搜索凹陷保留；设置前后差异不明显。全部界面框线与完整Flutter像素对齐仍OPEN。

图片审查者 music_import_durability 使用 view_image(original) 完成实际读取；Root依据其完整观察交接生成本记录和精确文件清单。审查者没有操作设备或修改产品，也没有独立验证HAP映射或业务数据。版本来源见[安装记录](installation-dev22.json)；最终返回原扁平首页及14卡/4草稿显示见[限定观察](final-observation-a2.json)。这些不是设备包受保护字节或完整资料逐字证明。

| 风格 | 实际原图 | 实际观察 |
| --- | --- | --- |
| 新拟态 | [dev21-neumorphism-home-b](dev21-neumorphism-home-b.png)、[dev22-neumorphism-home-b](dev22-neumorphism-home-b.png)、[dev21-neumorphism-settings-b](dev21-neumorphism-settings-b.png)、[dev22-neumorphism-settings-b](dev22-neumorphism-settings-b.png) | dev21首页Hero顶/右/底内侧及首卡可见顶/右边有锐灰重复轮廓；dev22对应可见区域不再有这层额外内轮廓。dev22搜索上左凹陷保留。设置前后差异不明显，不能声称设置也明确消除了同类轮廓。紫色磨砂选中框和环形装饰保留。 |
| 纸感 | [dev22-paper-home-b](dev22-paper-home-b.png)、[dev22-paper-settings-b](dev22-paper-settings-b.png) | 首页搜索、Hero及首卡可见部细灰边连贯，Hero下沿窄投影；设置容器、行和材质选项边框一致。可见区未见脱离组件形状的额外轮廓、异常文字重叠或显著误裁切。 |
| 黏土 | [dev22-clay-home-b](dev22-clay-home-b.png)、[dev22-clay-settings-b](dev22-clay-settings-b.png) | 圆角更大，Hero下沿单一柔和投影，搜索上左凹陷清晰。设置浅色圆角表面、磨砂紫色选中框清楚，未见旧新拟态的锐灰额外内轮廓或异常重叠。 |
| Fluent | [dev22-fluent-home-b](dev22-fluent-home-b.png)、[dev22-fluent-settings-b](dev22-fluent-settings-b.png) | 首页轻细边框、低强度下沿投影，搜索边缘与组件轮廓重合；设置细圆角边框和紫色选中框连贯。可见区未见独立第二大框或异常重叠。 |
| 粗野主义 | [dev22-brutalist-home-b](dev22-brutalist-home-b.png)、[dev22-brutalist-settings-restored-b](dev22-brutalist-settings-restored-b.png) | 首页与设置有明显厚灰边，Hero底部右下硬投影属于该形态主投影，不能作为未知框一概删除。恢复设置实际文字为粗野主义。可见内容未见明显异常重叠或误裁切。 |
| 工业风 | [dev22-industrial-home-b](dev22-industrial-home-b.png)、[dev22-industrial-settings-restored-b](dev22-industrial-settings-restored-b.png) | 首页搜索矩形灰边和内侧窄灰带，Hero薄灰边及窄下沿投影；恢复设置实际文字为工业风，控件边框一致。未见独立于组件边框/投影的重复未知轮廓。 |
| 扁平 | [dev22-flat-home-b](dev22-flat-home-b.png)、[dev22-flat-settings-restored-b](dev22-flat-settings-restored-b.png) | 首页圆角浅细边和低对比软投影；设置实际文字为扁平 · 默认，无立体深度区域，语言/字体/材质/独立设置按钮间隔正常。底部预览仅上部在视口内，完整预览未审；可见区未见明显异常重叠或误裁切。 |

目录有45张PNG，本报告仅16张实际逐图审查；其余29张明确NOT_VISUALLY_REVIEWED。每张原图及对应UI树的字节数、SHA256和未看清单见[结构化记录](visual-review.json)。

所有首页首卡下部均未完整入镜；未审全部卡片、详情、编辑器、尺寸/主题/材质组合。粗野主义的厚边及硬投影、工业风搜索内侧窄灰带、设计选中框不能一概视作未知框删除。底部仅部分预览是当前视口边界，不授完整预览资格。

粗野主义选择后旧工具因滚动标签不在可见树而停止，原动作与图片保留；Root后续关闭重开核对真实标签且未重复select。审查者未把该工具断言独立认定为产品故障。

本报告属于旧v32/dev22实际绘制观察。新v33/dev23任务与Preferences设备验收NOT_RUN；真实音乐、业务资料无损、偏好重启、ARM64、签名、全页面未知轮廓消除及完整对齐均未由这些图片取得资格。
