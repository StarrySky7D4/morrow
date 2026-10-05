# dev.11 卡片菜单、详情与拖动平台核对（2026-10-05）

本报告只读核对实际 Flutter 源码、本机 API26 声明及 OpenHarmony 官方实现。仅创建本报告，没有修改应用、启动或操作设备、构建 HAP、运行 UITest。状态为 `SOURCE_CHECKED / DEV11_DEVICE_NOT_RUN`。代码段用于说明已核对的接口，未单独编译；最终 Index 集成须由本轮构建和设备证据确认。

## 实际参照与 SDK

工作区观察分支为 `codex/ArkTsUI`。Flutter 参照为 `build/win-cloud-20261005`，本次 HEAD 为 `772466177fe589cee53bc633e69f411c34610104`；以下文件的本次工作树检查未显示修改。

| Flutter 文件 | Git blob | 原字节 SHA-256 |
|---|---|---|
| `lib/hold_reorder.dart` | `2ed1ab3b4b26bad3a3ba4c49050ef6b76980898e` | `2BD70DCD16EE8BEA076A7BF10B9FE1E99129D899899610D79827D7AE2EE97007` |
| `lib/component_context_menu.dart` | `e4cf439001e4347c65d4f07752eb9f9d06b3919d` | `9DE2D33330A9676BED8170FDB49A3344A2B324DF639A895D7463A5155B44E2AC` |
| `lib/pages/card_order.dart` | `c63c46511532d4bc0fc8d803de6dcaadeacfcde1` | `10ED921CB52B7ECDDE1C3AA78624FCDF1A3DEB1CD2E982EF2F3BE329DE505D9E` |
| `lib/pages/component_menus.dart` | `4a3237ff84419f7d0a65fe02c3fbca93685801a7` | `98B3D7B90BA8446B1F9FB548FB9924686F02D400BE774BDEDC61D805EABBE430` |
| `lib/pages/workspace_pages.dart` | `8f3197d1c978a332808ccc1b8e119a7d3a4b74b2` | `AC6C202E2C8CA0A63A6CB30A7D4A9EA55FED5EE0ECF5D87B1CBCEAC5F06A1FE0` |
| `lib/main.dart` | `2e00f41251c3215f0b6d886712c86adfdf2bb6e6` | `2CB2A519E31AC982D3A8638EB7DE95FE63D5421ED3D1B6ACDA507CD142169F06` |

SDK 根目录为 `C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony`。实际读取 `ets/oh-uni-package.json`：`apiVersion=26`、`platformVersion=26.0.0`、`version=26.0.0.105`、`releaseType=Release`。下面的声明均来自该根目录，不是从设备版本推定。

## Flutter 行为边界

- `hold_reorder.dart` 的 `moveRelative` 按源/目标身份移动，不使用按下时缓存的索引。接受条件包含 enabled、相同 scope、相同 revision、不同身份；目标的上/下半区决定放在前/后。
- Flutter `LongPressDraggable` 延迟为 380 ms，允许主鼠标键，最多一个拖动。触屏和鼠标均经长按；编辑器使用独立拖动柄以保留文字选择。卡片通常由 `sortableCard` 包住整卡。
- `_DragScroll` 在 viewport 边缘 64 logical px 内每 16 ms 移动 12 px，并通过全局 pointer route 保持虚拟化源卡片移出 viewport 后仍滚动；release/cancel 清理。原生 onDragStart 本身不提供这项完整产品行为。
- `pages/card_order.dart` 的 revision 包含 section、filter、query、sort、内容 generation、顺序 preferences revision、查询 phase 与查询 identities。接收时必须仍处于同一可写、查询已就绪视图。移动使用完整页面 identities 和可见 identities，并保留被过滤的卡片顺序。
- `component_context_menu.dart` 使用鼠标次键、Shift+F10、键盘菜单键打开上下文菜单。菜单打开不触发命令；选中后再次检查 action 可用性。触屏长按菜单和显式 `···` 是 HMOS 适配，不能称为原 Flutter 手势完全相同。
- `main.dart:3205` 卡片菜单包括查看、前/后移、编辑、复制、灵感转项目、删除、收藏及符合条件时的组件设置。命令沿已有业务入口执行；可写命令检查当前 source identity。`pages/workspace_pages.dart:193` 的主点击打开详情。
- `main.dart:4714` 默认 `openIdea` 先显示详情：正文 Markdown、附件、实验假设/观察、项目任务等。只有显式选择 edit 后才打开 editor session；打开/关闭详情本身不创建编辑草稿。详情中的业务控件仍受可写与当前身份检查约束，不应将其笼统描述为所有控件均不可操作。

