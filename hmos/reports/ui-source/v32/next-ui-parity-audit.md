# v32 下一批 UI 功能差距：只读源码审计

范围是实际 Windows build/win-cloud-20261005/lib 与 HMOS 产品接入，不是 SDK、设备、发布或全量 parity 证明。长期目标仍是追平 Windows UI 与大部分实现；音乐子集不会替代该目标。

实际 HEAD：62a8b964bb27840f0348bb6b8ef520142880b9b1。并行审计过程中 Index.ets 出现 tracked 漂移。下列锚点和 JSON 的 SHA-256 对应当前实际磁盘捕获，不代表冻结 HEAD 或已安装版本。最终捕获稳定性：true。

先修两个真实边界：外观读取失败后的默认快照覆盖，以及 completion=2 被普通勾选隐式写为完成。

## APPEARANCE_READ_FAILED_OVERWRITE

**当前：** restoreAppearance 读取失败后仅展示错误；persistAppearance 仍会构造新快照并写入，无 read-failed 禁写或原始未知字段保留。部分字段直接取未验证值。

**Windows / 已有核心：** Flutter saveContent 在 storageReadFailed 时拒绝覆盖；Rust encode_persistent 校验旧字节并合并未知字段。

**来源：** [hmos/entry/src/main/ets/pages/Index.ets:2676](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:2676)；[hmos/entry/src/main/ets/pages/Index.ets:2697](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:2697)；[build/win-cloud-20261005/lib/main.dart:411](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/main.dart:411)；[hmos/shared/plugins/workbench/src/preferences.rs:400](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/shared/plugins/workbench/src/preferences.rs:400)。

**最小完整路线：**

- 先建立有明确 owner 的 typed UI-preferences 存储，保留旧原始值、严格字段验证、读失败禁止普通写入。
- 保留现有写队列，但候选设置/资产引用仅在 durable write ACK 后成为已安装值；写失败保留旧状态与输入。
- 支持未知字段保留与显式恢复，避免后续字体、背景、清单功能覆盖无法读取的数据。

**还需验证：**

- 坏 JSON、错误字段类型、读取失败、未来字段均不得被普通设置操作覆盖。
- put/flush 失败及重启只恢复完整已确认记录，候选输入仍可重试。

## FONT_FILE_IMPORT_RESTORE

**当前：** HMOS 只有本机系统字体选择。导入字体按钮 disabled；AppearanceSnapshot 只有 font:string，无 FontChoice asset/name 或导入资产恢复。

**Windows / 已有核心：** Flutter FontChoice 保存 family/asset/name，用 SHA256 稳定族名；20 MiB TTF/OTF SFNT 头和表界限验证；字体私有存储原子替换；注册预算每进程 8 个族/64 MiB（失败尝试计入）；重启加载验原字节/hash，失败保留偏好并回退。预览含中文、日文、韩文、俄文。

**来源：** [hmos/entry/src/main/ets/pages/Index.ets:3746](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:3746)；[hmos/entry/src/main/ets/pages/Index.ets:3757](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:3757)；[hmos/entry/src/main/ets/model/Appearance.ets:25](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/model/Appearance.ets:25)；[build/win-cloud-20261005/lib/fonts/font_choice.dart:5](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/fonts/font_choice.dart:5)；[build/win-cloud-20261005/lib/fonts/font_repository.dart:10](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/fonts/font_repository.dart:10)；[build/win-cloud-20261005/lib/fonts/font_repository.dart:37](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/fonts/font_repository.dart:37)；[build/win-cloud-20261005/lib/fonts/font_storage_native.dart:22](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/fonts/font_storage_native.dart:22)。

**最小完整路线：**

- 建立独立 filesDir UI-assets/fonts 命名空间，复用 bounded FD capture/hash 与 exact picker ticket，按 20 MiB 限额完整捕获，再验证 SFNT 表与 hash。
- 新增 owned FontChoice/字体验证合同；已审查 Rust shared 模块没有现成 UIFont/SFNT 合同，现有 transport 不能冒称字体验证。
- 仅持久化稳定 hash 资产 ID/显示名，不保存 provider URI；UTF-8 family/name 校验、当前页面/操作 owner、原选择返回 foreground 的准入沿用单次 ticket 模式。
- 验证资产后注册稳定族名并管理进程预算；重启重新验 hash/注册，缺失或错误时保留选择记录并回退。
- 补全导入/取消/忙碌/错误/重置/文件名与多语言预览 UI，以实际设备字体渲染和重启证据闭环。

**还需验证：**

