# Windows x64 当前 Rust SDK 库限定资格 — 2026-10-03

一次 `cargo test --offline --locked --manifest-path sdk/rust/Cargo.toml --target-dir <新独占目录> -- --nocapture --test-threads=1` 完成，真实 Cargo exit 0。默认 Debug 测试 profile，不是整个应用构建或 Release 打包。Rust 报告92项 passed，0 failed/ignored/filtered，14个非空 suite、另0项 doctest。不把它称为92项 SDK 能力或整个 SDK 冻结。

| suite | 报告通过 |
| --- | ---: |
| unit | 17 |
| channel | 5 |
| channel_transport | 7 |
| codec | 8 |
| dependency_call | 9 |
| dependency_ffi | 5 |
| descriptor_guardpage | 1 |
| io_ffi | 6 |
| mutation | 5 |
| mutation_ffi | 7 |
| service | 7 |
| service_resources | 6 |
| ui | 4 |
| ui_alignment | 5 |

`write_explicit_smoke_fixture_when_requested` 是仅按需导出夹具的入口，本轮环境未启用其导出分支，Rust仍报告ok；这1项不作为新增功能路径证明。`c_digest_and_optional_native_fixture` 的 digest/bounds 断言实际执行，末尾可选导出分支未执行。两个 MORROW_SMOKE 环境项从子环境移除，以避免向继承路径写文件；没有以静默过滤或修改测试取得通过。

Windows guard-page 是单一测试，对本进程新分配的两页内存操作，验证七种 sized descriptor 在不可读页之前拒绝，未启动辅助子进程或打开真实数据库。只有本机 Windows x64 的 SDK 编解码、FFI、本地回调与合成内存边界获得本次证据。

## 身份与安全闭包

分支 `codex/windows-sdk-qualification-20261003`，HEAD `63f38d4a8a5bf453248dc7532197bee6980dc86f`。327份 SDK 原始输入逐字节等于该 HEAD，与之前 SDK-only 消费者输入一致；57份旧 base／transport 原件、8份 pending源码／工具、只读 Core schema、工具与锁在测试前后保持相同。测试闭包只读比较 schema，不初始化 Core/runtime/Workbench/protectedStore。其他阶段的 docs/report 更新不在此闭包，不宣称完整工作树未变。

锁 SHA-256：`395ffb285db0cf70876872b1f98c5fea95d84db0e1ccc7a798de7f648b6ed164`。
结果 JSON SHA-256：`de4c457b3eaf378c525aaac626616710019f08f5338505b188cd80a3b7e427e3`。
原封存 ZIP SHA-256：`15c02156d2beead8e9c9c3700262159bd4cf7c641cfa4d451e55f4977c471ba2`。

普通只读工具进程直接测得非管理员，实际 recorder 进程直接测得管理员。Cargo和test进程使用默认 CreateProcess token 继承；没有对其运行中的 PID 独立取样，结果中的token字段按这一限制解释，不称普通受限 token 下测试通过。封存后的独立 supplement 明确此限制，未重写原 ZIP。

首次 preflight 将允许的 capnpc::CompilerCommand 误匹配为待审 Command，是检查器误报，Cargo没有启动。修正词边界后在新目录执行本次唯一 Cargo 测试；原误报、raw logs、exit与检查器版本保留，不把 preflight 失败冒充 SDK 编译失败。

## 尚未获资格的部分

未重建／重打包 frozen guest或provider，没有执行三语言原件业务。后者有 [独立原件与消费者证据](windows-sdk-transport-consumers.md)，计数不相加。未运行原 committed-tool bounded runner、其他原生 codec 程序全集、正式模板／分发、第三方插件、Workbench/Dart channel、GUI、其他平台、普通 token、CI 或 Release；没有真实库、protected Session、DPAPI、账户密钥、外部 API或系统设置操作。

下一步：在新目录补全原生 codec／模板分发验证，核对原工具的准确身份；再推进生产channel及平台矩阵。各层独立判定稳定范围，不提前宣布整个 SDK 冻结。本阶段未提交或推送。
