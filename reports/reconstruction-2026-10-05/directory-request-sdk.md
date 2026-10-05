# C10 独立目录请求 SDK：Windows 限定验证

日期：2026-10-05。开发线为 `codex/windows-sdk-convergence-20261005`，增量基线为提交 `772466177fe589cee53bc633e69f411c34610104`。本次提交收录C08–C10，身份以分支历史为准。应用 `0.1.9-test.58+62`、原SDK327和冻结57保持；不生成安装包或Release，SDK26/G04仍OPEN。

## 接口与权限

新增 `extensions/fs-directory-request-v1`，提供独立Rust、C和C++17 codec/state接口。request version1 raw SHA256为 `04bc8c556059551c320df641f28bf8da5475737534f9d161ad2269bfe5b1d0df`；原page/IO schema不改。包采用ABI2、`io-v1`、`fs-directory-request-v1`、FileList-only及单额外import `morrow_fs_directory_v1.call(i32,i32,i32,i32)->i32`。新 `Runner::new_directory_task` 独立准备；原六factory拒绝该import及混合扩展/WASI。准备不执行guest，旧Core IO FileList仍Unsupported。

Open只领取可信宿主已批准且仍有效的原selection。本次调用的opaque nomination不导出路径、原生句柄、原serial或捕获/新授权。Next保留原epoch、连续sequence、精确after、原clock和累计预算；空非终页允许after=None，不能跳过sequence校验。

原worker直接分派目录页，不在同一worker排队后等待自身。实际wire费用沿原IoJobLease计费，失败不退款或更换预算上限。已推进后的交付丢失为Unknown并退休，禁止自动重放。Finish/Cancel或terminal不证明实际join；原资源和未回收会话仍占预算，实际owner停止/join/回收单独执行。

## Raw task与discovery

输入为原Open wire（最多512 bytes），完成值等于最后response wire（最多65,536 bytes；内部page也最多65,536 bytes）。任务read_input保留131,072-byte完整可写容量，`standard_typed_task_helpers=false`，不能替换为原typed task envelope/helper。

新可选记录为 `experimental_extensions.directory_request_discovery`。原base、四extension记录和feature_names不改。静态准备已实现，Windows native adapter已编译；production_public_binding_available=false、workbench_routes=[]。discovery只读，不产生授权或平台产品资格。

## 实际结果

下表来自普通合成Store/临时目录中的Windows x64 Release、locked/offline产物实际执行。机器可读摘要见[验证记录](directory-request-validation.json)。各组原始stdout/stderr、退出码、可执行文件和176源输入的前后哈希已本地保存。

| 范围 | 当前结果 | 计数边界 |
|---|---|---|
| C10 Rust codec/helper/package/owner guest | PASS，12/4/9/9，共34方法 | 修正后的owner九方法完整重跑；首跑6通过/3 WAT夹具解析失败不另计 |
| 既有选择和owner回归 | PASS，selection12+spelling8+factory14+owner115 | 149方法；不叠加C08/C09历史重跑 |
| 原始SDK定向回归 | PASS，base9+dependency3+region7+reader主9+shared14 | 42；原guest/provider未重建；region84过滤，reader raw10含child helper1不另计 |
| 既有frame路由 | PASS，5方法 | 独立回归，86过滤 |
| Python discovery/preflight/directory profile | PASS，33测试 | 7新directory+26既有，不与Rust相加 |
| Native C/C++17 | PASS，两个实际消费者 | MSVC严格构建与真实执行；不是Wasm资格或额外Rust方法 |
| Runtime/Workbench library | 编译PASS | Workbench仅Release locked/offline library check，产品未执行 |
| 新Rust示例Wasm | 编译PASS，wasm32-unknown-unknown | 真实新Rust产物消费者执行NOT_RUN；WAT owner测试不代替新Rust产物执行 |

17个既有回归进程共191 meaningful方法，raw192含reader helper1；加新34和frame5，限定Rust native共230 meaningful方法。Python33、C/C++两消费者和编译单独计数，不能作为全SDK完成度。

首次runtime compile exit101、C/C++编译参数顺序exit2及WAT夹具首跑exit101保留；后续最小修正和PASS分别记录。C10整库Clippy/完整格式检查NOT_RUN；C08历史library Clippy exit101不是本轮结果。

## 未验收及下一步

新Rust真实产物端到端、C/C++ Wasm编译和运行、独立第三方分发/安装、受保护Session、Workbench产品/GUI、picker及anchor以上来源、其它平台均未验收。C09相对句柄链只证明anchor到leaf，不能扩大来源证明。目录观察不授予FileRead、递归、watch、rename或删除；conditional Replace仍Unsupported，不降级overwrite。

下一步验证真实三语言新guest，再推进产品批准链、blob durable backend/history/upload/watch/rename、恢复和平台矩阵。生产secret生命周期、实际join及Unknown核对维持独立门槛。本次只保存开发状态，不恢复已暂停的持续开发任务，不运行CI或发布。
