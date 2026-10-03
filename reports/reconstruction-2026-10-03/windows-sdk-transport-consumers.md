# Windows SDK transport、服务生命周期与独立消费者续验 — 2026-10-03

本阶段完成旧 transport 六包的 Windows 实际执行、76 个 runtime 服务测试和三语言 SDK-only 消费者验证，并修复 HTTP 测试资源释放顺序。结论为限定通过，整个 SDK 未冻结。此前的 [接管报告](windows-sdk-takeover.md) 保留原 base9／dependency3／shared-descriptor 证据与范围。

## 来源与修复范围

分支 `codex/windows-sdk-qualification-20261003`，基线 `63f38d4a8a5bf453248dc7532197bee6980dc86f`，base tree `a8a74bc4563fae735e9b695f50db4e979ac1393d`。各阶段绑定当时的未提交增量，base commit 不能代表增量后的完整树。

此前的 native registry 与两份 dependency fixture 修补之外，本阶段仅新增五个文件的调整：

- `network_node/Cargo.lock`：补齐现有 manifest 的跨目标解析，245 个已有包的版本、来源与 checksum 不变，新增 43 项。未改 manifest、SDK 或契约；Windows locked 解析其他目标不等于 Linux 执行通过。
- `network_node/tests/managed_http.rs`：Running 的临时目录字段移至最后，让 host/worker/数据库资源先释放；新增实际 Windows 删除目录回归。
- `tool/windows/verify_sdk_native_bounded.py`：consumer workspace 与复制 SDK workspace 分开 target，C／C++ 执行传入合成 vectors。
- C／C++ consumer fixtures：补足两次成功的自有 64 KiB 响应、失败不重试、异常／非法输入与生命周期断言，拒绝 NDEBUG 关闭断言。

公共接口、Schema、provenance gate、源码 pin、原始 guest/provider 和 Linux process/controller/recovery 均未修改；原插件没有重编译或重打包。

## 实际结果

| 阶段 | 结果 | 范围 |
| --- | --- | --- |
| 原 transport 容器检查 | 1 通过，exit 0 | 六包内嵌模块与固定文件一致，不计 guest 执行 |
| 原始服务包执行 | 2 通过，exit 0 | 三语言原 service 包、本机 TCP、七方法、续租、撤权、期限、Unknown 与重开 |
| 原始 HTTP 包执行 | 4 通过，exit 0 | 三语言原 IO 包、本机 HTTP、二进制/header、固定合成 header 注入、429、发送前拒绝、Unknown 不重放及重开 |
| runtime 服务生命周期 | 7 套／76 个唯一测试，各 exit 0 | service_io24、binding8、budget7、broker budget2、renewal12、owned renewal11、durable jobs12；脚本后端，不算真实 HTTP |
| HTTP 目录删除回归 | 修前失败 exit 101；修后 1 通过 exit 0 | 实际资源释放后删除，修后零临时文件残留 |
| 修复后的原 HTTP 执行 | 同四项通过，exit 0 | 输入与57原件前后恒同，零临时残留；不重复加算四项 |
| SDK-only 消费者 | Rust／C／C++ 三程序通过 | 各两次成功、一次回调失败，总三次回调；C 坏 ABI、C++ 超限均不增加回调 |
| 当前 LF C／C++ 复验 | 编译与运行均 exit 0 | 精确落盘源码重编，复用同 SHA 的 SDK DLL/对象/vectors |
| C 的 NDEBUG 负向编译 | 预期 exit 2，C1189 | 禁止静默删除断言，没有生成或运行负向程序 |

消费者完整 metadata 的15包闭包仅含 SDK、consumer 和锁定缓存依赖，没有 Core/runtime/audit/Workbench 源依赖。327份原 SDK 输入、78份复制输入与11份合成 vectors 各自不变。MSVC 使用 /utf-8 /W4 /WX /MD；执行 token 为管理员，不宣称普通受限 token 通过。SDK-only DLL 的 SHA-256 为 `890e50b6ba88ba55f2c0ebbd6af751496db8231ee186eb4f3df4782d225d01bb`。