- 真实 TTF/OTF、坏签名、表越界/截断/过大、hash 不符、缺资产、8 族/64 MiB 预算。
- picker Promise 先于/晚于 foreground、取消、旧页面、写失败、重启。
- 设备实际中日韩俄文字体和数字；registerFont 调用不是 glyph 渲染证明。

## BACKGROUND_TEXTURE_FILES_RENDER_RESTORE

**当前：** 导入背景图片/视频 disabled，纹理模式只绘制 PaperBackdrop 纸点。AppearanceSnapshot 无 TextureSource/mediaPlaying；真实文件选择、持久化、解码、清除、动画/声音控制尚未接入。

**Windows / 已有核心：** Flutter TextureSource 保存 location/name/kind/local；本地 image/GIF 25 MiB、video/audio transport 150 MiB，私有 textures 文件；image/GIF cover fit，video 单项循环、初始静音、foreground/reduced-motion gate，player dispose 完成后释放 lease。还有 online URL 和背景声音仲裁。

**来源：** [hmos/entry/src/main/ets/pages/Index.ets:3584](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:3584)；[hmos/entry/src/main/ets/pages/Index.ets:4344](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:4344)；[hmos/entry/src/main/ets/pages/StudioArtwork.ets:104](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/StudioArtwork.ets:104)；[build/win-cloud-20261005/lib/media/texture_source.dart:6](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/media/texture_source.dart:6)；[build/win-cloud-20261005/lib/media/texture_repository.dart:7](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/media/texture_repository.dart:7)；[build/win-cloud-20261005/lib/media/texture_backdrop.dart:49](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/media/texture_backdrop.dart:49)；[build/win-cloud-20261005/lib/media/texture_backdrop.dart:91](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/media/texture_backdrop.dart:91)；[build/win-cloud-20261005/lib/main.dart:1003](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/main.dart:1003)。

**最小完整路线：**

- 独立私有 UI-assets/texture 资产与 typed source，完整 capture/验证/持久化 ACK 前保留旧背景；复用 exact picker/page/operation ownership。
- image/GIF 复用 generic 64 MiB helper 的 25 MiB 调用界限；video 通过明确资产归属的 150 MiB transport adapter 复用 underlying helper，不能走 music library import。当前 music ImportPolicy 确实只暴露 audio。
- 复用 verified image lease 与 AVPlayer factory/callback 模式，建立独立 TextureBackdrop：cover、loop、默认静音、真实 decoder 准入、生命周期/动态效果暂停、release 失败可重试。
- 补齐文件名、清除、错误、播放/暂停、重启恢复及实际 online URL 路径；不把纸点算作文件纹理复现。
- 背景视频/音乐/附件共用单一 audible-owner 仲裁；替换旧源/释放失败不得重开旧源或双播放器，unblock 不自动恢复音乐播放。

**还需验证：**

- image/GIF/video 选择、取消/旧页面、大小/hash/解码失败、清除/替换、重启。
- 设备 codec/cover fit、前后台/减弱动画、先 player release ACK 后 lease release 及重试。
- 背景声与音乐/附件声音交接、远程 URL 验证/失败。

## DAILY_EDITOR_AND_MENU

**当前：** HMOS 持久化三个默认中文 ID 的完成状态，但 Index 固定文本/说明，缺增删改排、自定义说明、恢复默认、全部完成/全部重置与组件编辑菜单；行用文本符号且 vertical padding=4。

**Windows / 已有核心：** Flutter local TipPreferences 独立于业务插件；相同三个默认 ID，自定义 ID daily:<32 lower hex>，100 行/16000 字符校验，存储 ACK 后安装；TipListEditor 完整草稿 UI/hold reorder，组件菜单全选/重置；LittleTask vertical padding=9、Material 状态图标与凹凸动画。

**来源：** [hmos/entry/src/main/ets/pages/Index.ets:3594](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:3594)；[build/win-cloud-20261005/lib/tip_preferences.dart:51](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/tip_preferences.dart:51)；[build/win-cloud-20261005/lib/tip_preferences.dart:55](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/tip_preferences.dart:55)；[build/win-cloud-20261005/lib/tip_preferences.dart:195](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/tip_preferences.dart:195)；[build/win-cloud-20261005/lib/tip_list_editor.dart:9](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/tip_list_editor.dart:9)；[build/win-cloud-20261005/lib/pages/component_menus.dart:46](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/pages/component_menus.dart:46)；[build/win-cloud-20261005/lib/main.dart:5118](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/main.dart:5118)。

**最小完整路线：**

