# C28 阶段 21：公开 guest 构建、静态接口核对与会话资格准备

截至 2026-10-10，公开 guest 第三次后继的 metadata、实际编译和外层验证均退出 0；新 Wasm 已完成独立产物与静态接口读回。真实七步会话资格仅完成测试及控制器源码审核，尚未执行。

本次开发分支更新仅同步七份脱敏进度文档。产品源码与此前开发提交 `98ad75dc5d6b7f6931242f218593d5976000ad64` 一致，应用版本保持 `0.1.9-test.58+62`。新 Wasm 二进制及本机操作材料不在本次公开载荷中。

## 已通过的限定验证

| 工作 | 实际结果 | 验收边界 |
| --- | --- | --- |
| 三个 Cargo Git checkout 的来源与固定元数据 | 第二轮验证退出 0；2,904 个 loose objects、选定 commit/tree 闭包、2,662 个 tracked files、索引和 TREE cache 通过；报告及身份完成独立有限读回 | 缺少历史 parent objects，不是完整历史 fsck；不补记旧生产者后置守卫通过，也不验收整个 SDK |
| 匹配的当前库、runner 与 setup 离线构建 | 第三轮 Cargo 与外层均退出 0；完整新 Cargo home、原始输入和物理集合守卫通过；三个产物及 169 个源码文件的当前字节、日志与终端记录完成独立读回 | 未运行 runner、setup、原始库测试、VM 或生产沙箱 |
| 公开 guest 第三次构建 | metadata 与 Cargo build 均实际退出 0，外层退出 0；原始输入前后相等、源码物理集合一致、完整新 Cargo home 后置守卫通过 | 只接受新 guest 的本次构建和固定路径检查；没有运行 Wasm，不是行为或发布资格 |
| 新旧 guest 静态调用接口 | 三个 imports、64 个 exports 及其中 61 个函数签名、memory/table 声明和 target features 对齐；无 start section | 两个布局全局值发生变化，需动态布局；静态解析不能代替引擎验证、实例化及真实操作 |

helper 产物身份保持此前记录：

| 产物 | 字节数 | SHA256 |
| --- | ---: | --- |
| `libcodex_windows_sandbox.rlib` | 22097724 | `a48ca6c612f8da64289c5646f9b879f23729fcc89e741d970a52904abcbba983` |
| `codex-command-runner.exe` | 24378368 | `c75530dbc7b0c789d4908c02628bafdf66cb73e46b109f1eeecc35e11dcebf8a` |
| `codex-windows-sandbox-setup.exe` | 37725696 | `3072a6c5443c6ec11fbaac266a66e88f0539fba34a6d8738c0251a526161de3d` |

这些哈希对应已保存的新产物，不表示三个程序已经形成获准运行的安装包。旧 runner/setup 的来源不匹配记录继续保留。

## 新 guest 身份与构建边界

新 Wasm 为 **425,912 字节**，SHA256：`cca04ebb2e787f69e84ec7260aca3e93ec895ec17b68afbb660e3c6896ae2f2b`。原冻结 Wasm 为 427,258 字节，SHA256：`b1f0f44ad2bbe2abc89eee0ee971683419bf3407f370194f5557df57fa45799e`。新旧字节身份不同，原锁、原冻结产物和 `SESSION_SHA` 均未改写。

本次以原 27 包锁为来源，严格检查 wasm32 目标过滤后的 25 packages / 25 resolve nodes：只允许排除固定的 `cpufeatures 0.2.17` 和 `libc 0.2.189`，同时核对唯一包 ID、实际根节点和依赖目标闭包，没有放开任意子集或版本漂移。目标过滤图不等于完整主机编译图；主机 build-script/proc-macro 路径实际编译原锁中的 cpufeatures，不是新增依赖或锁漂移。

对新产物所有字节及各 section 进行固定机器路径模式的 UTF-8/UTF-16 检查，命中为零。该结论只覆盖列出的有限模式，不能称作通用秘密检测或所有隐私风险消除。独立读回还核对产物副本、原始捕获、当前源码和新 Cargo home 的来源及物理集合。