## API26 原生接口

实际声明位置：

- `ets/component/common.d.ts:5836`：`DragItemInfo` 有 `pixelMap?: PixelMap`、`builder?: CustomBuilder`、`extraInfo?: string`。
- `common.d.ts:20636`：`onDragStart((event: DragEvent, extraParams?: string) => CustomBuilder | DragItemInfo)`。
- `common.d.ts:20650/20664/20678/20694/20727`：onDragEnter、onDragMove、onDragLeave、onDrop、onDragEnd 的回调均为 `(event: DragEvent, extraParams?: string) => void`。
- `common.d.ts:20770`：`draggable(value: boolean)`。触屏先长按超过 500 ms，再移动超过 10 vp；鼠标左键按下并移动超过 1 vp 即触发。这与 Flutter 380 ms 长按鼠标/触屏存在适配差异。
- `common.d.ts:21495`：`bindContextMenu(content: CustomBuilder, responseType: ResponseType, options?: ContextMenuOptions)`；`ets/component/enums.d.ts:2979` 有 `ResponseType.RightClick`、`ResponseType.LongPress`。LongPress 不支持鼠标长按。
- API23 的 `bindContextMenuWithResponse` 可统一触屏长按和鼠标右击。本轮决定使用卡片 LongPress 菜单及显式 `···` 按钮，鼠标通过按钮访问菜单。拖动放在独立柄上，避免与正文选择或详情点击混用。

可以接入现有 @Builder 的签名形态如下；回调业务函数不是 SDK 接口：

```ts
.draggable(this.canOrderCards())
.onDragStart((): DragItemInfo => this.beginCardDrag(card))
.onDragEnd(() => this.endCardDrag())

.bindContextMenu(this.CardMenu(card), ResponseType.LongPress)
.onDragEnter((event: DragEvent, extra?: string) => this.hoverCard(event, extra, card))
.onDragMove((event: DragEvent, extra?: string) => this.hoverCard(event, extra, card))
.onDrop((event: DragEvent, extra?: string) => this.dropCard(event, extra, card))
```

onDragEnter/Move/Leave 仅在同一目标绑定 onDrop 时生效。onDragStart 的自定义 builder 只用于当次预览；不能用它作为持续变化的实时卡片。祖先整卡菜单与子拖动柄仍须设备验证手势竞争；若菜单抢到柄的长按，将菜单绑定限定到正文容器，保留按钮入口。

## 菜单构建器不展示：生成代码复核

主任务反馈 dev.11 设备上两张卡片的显式菜单按钮点击均未出现菜单，而详情和普通数组排序菜单仍可用。该设备观察来自主任务；本审计未操作设备、未读 hilog、未自行运行 HAP，也没有菜单运行 PASS。

本审计只读生成文件 `hmos/entry/build/default/cache/default/default@CompileArkTS/esmodule/debug/entry/src/main/ets/pages/Index.ts`。旧版本第 4364 行保留 `Button.bindMenu(this.CardMenu.bind(this, item.card))`；第 3966、4465 行的 bindContextMenu 同样是裸 bound function，没有 `{ builder: ... }`。之后主任务尝试箭头调用，本审计重新读取生成文件，见 `Button.bindMenu(() => { this.CardMenu(item.card); })` 和对应裸箭头 bindContextMenu，仍无 builder 对象。

本机 SDK 编译器 `C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/lib/process_component_build.js` 是压缩单行文件，准确定位为 `isBuilderChangeNode`、`processCustomBuilderProperty`、`parseBuilderNode`、`getParsedBuilderAttrArgumentWithParams`：谓词识别已登记 Builder 名称的直接成员/调用。`.bind` 的成员名是 bind，裸箭头也不在该识别分支。`component_map.js` 中的 `CUSTOM_BUILDER_PROPERTIES_WITHOUTKEY` 不包含 bindMenu/bindContextMenu，因此正确识别的带参 Builder 调用会被生成 `{ builder: () => Builder.call(this, ...参数) }`。