- 新增 owned local TipPreferences 与编辑草稿，完成增/改/删/排/说明/Apply/Cancel/恢复默认流程及 storage ACK。此 presentation 状态不需要新建业务卡片协议。
- 保留现有三个默认 ID，使已存完成态直接兼容；自定义稳定 ID，改名/排序不丢完成态，仅剪除被删除的已知 daily ID。
- 接全选/重置/组件设置、忙碌/错误处理，按 LittleTask 实际 insets/图标/各样式状态复现。
- 新存储使用 typed read-failure/write-ACK 边界；Rust Preferences.completed 已有 ID 字段，但没有自定义文本/说明合同。

**还需验证：**

- 重启保留文本/说明/顺序/完成；改名/排不丢完成，删/恢复默认保留非 daily 状态。
- 100 行/16000 字符、重复/错误 ID、多行行文本、写失败保留草稿和旧状态。
- 设备编辑/长按排/焦点、行间距/各样式/全部完成重置。

## TASK_AMBIGUITY_IMPLICIT_RESOLUTION

**当前：** editor toggleTask 与 detailTask 均用 completion !== 1 得到 true；completion=2 的历史待确认任务会被普通勾选直接写为完成。

**Windows / 已有核心：** Flutter VersionedTaskPanel 对 needsExplicitDecision 给独立提示和标为完成/标为未完成按钮。Rust 三态投影和 SetCompletion(bool) 已存在。

**来源：** [hmos/entry/src/main/ets/pages/Index.ets:2621](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:2621)；[hmos/entry/src/main/ets/pages/Index.ets:2844](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:2844)；[build/win-cloud-20261005/lib/versioned_task_panel.dart:404](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/versioned_task_panel.dart:404)；[hmos/shared/plugins/workbench/src/tasks_v2.rs:399](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/shared/plugins/workbench/src/tasks_v2.rs:399)。

**最小完整路线：**

- 两个任务界面明确展示 completion=2，普通 checkbox 不得替用户选择结果。
- 用现有 task_toggle/SetCompletion(bool) 分别接完成/未完成显式选择，绑定 exact task ID/source 与 accepted receipt，成功选择前保持 2。

**还需验证：**

- 正常显示/普通勾选不会隐式消歧；两种明确选择只改变当前 owned task。
- 旧 source/删任务/过期详情/未知结果保留原请求与待确认状态。

## DETAIL_TASK_FULL_OPERATIONS

**当前：** 详情只有完成按钮与可选任务文本；缺 Flutter inline 增删改/copy/up-down/hold reorder 和选阶段完成全部。editor 已有部分操作，但 complete-all 固定当前阶段；submit 对 add/rename 依赖 editor 冻结完整输入。

**Windows / 已有核心：** Flutter VersionedTaskPanel 有完整详情操作和阶段选择，按匹配 accepted view 安装。Rust task_add/toggle/remove/rename/reorder/complete_all/stage 与任务投影已有。

**来源：** [hmos/entry/src/main/ets/pages/Index.ets:2844](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:2844)；[hmos/entry/src/main/ets/pages/Index.ets:2657](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:2657)；[hmos/entry/src/main/ets/pages/Index.ets:2066](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:2066)；[build/win-cloud-20261005/lib/versioned_task_panel.dart:18](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/versioned_task_panel.dart:18)；[hmos/rust/src/lib.rs:687](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/rust/src/lib.rs:687)。

**最小完整路线：**

- 建立 owned detail-task coordinator：source/task/revision 与 full TextValue/FieldPolicy 输入；不能把 detail 文本硬接缺少 editor frozen-input 的旧 submit。
- 复用 Rust 事务/投影/完整排序验证，补 add/rename/delete/copy/up-down/hold reorder、category 阶段 picker 与选阶段全部完成。
- 保留原始 IME/selection/composing、未知结果 exact original request；匹配 ACK 后才清空已提交输入/安装 UI；drag 绑定 task ID/source/generation。

**还需验证：**

- 全部详情操作、显式消歧、选阶段 complete-all、稳定 ID 排序。
- 完整输入/IME/paste、source 变化/并发编辑、失败/未知 ACK 与关/重开不丢输入或归属。

## DELETE_ACK_UNDO_AND_EXPIRY_UI

**当前：** HMOS 已有真正 deleted-card page/detail、复制与附件访问，以及 8 秒 restore 命令；restore button/menu 只按 busy/pending 禁用，过期仍可点击。删除成功只通用保存消息，缺直接 Undo 通知。

