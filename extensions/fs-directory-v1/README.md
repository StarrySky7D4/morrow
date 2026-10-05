# fs-directory-v1

当前C10检查点（2026-10-05）：本次开发分支更新收录C08–C10。C10已新增独立 `fs-directory-request-v1`、严格单import、包feature及只读discovery，复用原已批准selection和owner，不导出路径、句柄或新授权。Windows新Rust34（codec12/helper4/profile9/owner9）、既有回归191及frame5分别通过；Python33和两个原生C/C++消费者另计。新Rust Wasm仅编译通过，真实新Rust guest、C/C++ Wasm、Workbench产品／GUI、受保护Session与其他平台仍未验收。旧Core IO FileList保持Unsupported，SDK26／G04仍OPEN，无新Release。 详见[接口与实测边界](../../reports/reconstruction-2026-10-05/directory-request-sdk.md)。

独立实验 Rust/C/C++17 目录观察 codec 与有限状态 helper，version 1，raw schema SHA256 `ade60daee77497056a3fe618b38616331b8d5de61bdb494f703592e74ec3d5f7`。Windows Release 库内 13 个唯一方法通过；目录76个wire vectors（16接受/60拒绝）的Windows native三语言消费者及原生broker16方法限定资格通过；另有原件42和网络100兼容实测，没有新增Wasm或guest协商。G04/SDK26 仍 OPEN。

`FsDirectoryPage` 拥有条目和名称；`FsDirectoryPageRef` 借用完整 entry/name 表示。页含 selection_epoch、从 1 开始的 page_sequence 和 terminal。条目含 opaque entry_id、raw name、UTF-8/UTF-16LE、File/Directory/Other 与 File 可选长度。UTF-16LE 保留孤立 surrogate；不以有损转换生成替代名称。空名称、NUL、路径分隔符及 `.`/`..` 拒绝。名称、epoch 和 ID 是数据，不是路径、FileRead、递归或 mutation grant。

页上限 32 entries、合计 16,384 name bytes、65,536 wire bytes，有限 traversal/nesting，默认 allocator。`DirectoryState` 固定 epoch，检查连续页序、所有已接受 ID 唯一性和终页；硬上限 entries/pages 各 1,024、name/wire 各 1 MiB。admitted wire 先收费，语义失败不推进接受状态也不退费；release 释放 resident IDs、关闭 helper并保留计数。新 helper 不能恢复原来源预算或重新授权。

C 头文件 `include/morrow_fs_directory_v1.h` 提供 page/state opaque owners、checked views 和 codec/state 函数。create/decode 先拥有输入，错误保持输出；借用名称在 owner free/replace 后失效。C++17 `include/morrow_fs_directory_v1.hpp` 提供 move-only owners、失败保持的 vector 编码。内存有效性与独占访问属于原生调用者义务，大小/对齐检查不是指针隔离，普通 OOM 不承诺恢复。

库不依赖生产 Core/runtime，不创建选择、FileList 来源或公开 import。新的 Windows `DirectoryBroker` 是单独宿主接线：可信宿主提交已打开的 selected root，原 IoBinding 仍执行真实 admission。root handle 身份不是祖先或 picker 来源证明；root/child reparse 拒绝，hardlink 观察不意味着打开链接目标，目录观察不保证原子快照。原Core FileList仍Unsupported。C07已接原IoWorker的private捕获／分页／结束、Ticket合作取消与原clock idle维护；原owner实际join、预算与一次领取保持独立。C08在原owner工作线程新增 `capture_directory_fresh(File, CaptureLimits)`并限定验证factory14，115／42／100也分别fresh通过，见 [C08范围](../../reports/reconstruction-2026-10-05/directory-secret-factory.md)。该原生可信入口不改变独立codec；这些C08结果不证明picker／祖先来源；当前C09相对选择接线的限定范围见下文；legacy new(secret)本身不证明secret新鲜。普通合成Store／临时目录不提供真实用户目录或生产owner资格。

## C09 原worker相对目录选择（Windows限定）

C09已完成限定Windows Release／locked／offline资格，见 [C09阶段说明](../../reports/reconstruction-2026-10-05/directory-selection-owner.md)。可信宿主 `capture_directory_under(anchor, relative, limits)`只保留并核验原opened anchor到relative leaf的raw UTF-16句柄链，复用原worker FileList、原时钟、取消和预算；root加N个分量共享原8资源，32段语法上限不是可用深度。新selection_path8＋directory_selection12、C08 factory14、原owner九组115和原件42分别当前实际PASS；原件42为base9／dependency3／region7／reader主9／shared14，reader raw10含child helper1不加方法，region保留84过滤。17个credited测试进程合191 meaningful方法（raw192含child1），zero-match失败进程保留且不计功；这些数字不能作为SDK冻结。Workbench第二次Release x86_64 `--locked --offline --lib` check通过，首次缺offline asn1-rs0.7.2的exit101保留；只是编译检查，ProtectedSession／GUI／picker以上provenance和non-Windows产品执行NOT_RUN。C09 network100和Clippy明确NOT_RUN，不继承C08历史通过或lint结果。这不证明picker时刻、anchor以上来源或传入anchor的sharing策略，不增加guest FileList、目录guest或公共UI，blob耐久后端仍缺。SDK26／G04仍OPEN，公开FileList及conditional Replace仍Unsupported。C08/C09历史报告保留当时状态；本次开发分支更新收录C08–C10，无新Release。

这是runtime可信宿主入口，不改变本库wire/schema或Rust/C/C++ codec身份。相对分量保留raw UTF-16，32段／8192-byte语法边界不能放宽原8资源预算；原worker持有并前后核验anchor至leaf整链，实际drop后才归还资源，累计bytes费用不退。不会证明anchor以上路径、picker真实性或原anchor sharing策略，不会由entry ID或页面ACK推导递归、FileRead、rename、overwrite授权。

完整边界见 [接口说明](../../docs/PLUGIN_DIRECTORY_BLOB_SDK.md) 、[C06库记录](../../reports/reconstruction-2026-10-05/directory-blob-sdk.md)与[C07原owner范围](../../reports/reconstruction-2026-10-05/directory-owner-sdk.md)；最新总体状态见[项目状态](../../docs/PROJECT_STATUS.md)。本库不是 watch、recursive listing、read、rename 或 overwrite 实现；Unknown 不自动重放。
