# v33 字体导入源码合同与完整实施方案

审查日期：2026-10-09。范围：只读实际 Windows/Flutter、HMOS、安装的 API26 SDK 声明与 Huawei 本地 API 文档；仅新增本报告和同名 JSON。未改产品、字体原件、测试、Git、设备或工具环境。源码证据与摘要清单见同名 JSON；本报告不是字体功能完成或设备通过声明。

## 结论

字体导入缺口可以直接实施，当前不需要新的用户授权。必须一次接通字体描述符、真实选择、20 MiB 完整字节捕获与 SFNT 验证、私有原件、注册预算、确认后的偏好、重启回退和现有界面。当前 disabled 导入按钮、单个 `font` family 字符串及手机系统字体白名单均不足以追平 Flutter。

旧 v32 审计所述“HMOS shared 没有 UIFont/SFNT 合同”在 HMOS 目录内成立，但不能延伸成“现有 Rust 没有 UIFont 合同”：真实 Windows 源 `build/win-cloud-20261005/workbench_host/src/ui_preferences.rs:108-233` 已有可移植的 Rust `FontPreference`、独立 core 记录、CAS 与原 operation 重试。SFNT 字节验证实际在 Flutter `lib/fonts/font_repository.dart:15-34`；没有现成 Rust SFNT 验证器。

API26 可直接采用 `text.FontCollection.getGlobalInstance().loadFontWithCheck(family, fileUri, 0)`：返回 Promise，提供文件缺失、读取、空文件、损坏等明确错误。安装的 Huawei 官方本地文档的完整示例在 `en-us_topic_0000002583661148.html:2626-2655` 使用这份全局集合并直接渲染 `Text(...).fontFamily(...)`。这比返回 `void` 且声明“异步、不支持并发”的 `UIContext.getFont().registerFont` 更适合本项目确认加载后应用字体的合同。Promise 成功仍不是实际 glyph、排版或设备显示已经通过的证据。

## 1. 当前实际 Flutter 行为

| 合同 | 实际源码与含义 |
| --- | --- |
| 描述符 | `font_choice.dart:5-45`：`family/asset/name`；默认全空；系统字体仅 family；导入 asset 为小写 64 位 SHA256、family 必须空、name 必须非空。family UTF8 不超过128字节且已 trim；name UTF8 不超过255字节；拒绝 C0/C1 控制字符。字体字节从不进入普通偏好消息。 |
| 稳定族名 | `font_choice.dart:8-13`：导入字体固定为 `MorrowFont_<asset>`；同一字节再次导入不生成另一个族名。默认的 resolvedFamily 为 null。 |
| SFNT 基础验证 | `font_repository.dart:10,15-34`：大小 12 字节至20 MiB；大端 signature 仅 `0x00010000` 或 `0x4f54544f`；tableCount 为1至256，完整 directory `12 + count*16` 位于文件内；每表 offset/length 在字节范围内，检查 length <= fileLength-offset，避免相加越界。拒绝 TTC/WOFF/WOFF2及仅后缀正确的数据。该验证允许零长度表，不检查表内容、checksum或实际 glyph，不能声明完整字体语义验证。 |
| 选择与文件名 | `font_settings.dart:117-135`：单个真实 `.ttf/.otf` 选择，取消无变更。`font_repository.dart:72-85`：预查尺寸后完整读取，再做实际长度/SFNT检查；name 为路径末段，不保存选择路径。 |
| 注册预算 | `font_repository.dart:11-14,37-52`：进程内最多8个导入 asset、累计原始字体字节64 MiB；已有 asset 共用同一个 pending Future；发起注册前记账，失败也保留记账与失败 Future，不能重复释放额度。预算约束源字节与族数量，不等于引擎解码内存准确上界。 |
| 导入顺序 | `font_repository.dart:78-85`：建立 descriptor、注册、私有 store，全部完成才返回 choice。UI `_apply` 后交给 `scope.onChanged`；失败/取消不改当前 choice。HMOS 可以采用先私有原件，再有 ACK 的注册，最后保存偏好的顺序，避免无 durable 原件的已应用字体。 |
| 私有原件 | `font_storage_native.dart:5-36`：应用 support/fonts/hash.sfnt；仅 SHA256 文件名；temp 写入并 flush 后 rename；恢复优先迁移库/fonts，再 support目录；预查尺寸后读，实际 byte validator/hash 再核验。源码没有目录 fsync、已存在目标 NOFOLLOW、失败 temp 清理等更强保证，不能把这些描述成已实现的 Windows合同。 |
| 初次恢复失败 | `main.dart:345-370`：保留 uiFont choice，实际 family 回退默认并告知不可用；不会把丢失资产的 choice 自动改成默认再保存。generation+mounted 防止迟到加载覆盖新 choice。描述符解析失败本身回退空 choice；这不同于原件加载失败保留 choice。 |
| 字体切换 | `main.dart:376-385`：验证、加载、mounted/generation检查、修改 UI、保存；`studio_storage.dart:78-125` 冻结候选并由 backend.saveUiFont 单独确认。Windows main 对 UI-before-save 的行为不能被误写成 save-before-UI；HMOS 应区分候选与确认状态并明确未保存状态。 |
| 家族输入 | `font_settings.dart:85-109`：输入 trim 后按 FontChoice验证，没有“必须出现在系统枚举”限制。可识别 system family 由平台渲染，缺少字形继续系统 fallback。 |
| 界面 | `font_settings.dart:76-155`：当前家族/文件名、family输入、应用/导入/恢复默认、busy禁用与进度、失败提示；多语言预览 `Morrow · Aa 123 / 中文 · 日本語 · 한국어 · Русский`。 |
| 编辑器保留 | `test/font_settings_test.dart`：更改字体应保留 Studio 实例、已打开编辑器、同一输入controller、文字/selection/composing。现有测试是参考需求，不是本轮已经执行的证明。 |