**Windows / 已有核心：** Flutter WorkbenchPage 实际只有五页，没有 Trash 页。这里真正差距是 _showVersionedDeleteUndo：原标题、dispatch 前 monotonic Stopwatch、剩余 8 秒、同 backend owner、原位置；Rust 也限制 8 秒。

**来源：** [hmos/entry/src/main/ets/pages/Index.ets:2935](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:2935)；[hmos/entry/src/main/ets/pages/Index.ets:3906](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/entry/src/main/ets/pages/Index.ets:3906)；[build/win-cloud-20261005/lib/main.dart:1526](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/main.dart:1526)；[build/win-cloud-20261005/lib/plugins/workbench_ids.dart:3](C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/lib/plugins/workbench_ids.dart:3)；[hmos/shared/plugins/workbench/src/cards_v2.rs:166](C:/Users/Administrator/Desktop/CodeXProjext/morrow/hmos/shared/plugins/workbench/src/cards_v2.rs:166)。

**最小完整路线：**

- dispatch 前记录 source-bound delete receipt 与 monotonic 时间；仅实际 delete ACK 后显示原卡片 Undo，使用保守剩余时间。
- 复用 restore/source 与 CardOrder 原位置，detail/menu 过期真实禁用，foreground 时刷新期限。
- 维持共享 8 秒语义，不为所谓 Flutter 对齐新增永久恢复/清空回收站语义。

**还需验证：**

- ACK 延迟、过期前后、前后台、backend/source 变更、重复恢复、未知结果及恢复原位置。
- 设备实际通知/action 与过期禁用渲染。

## 复用边界

- **native file_stream/lib.rs/index.d.ts**：bounded owned FD + streaming SHA256 + sync_all；generic 64 MiB、另有当前名为 music 的 150 MiB helper。 传输不是字体或 codec 验证，独立 UI-assets adapter/命名空间；20 MiB font、25 MiB image/GIF、150 MiB video transport。
- **MusicWorkbench.ets + MusicFiles.ets**：exact one picker ticket、page/operation owner、foreground-before/after-Promise 准入、私有 capture/hash/journal 模式。 复用模式，不能复用 music IDs、library mutation、音乐 pending 请求来导入字体/背景。
- **AttachmentFiles / VerifiedImages / image-media preview / AttachmentPlayback**：filesDir 私有 capture、哈希、验证 lease、实际 image/AVPlayer components。 独立背景 controller 补 cover/loop/mute/lifecycle/release ACK，不能将普通附件播放器直接算作背景功能。
- **shared services.rs import_policy**：pure 25 MiB image/GIF、150 MiB video/audio 与 remote URL 验证。 现有 music_bridge ImportPolicy hardcodes audio；如需纯 generic policy，新增 owned native adapter，不扩权限。
- **shared preferences.rs / preferences.proto**：已有 TextureSource/completed、旧字节校验与未知字段合并。 无 UIFont FontChoice/SFNT、自定义 daily text/caption；仅复用支持的合同与存储模式。
- **Rust lib.rs / tasks_v2 / tasks_v2_codec / cards_v2**：tasks commands、三态投影、SetCompletion(bool)、完整 reorder、complete-all chosen stage、8 秒 restore。 补齐 UI coordinator/所有权与确认边界，无须重做业务事务。

实际 hmos/rust/Cargo.toml 使用 ../shared/core 与 ../shared/plugins/workbench 的 HMOS 拷贝；别的根模块不能冒称已编入。新增 UI-owned 合同复用现有模块，不为本批功能扩大平台权限。纯 policy 或播放器实例不是实际背景渲染证明。

## 建议实现顺序

1. 先修 typed preference 读失败覆盖和 task=2 显式决策。
2. 完整字体 vertical slice：选择/capture/验证/保存/注册回退/UI/重启和设备 glyph。
3. 完整本地 background vertical slice：image/GIF + video 所有权与真实 renderer/restart；在线 URL 保持明确 OPEN 直至接入。
4. 完整 daily customization/editor/menu/write ACK/restart，并跟实际行/UI样式。
5. 完整 detail task coordinator/actions/chosen-stage，再接 delete ACK Undo/expired UI。

本审计未改产品源码，未运行 tests/SDK/设备，未做 Git mutation，仅新增这对审查文件。验收仍需每项完整控制器/持久化验证，以及设备的字体、解码渲染、输入、生命周期和声音归属证据。Root 音乐设备证据必须单独关联实际编译/安装源码。

JSON 记录了 36 个读取来源的字节数与 SHA-256，后续源码变动须重新匹配锚点/哈希。