静态调用接口核对状态为 `CALL_INTERFACE_MATCH_LAYOUT_GLOBALS_DIFFER_REQUIRE_DYNAMIC_LAYOUT`。其中 `__data_end` 从 1,066,437 变为 1,066,093，`__heap_base` 从 1,066,448 变为 1,066,096；不能硬编码原数据区地址，也不能据函数签名相同宣称完整运行兼容。

## 七步会话资格：已审核，尚未执行

新的宿主集成测试固定绑定上述新 Wasm，保留原 127 包锁（7 个 path 包和 120 个 registry 包）。全部 120 份归档及对应非撤回索引行已从现有本地缓存核对版本和锁校验和，不需要下载或换锁。测试、控制器和选定输入完成源码审核；**本次未执行宿主 metadata、测试编译或测试程序**。

后继执行顺序为：严格 metadata → 输入及完整新缓存守卫 → 仅编译指定测试 → 守卫 → 单次精确测试 → 结果及最终守卫。测试使用独立的普通合成 SQLite 数据，执行 Wasmi 中的父会话创建、写入器打开、事件追加、检查点、父快照、子会话创建和子快照，核对输出、事件和继承摘要。七步指七次 R2 交换，另有读取输入与完成任务两次 ABI 调用。

测试请求的 1 亿 fuel 会由原包默认值约束为有效 2,000 万；内存预算为 16 MiB，R2 调用上限为 16。所选测试不注册进程 provider，不执行 Claim 或 native Start；普通编译工具、系统随机数和独立 SQLite 写入属于其必要效果。它不验收 Windows 沙箱、管理器所有权、真实断连或完整生命周期。

运行若进入 Unknown，须保留同一观察控制者，不超时重启或自动重放；已知非零结果不得继续后继阶段。上述控制器规则已审核，尚未产生真实七步结果，行为资格保持 `NOT_RUN`。

## 历史失败与未完成项

checkout 第一轮因错误要求六个未复制空目录而退出 1；第二轮仅修正这些固定目录期望。guest 第一次 metadata 因路径规范化权限拒绝退出 101、外层退出 1；第二次 metadata 退出 0，外层因错误要求全部 27 包出现在目标过滤图而退出 1，未编译 guest。以上失败及原始证据均保留，没有改写为通过。

阶段 18 的原根 build003 保持 **Cargo 退出 0、外层退出 1**，原缓存后置守卫未补记通过。阶段 18 的本地增量包已实际封包并完成全部成员 CRC、长度及 SHA256 独立读回；它依赖外部固定源码、工具及缓存，不是自足备份，不随本次文档公开，也不改变构建或 SDK 资格。

新 Wasm 尚未接入公开 harness，公开 Git 树完整构建状态继续为 `NOT_RUN_MISSING_SESSION_FIXTURE`。不能把本地载体构建、静态接口或准备好的测试入口当作公开 Git 树已完整构建。

原生 Start 保持 `SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY`。owner finish、factory release、cleanup/join、真实断连、Windows 生产沙箱和后续 11 项原始库测试仍待验收。四项额外 dev 依赖仍待授权，不能与本轮已齐备的 120 份会话宿主依赖混为一谈。`SDK26_G04=OPEN`，`release_eligible=false`，SDK 未冻结。

下一步实际执行审核后的单次七步测试，随后在明确授权范围内推进 Windows 生命周期与安全执行复验。会话层与安全执行层验收后暂停准备测试预览，不等待扩展执行层。

本次仅更新既有开发分支文档，不更新 main、tag 或 Release，不运行 CI。原始日志、路径表、缓存、二进制和本机专用入口不随文档公开。[阶段 20 准备记录](windows-agent-sdk-c28-stage20.md)与[阶段 18 编译记录](../reconstruction-2026-10-09/windows-agent-sdk-c28-stage18.md)保留各自历史时点。
