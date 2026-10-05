# fs-directory-request-v1

独立实验Rust/C/C++17目录请求库，AGPL-3.0-only。接口位于 `src/`、`include/`，三语言新guest示例位于 `guests/`。原page库及SDK327保持原字节。完整SDK26/G04仍OPEN，当前范围见[C10报告](../../reports/reconstruction-2026-10-05/directory-request-sdk.md)。

Version1 raw schema SHA256：`04bc8c556059551c320df641f28bf8da5475737534f9d161ad2269bfe5b1d0df`。request上限512 bytes，outer response上限65,536 bytes。Open/Next/Finish/Cancel作用于原owner已批准的活selection；nomination/nonce/epoch/entry ID不是路径、句柄或授权。原宿主核验连续sequence、精确cursor、clock、累计预算和退休；Unknown不自动重放。

包声明 `io-v1`、`fs-directory-request-v1`、FileList-only，ABI2。固定extra import为 `morrow_fs_directory_v1.call(i32,i32,i32,i32)->i32`，不能混合其他extra import/WASI，原factory仍拒绝。仅新目录Runner及原managed owner可驱动。

此profile使用raw task：输入为原Open request，完成值等于最后response。read_input必须提供131,072-byte完整可写容量，即使实际输入不超过512 bytes；standard_typed_task_helpers=false。原通用typed task envelope/helper不适用。

旧Core IO FileList仍Unsupported；page/IO schema、base与原四extension discovery不改，新记录独立置于 `experimental_extensions.directory_request_discovery`。元数据不授权，production public binding=false，Workbench routes为空。

Windows codec12/helper4/package9/owner9及两个native C/C++消费者已通过。新Rust示例仅编译到wasm32-unknown-unknown；真实产物执行、C/C++ Wasm、第三方分发和产品界面均未验收。原guest/provider未重建，提供新源码不表示三语言产品资格已通过。

```powershell
cargo test --manifest-path extensions/fs-directory-request-v1/Cargo.toml -p morrow-fs-directory-request-v1 --target x86_64-pc-windows-msvc --release --locked --offline
cargo build --manifest-path extensions/fs-directory-request-v1/Cargo.toml -p morrow-fs-directory-request-guest --target wasm32-unknown-unknown --release --locked --offline
```

要求对应Rust target、锁定依赖缓存和Cap’n Proto编译器可用。输出放在独立target目录，不能覆盖冻结guest/provider。命令不表示完整SDK资格。
