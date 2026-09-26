# 入站服务 SDK 实现与进度更新

日期：2026-09-26（Asia/Shanghai）。基线：`d9c043191a400df972832d398b80cb73dfc51f56`，主线 test.56，应用 `0.1.9-test.56+60`。开发分支：`codex/plugin-service-sdk`。

本轮已完成有界 Rust／C／C++ 入站服务 SDK、对应生成原包与 Linux 真实节点验证。完整插件系统尚未完成；不创建标签或 Release，未运行 Actions/CI。应用版本、核心 wire/schema、数据库格式和旧冻结原件均不改变。

## 实现范围

1. 复制并核对现有 `service.capnp`，加入 SDK 构建摘要。Rust 提供 Request/Invocation/Reply/Response 和服务专用读取／完成函数，复用 `morrow_task_v1`，不新增 host import。
2. C 提供拥有原帧、字段及头数组的不透明句柄；C++ 提供不可复制、可移动的 RAII 包装。视图借用句柄，响应按原始字节摘要关联；释放输入缓冲不会破坏请求身份。
3. 保留 UInt64、重复头、二进制正文与原始顺序；限制 128 KiB 帧、64 KiB 正文、64 头／16 KiB 头预算，拒绝敏感头、传输头、畸形目标、无正文状态携带正文、错误 schema 与关联。
4. 新增三语言 `--kind service`：生成 `service.echo`，显式声明 HttpListen/HttpPublish。核心打包 CLI 要求 `--service`，设置既有服务摘要，并输出声明与限额。未声明、漏项或混用其他 profile 均拒绝，不授予权限。
5. 本地资格脚本生成三语言原包并以原声明执行，不修改包或扩大预算。保存实际包摘要、构建日志和排除范围，各 Rust workspace 使用独立目标目录。
6. 修复两个既有 SDK 负面测试 helper：静态 fixture 不保证 8 字节对齐，改用 OwnedSegments 读取后再修改字段。没有改生产内容／UI codec 或 fixture 字节。

开发文档见 [SERVICE_API](../sdk/SERVICE_API.md)、[插件状态](../docs/PLUGIN_SYSTEM_STATUS.md) 与 [项目工具](../docs/PLUGIN_PROJECT_TOOLS.md)。开发看板、未来路线、服务实施计划、SDK README 和兼容说明已同步。

## 本轮实际验证

执行环境：Linux x86_64；Rust/Cargo 1.98.1；Cap’n Proto 1.2.0（固定上游 tag 源码构建）；WASI SDK 34.0；原生 GCC/G++ 13.3.0；Python 3。依赖使用仓库锁文件，首次下载后本地测试。没有 Windows、Flutter 或设备构建。

| 检查 | 实际结果 | 范围 |
| --- | --- | --- |
| SDK Rust | 60 passed、0 failed、0 ignored | 含新增 7 项服务 codec；原 IO、任务、依赖、UI 及非对齐回归 |
| 严格 Clippy | 通过 | SDK `--all-targets -- -D warnings` |
| Python 项目工具 | 32 passed | 包含三语言六类模板生成和服务 profile 拒绝；不是所有 18 种原包运行资格 |
| 契约与固定原件 Python | 16 passed | service-only 漂移检测、固定旧原件保护 |
| 核心打包 CLI | 11 passed | 显式 service 摘要、声明限额、漏项及隐式升级不发布 |
| 独立 host/SDK codec | 2 passed | 核心编码→SDK→核心响应；精确高 UInt64；不同 Cap’n Proto 分段不能替代原帧摘要 |
| C/C++ 原生互操作 | 1 项覆盖两种语言，passed | GCC/G++ 严格编译，句柄自有字节、移动语义、拒绝非法状态/小缓冲；核心验证最终输出 |
| 生成原包 | Rust/C/C++ 均构建、打包并准备成功 | 源码经 `new → pack`，同一归档原字节进入真实节点 |
| 真实节点 | 1 项覆盖三语言×七方法，passed | loopback TCP→认证→原 managed worker→guest→HTTP；二进制回显；无批准绑定拒绝、错误 token 401、撤权 403、正常关闭及 Store integrity |
| 旧冻结 guest 实跑 | 9 passed | 三语言内容／转换／UI；未重编译或重打包旧原件 |
| 固定原件摘要 | 36 文件／13 对原件通过 | Windows 专属旧依赖三项未执行，不计入完整十二项资格 |

原生用例在默认 codec 测试中标记 ignored，随后设置两种实际可执行文件路径显式运行，最终通过。Windows-only 测试在 Linux 为零用例，明确属于未执行，不将退出码成功当成该平台通过。上述分项不能相加为全产品通过率。