## 2. 当前 HMOS 实际缺口

`Appearance.ets:12-31` 仅 `font: string`。`Index.ets:2676-2712` 当前外观读取/写入没有字体 asset/name；root 同步正在改独立 AppearancePreferences，字体应接它的确认接口而不是另建无保护的整体偏好覆盖。

`Index.ets:3746-3763` 的导入按钮 disabled；预览只有中文/英文/数字；恢复默认、应用直接改变 family 后调用外观写入。`systemFonts():3629-3633` 使用 `getSystemFontList()`。实际 SDK 声明该方法仅 PCs/2-in-1 有效，在手机返回空数组，因此这里只保证 `HarmonyOS Sans` 被列出；`FontSettings` 的 includes 白名单拒绝其他手输 family，比 Flutter 更窄。应保留系统建议列表作为建议，使用 FontChoice 的文本合同验证应用值，不以枚举为空证明手机没有其他字体。

图标字体在 `Index.ets:446` 单独 `MorrowIcons` rawfile 注册；UIFont不得重命名或清除该字体，也不能把其一个固定内建 rawfile 计为用户导入8family预算。现有很多 Text 与输入使用 `fontFamily || 'HarmonyOS Sans'`，应继续把已确认/已加载的 resolvedFamily 接入这些属性及独立页面，不替换输入controller或重新创建业务工作区。

## 3. Rust复用与边界

1. 直接移植真实 Windows `workbench_host/src/ui_preferences.rs:108-233` 的 FontPreference 校验和独立 core-owned 元数据存储。其 proto 字段 schema_version=1/family/asset/name、记录类型 `org.morrow.host.ui-font`、原ID `morrow-ui-font`、1024字节记录预算、type/format/raw protobuf重编码一致性、read-before-write、CAS、原 operation 再次调用，都有真实源证据。依赖 Windows host本体与channel不可整包装到HMOS；提取 portable模型/adapter并保留其行为。适配当前 HMOS development-unsealed HostRuntime 时必须使用项目实际 prepare/grant/receipt路径，不能伪造 Windows host flush/sealing资格。
2. HMOS当前保护 `morrow-host-*` 记录并在查询路径分别过滤/验证journals；引入 UIFont记录时应使用明确保留的 host identity，并同时从业务 cards、query_candidates及列表/统计中排除且验证该类型，拒绝通过普通 mutate/import/export 改写它。Windows `FONT_ID=morrow-ui-font` 的字面值不能不加检查直接照搬成普通用户卡片。源行为 parity 为独立UI元数据、不可混进想法/任务，而不是无条件保持同名ID。
3. 复用现有 `native.prepareFile(sourceFd,destinationFd,20*1024*1024)` 的通用有界 FD 捕获；NAPI在返回前 `F_DUPFD_CLOEXEC`，Rust只消费副本，write_all、SHA256、EOF、有界copy、destination.sync_all已有实现。它是 transport，不能冒称完成 SFNT 或 UIKit font validation。明确不要用 `prepareMusicFile` 的150 MiB、多文件/20份/512 MiB音乐缓存或音乐 import request来实现字体。
4. 新增独立纯 font/SFNT contract，优先 Rust `font_asset`模块：完整字节/20 MiB、SFNT大端头/table界限、SHA256对 expected_asset。可通过 owned FD read/inspection adapter返回 bounded metadata，避免20 MiB进入JSON/普通偏好。对算法不引入不同于实际 Flutter的静默修复，不重编码字体。ArkTS可共享同样规则做早期反馈，但 Rust/平台真实 parser承认决定最终加载。
5. UIFont仅保存 family/asset/name，不保存 provider URI/任意path/FD。字体原件独立 private fontnamespace持久保留；将 UIFont metadata记入core记录不等于fontbytes已经进入现有core blob迁移/导出合同。若以后需要跨平台库迁移，补显式验证资产搬运与绑定；本次本地导入不得声称迁移已经支持。