官方引擎 [js_popups.cpp](https://github.com/openharmony/arkui_ace_engine/blob/master/frameworks/bridge/declarative_frontend/jsview/js_popups.cpp) 的 `JSViewAbstract::JsBindMenu` 接受数组，或读取对象内的 builder Function；缺少 builder 时直接返回。`JsBindContextMenu` 同样要求对象内 builder Function。旧 bound function 和新裸箭头均缺少该字段，可解释为什么普通数组菜单可用而卡片自定义菜单未绑定。此源码解释尚未与设备二进制作 commit 对应，修复仍须实际界面验收。

针对本机编译器最稳的推荐写法是官方 [菜单示例 18](https://github.com/openharmony/docs/blob/master/zh-cn/application-dev/reference/apis-arkui/arkui-ts/ts-universal-attributes-menu.md#示例18bindmenu传入带参数的custombuilder) 中的直接带参 Builder 调用：

```ts
.bindMenu(this.CardMenu(item.card))
.bindContextMenu(this.CardMenu(c), ResponseType.LongPress)
```

这里是 ArkUI 声明式 Builder 语法，编译器生成延迟调用，并非手动在点击前执行菜单。修复后应检查生成文件包含 builder 对象，再分别核对显式菜单与正文长按截图、取消无业务动作、命令身份重验。

本审计最初依据通用 [@Builder 作为 CustomBuilder 使用](https://github.com/openharmony/docs/blob/master/zh-cn/application-dev/ui/state-management/arkts-builder.md#将builder装饰的函数当作custombuilder类型使用) 建议裸箭头；该建议没有充分核对本机菜单参数包装，现明确纠正。通用箭头建议适用于已有 builder 字段的结构，例如 `DragItemInfo.builder: () => { this.CardDragPreview(c); }`；不能据此宣布裸箭头 bindMenu 已修复。菜单和拖影的参数结构应分别核对。

## 本地令牌与 extraInfo 的准确传递

API26 的 DragEvent **没有 `getExtraInfo()`**。onDragStart 可返回 `{ extraInfo: nonce }`，后续目标回调的第二参数是 JSON，读取其 `extraInfo` 字符串。

已核对官方引擎：JSViewAbstract 解析返回对象的 extraInfo；EventHub::GetDragExtraParams 将非空字符串放入 JSON；DragDropManager 在 DROP 用该方法生成回调参数。JS onDragEnd trampoline 传空 JSON，不能要求 onDragEnd 回传 nonce。

本轮采用本地令牌，不调用 DragEvent.getData，不接受外部 URI、文字、文件或 UDMF 数据。extraInfo 自身不是权限凭据，必须与本窗口最近一次合法 onDragStart 建立的、一次性的本地 nonce 完全相等，同时仍有 active source。没有 active source、缺失/错误/不匹配 nonce、JSON 无法解析、源等于目标、源/目标已失效、视图变化或不可写均拒绝。可以给 extraParams 加小的长度上限后再解析。

冻结值应包括 page、sort、query/filter、查询 phase 与 identities、内容 generation、顺序 preferences revision。onDrop 前重新验证全部字段，并按 identity 调用原顺序模型；页面/查询/排序/内容刷新、进入编辑、生命周期结束、取消和 onDragEnd 均清理本地状态。不得用客户端 extraInfo 中的 card id 直接决定业务 source。

`allowDrop` 未设置时允许任意数据类型进入回调，这不是来源鉴权。`allowDrop(null)` 会连内部 drop 一起拒绝；本轮无 UDMF 数据路线由本地令牌守门。enter/move 拒绝时设置 `DROP_DISABLED`；drop 拒绝时显式设置 `DRAG_FAILED`，避免平台将普通目标默认标为成功。

## 坐标、结果与保存边界

- `common.d.ts:10704` 的 `getWindowY()` 返回窗口左上角原点的 vp。`ets/component/units.d.ts:2119` 的 Area.globalPosition 同样使用当前窗口坐标，Area.height 单位 vp。两者可比较目标上下半区。
- `getDisplayY()` 是屏幕原点 vp，`getGlobalDisplayY()` 是全局屏幕 vp；不得和窗口 bounds 混用。`getY()` 自 API10 废弃，替代为 getWindowY。
- UITest Point 和 layout 的坐标是 px，不能直接带入应用侧 vp 逻辑。UI 测试从最新布局重新获取节点 bounds；屏幕旋转、键盘和滚动后重新观察。
- `getResult(): DragResult` 与 `setResult(result)` 可用。枚举为 `UNKNOWN=-1`、`DRAG_SUCCESSFUL=0`、`DRAG_FAILED=1`、`DRAG_CANCELED=2`、`DROP_ENABLED=3`、`DROP_DISABLED=4`。最后两项用于 enter/move/leave；`DRAG_SUCCESSFUL`、`DRAG_FAILED`、`DRAG_CANCELED` 用于 drop 完成结果。
- onDragEnd 不包含拖放位置，只用于清理 token、插入提示与边缘滚动。拖放接受不等于顺序保存：Index 若先 setResult(DRAG_SUCCESSFUL) 后异步保存，只证明原生动作被接受。保存失败必须保留原顺序并反馈；持久性以最终 preferences 及重启回读为准。
- 虚拟布局必须按当前测量更新目标 bounds。整卡高亮仅表明目标，若要完整跟随 Flutter，还需要上/下插入线、取消清理和虚拟化源离屏后边缘滚动的独立实现与验收。

## 可行的 UITest 验收，尚未执行

本机 `ets/api/@ohos.UiTest.d.ts` 声明 `Component.longClick()`、`Component.dragTo(target)`、`Driver.longClickAt(point, duration?)`、`Driver.dragBetween(from, to, speed?, duration?)`、`Driver.mouseDrag(...)`、`Driver.mouseClick(point, MouseButton)`。dragBetween 的长按参数要求至少 1500 ms，默认 1500 ms；mouseDrag 也有带 duration 的重载。左鼠标键枚举 0，右鼠标键 1。

已核对 DevEco CLI：ui drag 使用 `uitest uiInput drag fromX fromY toX toY [speed]`，ui longclick 使用 `uitest uiInput longClick X Y`。官方 UITest GenericSwipe 对 DRAG 先增加 longClickHoldMs；源码默认该值 1500 ms。因此 CLI drag 是长按后连续移动的触屏动作，不能用两个独立 click/move 命令代替仍按住的指针，也不能作为鼠标拖动验收。

### 本机命令接口，仅执行了 help

本轮实际读取 `C:/Users/Administrator/AppData/Roaming/npm/devecocli.ps1` 及 `node_modules/@deveco/deveco-cli/dist/cli.js`，并执行 `devecocli ui --help`、`ui drag --help`、`ui longclick --help`、`ui swipe --help`，均返回 0。未执行以下设备输入命令。

- `devecocli ui drag <x1> <y1> <x2> <y2> --device <name|serial> --speed <n>`；坐标和速度使用 px、px/s，无节点 `--id` 或长按 duration 参数。
- `devecocli ui longclick [x] [y] --device <name|serial>`；另支持 `--id <id>` 和 `--window <windowId>`，将节点中心解析成坐标。正文菜单验收宜使用已观察的正文点，避免节点中心落在按钮或任务控制上。
- `devecocli ui swipe <x1> <y1> <x2> <y2> --device <name|serial> --speed <n>`；swipe 没有 DRAG 的预先长按，不证明拖动柄排序可用。
- 官方 uiInput 声明 `drag <from_x> <from_y> <to_x> <to_y> [velocity] [displayId]`；velocity 为 200..40000 px/s，默认 600。`longClick <x> <y> [displayId]`。本机 CLI 仅转发上述坐标和可选 speed，未暴露 displayId。
- 该 CLI 和已读 uiInput 分派没有独立 `press`、`pointerDown`、`pointerUp` 或鼠标拖拽命令。drag 本身包含持续按住过程；先 longClick 再 drag 会先松开，可能先打开祖先菜单，不能组成一次拖拽。

下面是接口示例；变量必须来自执行前最新布局或截图，起点位于拖动柄，终点分别选目标正文上/下半区。示例没有提供未经观察的坐标，也未在本轮操作设备。

```powershell
$dev11Hdc = 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe'
& $dev11Hdc -t '127.0.0.1:5557' shell uitest uiInput drag $sourceX $sourceY $targetX $targetY 600
& $dev11Hdc -t '127.0.0.1:5557' shell uitest uiInput longClick $bodyX $bodyY

# 本机 CLI 的等价入口
devecocli ui drag $sourceX $sourceY $targetX $targetY --device 127.0.0.1:5557 --speed 600
devecocli ui longclick $bodyX $bodyY --device 127.0.0.1:5557
```

上述两个动作是各自独立的验收项：一条 drag 验证拖动柄排序，一条 longClick 验证正文上下文菜单。执行后检查 stdout 中的失败/异常，并重新观察 UI，不能只凭 shell exit code 认定成功。鼠标拖拽须另用 UiTest Driver.mouseDrag 或人工鼠标验收。

建议设备检查范围：

1. 主点击进入详情；查看/返回不创建 editor draft 或 business change。菜单打开、取消不执行命令；显式菜单按钮与触屏长按均能选择动作。
2. 在同一就绪视图使用拖动柄，目标上半区/下半区分别前移/后移；同源 drop 和 viewport 外 release 不改变顺序。命令后重新读取当前可见 identities。
3. 显式切换排序、页面、查询或刷新期间的旧令牌被拒绝。菜单 command 在 source identity 变化后同样拒绝。
4. 过滤后移动保持隐藏 identities；保留合法 sibling 顺序；保存失败保持旧顺序；进程重启后回读已保存结果。没有 persistence 证据时不能只用拖影或高亮宣布排序成功。
5. 手机手指与鼠标拖动分别验收，长按柄不弹出祖先菜单；拖动期间不触发卡片主点击。需要边缘滚动时单独覆盖源卡离屏及 release/cancel。
6. 对任意外部拖入，UI 可发生回调但没有合法本地 token 就拒绝，业务顺序及内容保持不变。静态 nonce 门测试可覆盖此分支；不能把静态测试等同跨应用设备拖入验收。

## 官方来源

以下是读取时官方 master 文档/源码，未与当前设备二进制作 commit 对应，不代替本地 SDK 声明或 dev.11 运行证据。

- [ArkUI 拖拽事件](https://github.com/openharmony/docs/blob/master/zh-cn/application-dev/reference/apis-arkui/arkui-ts/ts-universal-events-drag-drop.md)
- [ArkUI 菜单属性](https://github.com/openharmony/docs/blob/master/zh-cn/application-dev/reference/apis-arkui/arkui-ts/ts-universal-attributes-menu.md)
- [@Builder 与 CustomBuilder](https://github.com/openharmony/docs/blob/master/zh-cn/application-dev/ui/state-management/arkts-builder.md)
- [菜单原生参数解析](https://github.com/openharmony/arkui_ace_engine/blob/master/frameworks/bridge/declarative_frontend/jsview/js_popups.cpp)
- [JSViewAbstract 拖拽返回值与结束回调](https://github.com/openharmony/arkui_ace_engine/blob/master/frameworks/bridge/declarative_frontend/jsview/js_view_abstract.cpp)
- [EventHub extraParams 生成](https://github.com/openharmony/arkui_ace_engine/blob/master/frameworks/core/components_ng/event/event_hub.h)
- [DragDropManager drop 与 extraInfo](https://github.com/openharmony/arkui_ace_engine/blob/master/frameworks/core/components_ng/manager/drag_drop/drag_drop_manager.cpp)
- [UITest shell 输入分派](https://github.com/openharmony/testfwk_arkxtest/blob/master/uitest/input/ui_input.cpp)
- [UITest 触屏 drag 分解](https://github.com/openharmony/testfwk_arkxtest/blob/master/uitest/core/ui_action.cpp)
- [UITest 默认时间参数](https://github.com/openharmony/testfwk_arkxtest/blob/master/uitest/core/ui_action.h)