本阶段经过独立只读复核，关键日志、raw exit、源码、复用产物与旧 ZIP 均核对通过；复核不重新运行测试。

最终八份源码／测试／工具的 SHA-256：
- `core/src/plugin_package/registry/native.rs`：`05a102dec6bb2b449a7572c443534af1938351966e225591470a8cc2fd876beb`
- `plugin_runtime/tests/dependency_graph.rs`：`36becd69a3306fe3b94452aa131daf9dcc3c5e8d85bacb0fe5b1cc8032603d27`
- `plugin_runtime/tests/dynamic_dependencies.rs`：`921e9e6a62c50f752664010ee7aa2dc8f4dedbd1c7ab3d5263eee35f29da5e64`
- `network_node/Cargo.lock`：`821993b4b38d47c3f36f8c3717e3ab2eb54f8dbe24156d9e0282f53b8f582d60`
- `network_node/tests/managed_http.rs`：`d8ef92cc9ce91d01efbf4fe5ecb0353a1a5b4544154373fcebc15efb6d3323ab`
- `tool/windows/verify_sdk_native_bounded.py`：`72b6ebbc96cfa3d8819532ab302d6c47b41b1d75c0da712b2c9f802c06143bfa`
- `tool/windows/fixtures/sdk_consumer.c`：`0b8eae3cd91931947125ade6674abf7bedb13cd2f41e9a0658bd806482b6667b`
- `tool/windows/fixtures/sdk_consumer.cpp`：`690b25814ba3319fd5b6497df2d931239e06bd7688f2509390779c03d3f6ec70`

## 失败与历史证据保留

首次 network locked 因旧锁缺少 manifest 依赖而 exit 101，零测试执行；离线候选解析及既有包无漂移比较单独保留。早期 HTTP 四项虽通过，但留下36目录／72个合成 registry 文件，共4,656字节；已补独立更正，旧封存包保持原样。新修复先复现失败，再证明零残留，不追溯改写旧结果。

runtime attempt001 的 MSVC quoting 前置失败、attempt002 recorder 的 import 缺失及真实 Cargo exit 丢失均保留。第二次 stdout 24 passed 不计严格通过；新 attempt004 保存 raw exit 后仅补验24项，其他52项没有重复，不能补造历史退出码。

消费者首轮是 CRLF 输入，候选应用为 LF，已在新目录复验精确 LF 字节。旧产物、日志与早期 patch check 失败保存。独立 driver 不等价于原 bounded runner。

## 下一阶段与限制

1. 补当前完整 Rust SDK 库测试、其他原生 codec、模板和分发包；SDK-only 复制源不是正式冻结分发。
2. 变更后的 bounded runner 需准确已提交工具身份与新全流程证据。当前增量未提交，snapshot gate 继续拒绝错误身份；未绕过，也未宣称原 runner 通过。原 Rust fixture 仍仅两次成功，独立 driver 的失败证明不自动迁移到原入口。
3. 本候选尚未复验生产 Workbench/Dart channel 与 GUI。队列内权限核验及失败后 Stop 已在源码存在，不能当成仍未实现；历史20项11通过/9失败不自动升级为当前通过。
4. 完整文件系统、网络流／异步组合预算、跨重启 Unknown、第三方接入和全平台仍有未覆盖项。Linux、其他架构与普通 token 资格独立验证。
5. 保留 GUI 离线 artifact 与历史隐式下载缺口。本阶段未运行 Flutter、真实库、protected Session、DPAPI、账户密钥、外部 API、CI、提交、推送、tag 或 Release。stage16 丢失增量未恢复；CCswitch/Codex 接入仍在 SDK/platform 验证之后。

本地交接包含分阶段 raw logs/exits、源码／插件哈希、独立复核、完整 pending source manifest、Git tree 身份及有界增量包。重复计数不相加，编解码通过不代表 SDK 或产品冻结。