## 4. 推荐完整切片及资源所有权

### 模型边界

- `FontChoice`：三字段、UTF8/C0/C1/SHA256/互斥验证、稳定 resolvedFamily，clone/freeze候选。
- `FontFiles`：只处理真实单个picker、owned URI ticket、私有capture、完整验证/hash、原件归档、重启readback；独立于music/editor资产命名空间。
- `FontRegistration`：应用进程静态 Map<asset,Promise/outcome> 与 byte计数；调用前reserve，失败也计数，asset dedupe；注册串行，reset仅变 choice 不卸载或返还预算。
- `FontWorkbench`：confirmedChoice、candidateChoice、activeFamily、busy/error/load-unavailable、固定 original preference请求与确认状态；page/operation/foreground owner隔离picker回返与普通异步任务。
- `PlatformFontLoader`：API26采用 `import { text } from '@kit.ArkGraphics2D'`，获得全局collection，`await fc.loadFontWithCheck('MorrowFont_'+asset, 'file://'+verifiedPrivateAbsolutePath, 0)`。TTF/OTF为index0，TTC仍在本应用合同拒绝。该collection global handle非自己创建，不destroy/unload。

### 导入与确认顺序

1. busy/read-failed/unknown-pending gate通过后建立一个 owned picker ticket，使用当前 UIAbilityContext；maxSelectNumber=1、DocumentSelectMode.FILE、fileSuffixFilters=`['TrueType / OpenType|.ttf,.otf']`。过滤是选择辅助，最终按实际 bytes验字体。返回0个是取消；类型错误、重复、多个、异常URI不是默认取消。
2. 系统picker背景/前台只允许这次ticket在同page且确切前景回返时更新epoch；不得全局放宽异步owner。迟到返回只安全收回自己原件与FD，不能改新page/新choice或任何业务数据。
3. 开真实返回 URI READ_ONLY；创建自己private根目录下唯一temp文件，验证根/文件regular且非symlink；用通用prepareFile有界完整copy+SHA，close源/目标均需确认。选取displayName后按字体descriptor文本合同检查；不把任意路径直接拼入root，不以suffix判断字节有效。
4. 对捕获原件完整读取/有界inspect，验证12..20MiB、signature/tables/offset-length、hash匹配捕获reply。大小预查不能替代EOF/实际长度；私有文件变动、partial read、hash mismatch、close失败全部阻止注册/偏好写入。
5. 原件归档 `${filesDir}/hmos-font-assets/<sha256>.sfnt`。源bytes不修改、不压缩、保留原 hash。已存在目标仅可在 regular/NOFOLLOW、完整SFNT/hash/实际长度一致后复用；不truncate或覆盖无法确认的旧font。新temp flush/fsync+close后同目录原子publish，再核对目标。需要power-loss durable声明时加目录fsync能力；ArkTS rename只声明移动结果，不能虚构目录fsync。失败unknown或未知旧内容保留，显式报告/核对，不按文件名自动删除。
6. 平台加载前按实际captured byteCount reserve 8family/64MiB；一个asset只发起一次loader并复用固定Promise，任何注册失败仍保留charged outcome。串行等待 `loadFontWithCheck` Promise完成。必要时增强 isFontSupported做平台早期检查，但成功同样不证明所有字形；不得另用UIContext register同族造成双加载。没有glyph支持的语言使用系统fallback，不整包拒绝仅含Latin的有效字体。
7. 原件已归档且loaderACK后，以 fixed operation+original expected_revision+exact frozen FontChoice向RustUIFont写入。确认收到同choice、revision=original+1；Unknown保留原literal/operation/revision并显式核对/重试，不新建ID覆盖。启动/关闭page后选择本身成功不代表偏好保存成功。显示候选、已应用但未保存或确认后的当前choice要明确，不能更新“已保存”状态后才等待flush。
8. 偏好确认后接入所有正文、控件、settings/footer/music/lyrics/输入的activeFamily；图标MorrowIcons独立保持。恢复默认为FontChoice全空，加载无需asset但仍要走确认写入；不清除私有原件、不卸载font、不重建工作区、不改草稿/业务/IME/controller。

