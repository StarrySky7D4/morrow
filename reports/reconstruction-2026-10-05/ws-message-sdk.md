# C04 Windows 类型化 WebSocket SDK

2026-10-05。在云端 `468ef2e912ac74e5f97f0016a8729b7d5c1f5399` 的独立 Windows 候选上，先保留 [C02 复验](windows-sdk-revalidation.md)与 [C03 discovery](changes-sdk-discovery.md)的封存身份，再开发独立 `extensions/ws-message-v1`。本阶段完成有限的编解码、三语言真实插件和生命周期资格；完整 SDK 仍 OPEN。应用版本保持 `0.1.9-test.58+62`，当前源码没有提交或推送。

## 实现范围

复用既有 `network_node_stream_001/schemas/ws_message.capnp` 原始字节，version 1，SHA256 `2d2f3b1913060bd410d3bc608362c01d28cf7d0e3c1cdc82a5293abcaa696e6d`。新库提供 Rust 自有／借用消息、C 固定自有 payload 与 C++17 move-only 包装，支持 Text、Binary、Ping、Pong、Close。C ABI 与 C++ view 的所有权、错误输出不变、缓冲区重叠及上限分别验证；编解码不产生授权或网络执行能力。

新 Rust／C／C++ Wasm guest 只使用旧 task／channel import 与旧 required features。可信宿主仍按精确 package／selection／实例／live owner／目标／期限和预算批准一次受管源，再由原 Store 赢得严格 claim。ACK 保留收到的原 frame 字节、digest、sequence 和 cursor，不用重新编码后的近似消息替代。

准备工具使用三个互不重叠的外部输出／target 目录，拒绝重写旧产物、契约或锁漂移。选定的编译器、linker、packer、完整 WASI sysroot 文件集、源文件与原 SDK 输入均保存前后摘要；拒绝 sysroot 重解析点。13 个 registry 依赖的 name／version／source／checksum 与原 SDK lock 精确一致，工具不下载或替换工具链。

## 最终实际运行

| 范围 | 最终结果 | 证明边界 |
|---|---|---|
| 原冻结依赖／provider、base、映射、共享对象、owned reader | 42 个唯一 Windows 方法通过 | 原 dependency3／base9／region7／shared14／reader9；原 Wasm／package 不重建、不重封 |
| 网络执行 | 98 个唯一方法通过 | 原9个 test target 共96＋新 ws_sdk target2；重复冷启动运行不重复加数 |
| 新独立 Rust 库 | 6 个唯一 Release 方法通过 | 五种消息、边界、错误保留、alias与所有权；strict Clippy 与限定格式检查分别通过 |
| 新 guest 准备策略 | 11 个唯一 Python 方法通过 | 输出／target、契约、锁、Windows工具路径、重解析点与 sysroot文件身份；不是 guest 业务运行 |
| 原生参照、C11 与 C++17 consumers | 109 个 wire vectors：38接受／71拒绝 | 真实既有原生 codec，包含多段／far pointers、畸形与上限；不是109个方法 |
| close code | 65536个值，2011个允许 | 与固定原生后端一致，属于穷举值集 |
| 默认编码一致性 | C／C++各37份输出语义及默认allocator字节相同 | 另1份合法紧凑上限输入可解码，但默认allocator重编码返回 Limit；没有截断或更换编码策略 |

原生 conformance 共16个不同的实现检查族；C++重复执行C族、重复原生比较及遍历值均不重复计作新方法。上述范围分开计数，不合成“整个SDK通过数”。

三语言实际成功流程保持冷启动：start 后立即执行 guest。各自完成双向混合消息、原 frame 的五次精确耐久 ACK、一次 Close、实际 socket worker／broker producer／peer join。对端额外的自动 Pong 单独记录，不误计为额外 guest Send。

撤权与原实例 Stop 覆盖三个语言的六种组合。每次先观察对端收到恰好一条 Text，再触发控制；源返回原 `Source(Denied)`，无 ACK／guest output，原 operation 的 OutcomeUnknown 在重开普通合成 Store 后仍保持，重复 start 拒绝且只有一次握手。对端收到消息后，撤权可能先于本地 post-send guard；本地 successful outgoing receipt 的0或1据实记录，不把0解释为未发送、可重放或回滚。

## 初始失败与最小修补

新 lock 首次解析选出不匹配的 cfg-if，准备工具拒绝后只修复新 lock，原 SDK lock 保持。C++首次编译与工具版本字串的准备失败、MSVC archive 参数位置及原生 oracle 搜索路径失败均保存原日志与退出码。

新 peer 从 nonblocking listener 接收 socket 后，遗漏旧 helper 的 `set_nonblocking(false)`。真实握手返回 `HandshakeError::Interrupted`，源随之得到 Transport／Unknown、零入站帧与零ACK；不是 guest 收到有效内容后成功。先保留这次精确观察，再只修补新测试夹具的 blocking socket 模式，握手仍有8秒上限、后续读取仍为100ms轮询。成功测试不加就绪预热来掩盖冷启动。

新 C／C++ guest 显式处理契约允许的 Idle／ClosingUnconfirmed，仍受旧1024 host-call预算限制；这不是已定位的握手失败原因，当前 native continuation driver 会等待交付就绪。返回的原 frame 持有到精确ACK完成。之后用最终工具重新构建新 guest，再复跑网络回归。历史失败和旧新产物均保留；没有改写原生产错误映射或把失败断言降为成功。

## 恢复与未覆盖范围

本地阶段交接保存源增量、基线与最终 Git **tree** 身份、全量／相对C03补丁、原始命令／日志／退出码以及输入和产物前后摘要。独立 cached-index应用验证最终tree一致；tree不是commit，实际工作树index与原Windows开发分支保持。C02／C03有界档案按原哈希引用，不重打包。

SDK327／冻结57字节不变，新 payload 扩展尚未成为冻结全局 SDK 分发。普通用户 token、受保护 owner、用户真实数据库、DPAPI／账户、生产 Flutter批准链、公开网络／TLS、跨进程恢复全矩阵、其它平台、CI及完整SDK冻结均未运行。本阶段测试只使用普通合成数据与 loopback；资源 join、ACK、远端业务结果和正式内容提交是独立事实。

后续先补类型化 SSE 与独立源码分发的实际开发闭包，再推进目录／blob、异步组合及恢复矩阵。详见 [接入指南](../../docs/PLUGIN_WS_MESSAGE_SDK.md)与[完整门槛和顺序](sdk-next-gates.md)。