可审查回执保存在 [evidence/plugin-service-sdk-2026-09-26](evidence/plugin-service-sdk-2026-09-26/)，真实节点、SDK、原生 codec、工具及包摘要分别列示。

### 最终生成包摘要

| 语言 | `.mplugin` SHA-256 |
| --- | --- |
| Rust | `c6db60c22a1807bff91047af67aa5ffee2b5c31b565e4d8b89d38d4076672cf9` |
| C | `1c0b53f418078120a1a32c22a754259123350ef7cb88e2dc3b9302cf314212cc` |
| C++ | `a161fecf034f2f2541f71ca9b8344211fb923a852fcaf818a95233566039bc6e` |

这些是本轮实际生成与运行产物，不是新冻结兼容基线，不主张跨机器构建摘要一致。完整路径见证据目录 `packages.json`，构建树不纳入源码提交。

## 首次失败与处理

- 初次 SDK 全测两项负面 codec 测试因静态 fixture 的 UnalignedSegment 失败，测试尚未抵达预期拒绝分支。两个测试 helper 改为拥有对齐存储的读取后，60 项全过；冻结字节未变。
- 首次真实节点夹具请求 4 MiB 总额度，超过生成包的 1 MiB 声明，worker 正确以 InvalidOptions 拒绝。修正夹具到原包 1 MiB 上限；未修改包预算或放宽宿主。
- 多个独立 Cargo workspace 共用临时 target 时出现同版本 Prost trait/缓存产物不一致诊断。为每个 workspace 使用独立 target 后编译及完整最终流程通过，没有改动依赖版本或业务代码。
- 临时环境未预装 Rust/Cap’n Proto/Clang，使用独立目录准备工具链；最终测试不是只读源码推断。工具链准备失败不计作产品测试失败。

## 复验入口

```sh
python3 tool/verify_plugin_service_sdk.py --sysroot /absolute/path/to/wasi-sysroot
# 首次未缓存 Cargo 依赖时显式添加 --allow-network。
python3 -m unittest discover -s tool/tests -p 'test_plugin_project*.py' -v
python3 -m unittest discover -s tool/tests -p 'test_sdk_*.py' -v
cargo test --offline --locked --manifest-path core/Cargo.toml --example plugin_package
cargo clippy --offline --locked --manifest-path sdk/rust/Cargo.toml --all-targets -- -D warnings
```

本轮完整主入口使用 `--output-root build/service-qualification-final-2026-09-26 --allow-network`，WASI sysroot 来自 34.0。脚本拒绝复用已有证据目录；再次验证使用新目录。

原生 C/C++ 额外检查（Linux，`SDK_TARGET` 指 `cargo build --manifest-path sdk/rust/Cargo.toml --target-dir ...` 的目标目录）：

```sh
gcc -std=c11 -Wall -Wextra -Werror -Isdk/c/include sdk/tests/c_service.c \
  -L"$SDK_TARGET/debug" -Wl,-rpath,"$SDK_TARGET/debug" -lmorrow_plugin_sdk -o build/c_service_test
g++ -std=c++17 -Wall -Wextra -Werror -Isdk/c/include -Isdk/cpp/include sdk/tests/cpp_service.cpp \
  -L"$SDK_TARGET/debug" -Wl,-rpath,"$SDK_TARGET/debug" -lmorrow_plugin_sdk -o build/cpp_service_test
MORROW_SDK_SERVICE_NATIVE_C="$PWD/build/c_service_test" \
MORROW_SDK_SERVICE_NATIVE_CPP="$PWD/build/cpp_service_test" \
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml \
  --features packages --test sdk_service_codec -- --ignored
```

## 后续边界和顺序

1. **入站 SDK 产品接线**：当前模板只有有限 IO profile；补显式长时 service-run 及预算选项、原拥有者配置/续租和 Windows/Flutter 实际运行。继续区分声明、批准和运行授权。
2. **服务恢复与兼容**：生成原包接入 TLS、持久请求 journal/Unknown、跨进程重开和撤权故障链；另建服务/IO 固定原件，不更新旧 guest-v1-rc1 来掩盖缺口。
3. **完整文件 IO**：目录、创建/替换/删除、分块写入、平台资源授权和有外部效果的核对路径。
4. **异步与流式**：异步续接、全链证据及隔离重放，然后 OAuth/多账号、上传下载、SSE/WebSocket；不得直接删除现有互斥 profile 防线。
5. **平台及产品开放项**：Windows 专属旧依赖三项、Flutter 窗口、正式编辑器 autosave 与各平台独立资格继续保留。

本次改变的是外部开发者可用的有界服务接入层，不构成完整文件系统、长任务恢复、通用原生插件执行域或全平台插件系统完成的声明。