### 首次启动/重启与恢复

- 读取 UIFont descriptor时严格验证；读取失败不可自动写默认值覆盖。若原件缺失、损坏、hash不匹配、平台loader失败，保留原 choice/asset/name，以默认 activeFamily回退并展示不可用及显式重试/恢复默认。
- 每次进程启动重读private original、完整SFNT/hash后重新加载；不沿用上次SDK ACK，不信只剩asset的metadata。实际旧family字符串的兼容读取是空asset/name系统 choice，不自动改字体字节/丢未知外观字段。
- 同页restore/load和用户apply竞争使用generation；late restore不得覆盖用户新的confirmedChoice。普通async严格page+run owner。dispose停止UI写入，不卸载全局collection；未确认写入保留recovery literal。
- Failed register、字体加载挂起或字体偏好write Unknown应各自可见，不把它们降成“导入已取消”。没有可靠platform取消ACK时同族不重发；budget保留直到新进程。
- 8family/64MiB是已加载及失败attempt的进程额度；私有资产磁盘量不是这个budget。完整切片另外规划独立磁盘有界capture/ownedtemp数量，未知残留不自动清理；不要借用Music 512MiB/20文件并声称它是Flutterfontcontract。

## 5. SDK备选与限制

| 路径 | 本地权威证据 | 使用判断 |
| --- | --- | --- |
| `FontCollection.loadFontWithCheck` | API26 `@ohos.graphics.text.d.ts:1395-1419`，API23+，Promise/error25900001..08；本地HuaweiHTML2626..2655直接全局collection+ArkUI Text.fontFamily | 当前API26首选，有解析错误ACK，保留设备字体/缺字证据门槛。 |
| `UIContext.getFont().registerFont` | `@ohos.arkui.UIContext.d.ts:53-69`，void/异步/不支持并发；familySrc=`file://` sandbox存在可读 | 可兼容较低API；无法建立Promise加载完成合同，不能`await void`后写“字体已加载”。若使用须分开 issued与rendered，不猜delay当ACK。 |
| NDK `RegisterFontBufferByIndex` | drawing_register_font.h:112-130；API23，0成功/6空buffer/7零长度/8null collection/9损坏；globalcollection API14、不可释放 | 与Rust+NDK路线一致，可加入真实native font adapter：ownedFD→bounded original→hash/SFNT→buffer→globalcollection。需NAPI资源释放、buffer lifetime、native/ArkUI共享、真实设备验证，当前链接尚无native_drawing，不称现成可调用。NDK registerBuffer旧API11没有显式corruption9，优先23版本。 |
| native unload | drawing_register_font.h:133-146 / text.d.ts:1421-1440 | SDK存在，但文档要求用到此font的layout全部销毁重建，使用中卸载可缺字；本切片按Flutter预算合同不调用它，不能用卸载换无限导入破坏开放编辑器。 |
| `getSystemFontList` | UIContext.d.ts:71-86 | 手机返回空；建议列表不可充当用户family白名单。 |

## 6. 必要源码检查与测试

本轮没有执行这些测试；下列是实施时真实验收项，不是计数凑齐或仅测试自己写出的getter。

