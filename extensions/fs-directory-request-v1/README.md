# fs-directory-request-v1

独立实验Rust/C/C++17目录请求库，AGPL-3.0-only。接口位于 `src/`、`include/`，三语言新guest示例位于 `guests/`。原page库及SDK327保持原字节。完整SDK26/G04仍OPEN，当前范围见[C10报告](../../reports/reconstruction-2026-10-05/directory-request-sdk.md)。

Version1 raw schema SHA256：`04bc8c556059551c320df641f28bf8da5475737534f9d161ad2269bfe5b1d0df`。request上限512 bytes，outer response上限65,536 bytes。Open/Next/Finish/Cancel作用于原owner已批准的活selection；nomination/nonce/epoch/entry ID不是路径、句柄或授权。原宿主核验连续sequence、精确cursor、clock、累计预算和退休；Unknown不自动重放。

包声明 `io-v1`、`fs-directory-request-v1`、FileList-only，ABI2。固定extra import为 `morrow_fs_directory_v1.call(i32,i32,i32,i32)->i32`，不能混合其他extra import/WASI，原factory仍拒绝。仅新目录Runner及原managed owner可驱动。

此profile使用raw task：输入为原Open request，完成值等于最后response。read_input必须提供131,072-byte完整可写容量，即使实际输入不超过512 bytes；standard_typed_task_helpers=false。原通用typed task envelope/helper不适用。

旧Core IO FileList仍Unsupported；page/IO schema、base与原四extension discovery不改，新记录独立置于 `experimental_extensions.directory_request_discovery`。元数据不授权，production public binding=false，Workbench routes为空。

此前 Windows codec 12／helper 4／package 9／owner 9 及两个 native C/C++ 消费者已通过。2026-10-06，新构建并分别核对独立 SHA256 的 **Rust／C／C++ Wasm guest** 已在原 managed owner、普通 Store 与合成 TempDir 上实际执行：`directory-windows-004` 共 **12 项通过，即三语言各四案**，覆盖 70 条目录项的三页完成、FileList 独立批准、外国 worker 的 selection 拒绝，以及 Ready 输出取消后 Unknown／不重放／字节费用不退／原 owner 实际 join。初次 `directory-windows-003` 的 3 PASS／9 FAIL 源于测试错误假设 prepare 只发生一次，原始失败记录保留。详见 [Windows C11 限定复验报告](../../reports/reconstruction-2026-10-06/windows-sdk-c11.md)。

真实 guest 组须显式启用 `directory-guest-qualification` 并运行 `directory_sdk_guests`；普通 `packages` 测试不要求这些外部 Wasm 产物。复验须使用报告所绑定、经审阅的新构建记录，分别提供下列六个环境键；三个路径必须是绝对路径，三个 SHA256 必须来自独立审阅的产物身份，不能用临时重算摘要替代 pin。

| 语言 | Wasm 绝对路径 | 独立 SHA256 |
| --- | --- | --- |
| Rust | `MORROW_RUST_DIRECTORY_REQUEST_WASM` | `MORROW_RUST_DIRECTORY_REQUEST_WASM_SHA256` |
| C | `MORROW_C_DIRECTORY_REQUEST_WASM` | `MORROW_C_DIRECTORY_REQUEST_WASM_SHA256` |
| C++ | `MORROW_CPP_DIRECTORY_REQUEST_WASM` | `MORROW_CPP_DIRECTORY_REQUEST_WASM_SHA256` |

下面的 `$RustWasm`／`$CWasm`／`$CppWasm` 及三个 `$Reviewed…Sha256` 从同一批经过审阅的构建记录取值；`$RuntimeTarget` 为仓库外独立输出目录。先准备 Windows 工具链、Cap’n Proto 编译器及锁定离线依赖，再从候选仓库根目录执行：

```powershell
$env:MORROW_RUST_DIRECTORY_REQUEST_WASM = $RustWasm
$env:MORROW_RUST_DIRECTORY_REQUEST_WASM_SHA256 = $ReviewedRustSha256
$env:MORROW_C_DIRECTORY_REQUEST_WASM = $CWasm
$env:MORROW_C_DIRECTORY_REQUEST_WASM_SHA256 = $ReviewedCSha256
$env:MORROW_CPP_DIRECTORY_REQUEST_WASM = $CppWasm
$env:MORROW_CPP_DIRECTORY_REQUEST_WASM_SHA256 = $ReviewedCppSha256
cargo test --manifest-path plugin_runtime/Cargo.toml --locked --offline --features directory-guest-qualification --test directory_sdk_guests --target-dir $RuntimeTarget -- --test-threads=1 --nocapture
```

这 12 项只证明 Windows 合成选择上的三语言实际消费与原批准／生命周期边界。完整 SDK26／G04 仍 OPEN；生产 GUI、ProtectedSession、系统 picker 来源、真实用户数据、第三方分发与其他平台本轮 `NOT_RUN`。旧 guest/provider 未由本阶段重建，原冻结 SDK 输入与锁文件保持；本结果不构成完整 SDK、平台或产品验收。

构建方法与单一Rust runtime约束见[FFI support](guests/ffi-support/README.md)。