1. **descriptor/reference parity**：default/system/import；128/255 UTF8边界、中日韩/combining/emoji；trim/C0/C1；asset小写64hex、family与asset互斥、name规则、非法JSON不落盘。跨Rust与ArkTS调用实际decoder/validator，用现有Flutter样例核对，不维护一份不同规则的测试复制品。
2. **SFNT/readback**：真实合法TTF/OTF（有权使用的现有fixture）；12下限/20MiB上下界、截断目录、0/257tables、offset超界、length超界/uint32溢出、zero-lengthtable、wrongsignature/TTC/WOFF伪后缀、完整hash及复制后改变字节。纯28字节SFNT边界fixture只能证明validator，不能当合法可渲染font设备fixture。
3. **FD/private assets**：捕获limit强制20MiB、短读短写/interrupt、source/targetalias拒绝、fsync/close失败不confirm、noJSONbytes/URI偏好、symlink/未知target保留、sameassetdedupe、rename失败/unknown、captured长短与actualstat/hash一致、恢复缺失与hash变动。真实通用prepareFile和adapter被测试，不能只mock“capture success”。
4. **loader/session limits**：严格8family/64MiB等号边界，第9/超budget拒绝；sameasset并发只一次platformload；失败pending继续charged且同asset不重复；reset不返还；serialize不同assets；void API不伪ACK；ownerdispose/foreground/late-load/late-restore、新family竞态、parser rejection留旧choice。
5. **coreUIFont request**：独立记录不出现在普通cards/查询统计、普通mutate不能改hostrecord；fixed op/revision/descriptor/receipt、staleCAS、sameopdifferentpayload拒绝；Unknown后exact requestretry/historyread，重新打开后仍保留pending；原未知记录不覆盖，font写失败不重置其他外观/语言/草稿。
6. **UI integration**：导入取消不变；应用/导入/恢复默认互斥busy；当前文件名、字节错误/不可用信息；任意已验证systemfamily输入不依赖空枚举白名单；精确多语言预览；所有可读Text/input使用resolvedFamily、图标独立；已开编辑器实例/controller、selection/composition/text保持。
7. **SDK qualification**：两ABI新native若改Rust/NDK必须重建，不复用旧archive声称新adapter存在；实际API26 assembleHap需确认newfontmodules真入产物、源freeze/indexhash与artifact对应。通过source tests/API26构建仅支持对应范围。

## 7. 必要设备gate

真实设备/当前API26模拟器通过系统fontpicker选取一个现有授权/自造真实可渲染font，记录hostbytes/hash与private读取一致。候选的实际字体必须能区分fallback效果，不能只看到相同 `HarmonyOS Sans` 字体后宣称导入成功。

- 正常导入：picker返回→privatecapture→完整SFNT/hash→loaderACK→preferenceACK→界面中Latin及该字体包含的glyph确实改变；选择filter不是font权限/format证明。
- 显示中文、日文、韩文、俄文、数字和combining/emoji；不含glyph正确fallback，不遮住字，不出现方框替代应由fallback可显示的字。原图标字形保持。
- 设置页、主页/列表/详情、打开的编辑器、待办、音乐/歌词、窄屏/宽屏含长内容；字体切换不丢文稿、selection/IME、不重新创建businesssession。
- 导入取消、无效font、过大/截断font、provider读失败、save不确认、预算耗尽、同asset第二次导入；界面结果与实际状态一致，业务卡片数与四份现有草稿保持。
- force-stop/relaunch在已确认安装/操作范围内重启恢复：真实私有原件再次核验与loaderACK，新UI字体恢复；不得依赖host侧文件存在替代device读证据。
- 原件缺失/损坏测试只在独立ownedfixture/user允许测试资产上执行，不破坏已选用户font。保留偏好而activeFamilyfallback，显式恢复/重试或默认按钮的确认行为。
- 背景/前台、picker晚返回、关闭settings、fontrestore与apply竞态；没有oldowner把新choice覆盖或关闭新操作。当前musicprovider关闭现象若仍发生，也属于真实font选择的设备gate未通过，不能注入file URI代替选择证据。

## 8. 下一个可执行步骤

先实现 `FontChoice` + `FontFiles` + API26 `PlatformFontLoader/FontRegistration` + `FontWorkbench`，同时将Windows现有FontPreference提取移植到HMOS独立Rust记录并接真实receipt/CAS恢复。root正在完善的AppearancePreferences只负责保留旧font字符串兼容及未知字段，UIFont确认路径独立；不在本报告占用它的写文件范围。

FontSettings页在该完整控制器接好后启用导入，补当前文件名、多语言预览、busy/取消/失败/默认、任意合法systemfamily输入。完整tests/SDK冻结后再真实device验证。没有新确认需求时直接推进；不得以新增类或一组modelPASS替代这一完整verticalslice。
