# C16：Agent 原生 owner 可信 Rust 接线与限定 Windows 资格

2026-10-06，本地实验性候选；未提交、推送、Release 或 CI，`release_eligible=false`。新入口仍 experimental，不代表正式 SDK 冻结，也不宣称生产 GUI 可执行。接口说明：[PLUGIN_AGENT_NATIVE_OWNER](../../docs/PLUGIN_AGENT_NATIVE_OWNER.md)。

公开派生 validation（仅本机路径及带路径的命令文本改为逻辑标识）随报告保存：[codex-sdk-c16-validation.json](codex-sdk-c16-validation.json)；原始未脱敏记录的历史 SHA `638cd06075cbded33cc6c6d9549e42af8aa492ddfdbb34720cceb096e2636e62`；状态为 `PASS_CURRENT_OWNED_SCOPES_ONLY`，绑定当前 27 个 owned 编译路径。qualified path 按各自实际输入覆盖，不把早期组件快照提升为全 workspace 资格。报告组数来自该 validation，既不累计重复运行，也不将总测试数当完成率。

## 当前行为

可信 Rust Workbench start/submit/poll/cancel/recover/acknowledge 已接完整原 owner 移动；没有复制 Core/Store/保护 Session、另建实时授权或冒用旧 IoWorker carrier。AgentContext 沿用原 R2 host、原 executor Arc/Admission 和 same-owner BorrowedNativeResources。每次 Wasm import 维护原 owner 并复验原 Manager/catalog/Core、取消和时钟权限。静态审批只是上限，生产 native port 与原 owner 绑定且必须显式 provision，普通 qualification port 不能进入生产入口。

stop 请求取消 guest、原 lease/executor 和真实 native；原 owner 必须保到实际 exit/EOF、provider/jobs 退役、facts 历史确认与 OS worker join。CAS Conflict/Unknown 保留清理与计费、不自动重放；显式 recover 只对账/清理。回收同 Runtime 必须同步非 Tokio 的唯一 Arc，外部引用未退保持 pending；unused 上下文同样保 anchor。scheduler token/debt 16 上限可观察并显式 reap，不能用 clean 计数、超时或 stop 冒充 task join。

## 实际验证分组

| 根代理运行标签 | 范围 | 当前结果 |
| --- | --- | --- |
| `native-preflight-001` | 生产输入预检查（纯参数，不启动生产沙箱） | 10 PASS；exit 0 |
| `native-borrowed-003` | 借用原 owner / 实际普通 Windows 子进程 | 10 PASS；exit 0 |
| `worker-002` | 最终 worker / 每 import 维护 / scheduler 寿命 | 9 PASS；exit 0 |
| `coupled-worker-003` | 封存 Rust Wasm＋实际 Windows＋泛型原 owner worker | 3 PASS；exit 0 |
| `native-owned-001-parse-correction` | 既有原生封存 Wasm 链（原日志解析纠正） | 1 PASS；exit 0 |
| `gate-regression-002` | 原 ProductGate 实际 host 回归 | 4 PASS；exit 0 |
| `host-bridge-001` | 旧 host schema/catalog/managed/route 回归 | 1 / 18 / 14 / 13 PASS；exit 0 |
| `host-tls-001` | host TLS 回归 | 1 PASS；exit 0 |
| `network-tls-001` | network TLS 回归 | 5 PASS；exit 0 |
| `network-client-tls-001` | client TLS 回归 | 4 PASS；exit 0 |
| `network-authorized-tls-001` | authorized TLS 回归 | 1 PASS；exit 0 |
| `host-check-002` | Windows host lib/bin 编译检查 | check PASS；无测试方法；exit 0 |

host bridge 行的 1/18/14/13 分别对应 schema/catalog/managed/route；零方法 doc target 不计数。native helper filtered 不计数；worker-001 旧 8 与 native-borrowed-002 旧 10 是历史，不与最终结果累计。check 通过也不计为运行测试。

最终三项组合实际执行，均为普通无沙箱 Windows 和合成普通 SQLite owner，非 ProtectedSession/DPAPI：

1. 原封存 Rust proposal/Process Wasm、原 review/claim、借用真实子进程与 stdout/stderr/exit/EOF，generic AgentWorker 移动同原 owner 并在真实清理/join 后归还。
2. 真实写后失权不能交付，结果 Unknown；真实效果不重复，trusted cleanup 保留原 native facts。
3. 同原资源已 charged 而上下文无 port 时 pre-spawn 拒绝，零 guest import；原 owner 原样返回，由 trusted port 观察 exit/EOF/facts/finish 后清理。

## 原始收据与日志 pins

| 标签 | receipt SHA256 | 原 stdout SHA256 |
| --- | --- | --- |
| `native-preflight-001` | `65aa0f72c7711a2ba8129741634368eebe011565dbf40461d1963034ab68099b` | `2cf94fc5b9823c7888b870b97746fed15e19366c799d42ffe623b07219e5f4b9` |
| `native-borrowed-003` | `2a6244decb1b5da562871447eb161f756e695b366bf8437411cc54fc7f77ca97` | `432c1ab5b12379376f6f1a1246a9a50db3e9a582fa104d00ecdfe05f69f3bb07` |
| `worker-002` | `3aee91b463cf8a4b95c740bbe29e6a89619944f1796e0af9a38e209a220e1caf` | `8a45a706134bf6945ffc8f92969bc3bbde641a7c0bd28f93fb3b776d391457a9` |
| `coupled-worker-003` | `6bf6d9457193fe94ae76f14462d9d0f405cb0db17e80cd34ecc279652c02685c` | `92532bf516d528858ee2497a345bf6269e08cd33f6632be00c15bec75d1c9a6b` |
| `native-owned-001-parse-correction` | `679c0ba653d811f6c22ffef6e6ea0a7684e7fddb5f2d3c8a416034bfe842d32a` | `acbb5b2c2c5c14b618c5da1fe51fdc75901c2dcfbf90d55a7cda35efc09d89a8` |
| `gate-regression-002` | `4a438d83d98774e641becea14edc4b21e7ffc08518651af2504f7f6b98184bdf` | `b2ee391431b4a20b5bb355b0684f116040199c34c49b941d7e5a2804f1c1a640` |
| `host-bridge-001` | `4b336bd676b591e899a724021997933c40fe43323891ce0449b2edeae0802241` | `a23188a2b522d30be2643b99d56fa08786827256a048a062b04cde88285a3a15` |
| `host-tls-001` | `4ecbf3f9cea2f1c37391f52a010ede7990048a319e2ba1affc7d71398c9d3238` | `81e8e054da526431a13d65b1ad6471dfbc9ba0b3cd62b34b1a19a3c253ca2418` |
| `network-tls-001` | `9615ee6385f7b134effa449d26d25e5eb42b72ee3b104e650f0078769af9f066` | `601958d554cc6c2e8eba57cd3cd4fb5f4f17bbb2d85482711550339042946911` |
| `network-client-tls-001` | `622672d5a8eb2f2c67dbfdf4fc75d09d7cb6792359d98fbc79c916beb530c24e` | `50ce3cf1aa2611d7587d21f485759eb1fe7a3eb90b43d21f772b7989b493a039` |
| `network-authorized-tls-001` | `c73fe90cea45ab1a4e2d8b4d5ba7c352458ff4bf5ba15b2a11ab22c51235d014` | `6b23e8bcf2dda2ea57bd88dab27b49c7b750ab38dae6783700c2dd3b02427b74` |
| `host-check-002` | `63cfd1d506b9eb8bb0e2a15d33ab706f342fdda4371b7127009712a8a3a8e616` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

全部完整 argv、stderr SHA、源前后 pin、实际退出码、过滤方法及身份由原 validation 保留；不需要依赖私有本地目录文案。封存 guest 本轮未重建，不把 Wasm 编译当作执行。

## 保留的失败与修复

首 native 编译缺少 target-specific packages feature，exit 101、0 方法运行，原件保留。原 native-owned 程序真实 exit 0/1 PASS；nocapture 跨行导致最初元数据漏抽方法，另写 parse-correction，只重新解析原日志、不重跑、不改原收据或日志，不记成程序失败。

coupled-worker-001 编译 exit 101、0 方法运行；002 编译通过后实际 3 FAIL/exit 101，含独立 parse-correction 原件。独立审查定位 fixture 在 claim 后过早关闭原 proposer，原 R2 执行时仍要求 live Propose，因此原权限拒绝正确。修复只让原 proposer/package/连接随完整普通 owner 保留至事实收尾，再同 host 明确关闭，并保持重复 finish 的幂等性；没有放宽生产授权或重放 Unknown。最终 003 的 3 PASS 是新真实运行，不抹去前两次失败。

unused Runtime lifetime P1 在最终 worker 修复：原生 clean 可能早于 async capture 析构，两个 unused 入口现在同样检查唯一 Arc。新真实 Tokio task alias 回归与最终 worker 9 方法通过，静态初审与修复原像仍保留。

此前另有已授权的 cc 公共原件索引替换，独立收据 `cache-preparation/cc-index-patch-001/receipt.json` SHA `7386ce4c2aec797660cc8256adfebe71bf463d53afe26cc5438f293f4453aca7`；复核既有 cc 1.4.5/1.4.7 完整 JSON 与依赖相等，原 cc 归档未改、三份锁前后摘要一致，索引原像保留。后续旧锁补缺阶段仅补原锁 uuid 1.24.0、toml_edit 0.25.13 和 find-msvc-tools 0.1.14 公共归档及 find 完整原始索引；旧 14 条 JSON/依赖保留，目标原像有 pin。这两个授权阶段均未读凭据/配置或下载；不能将后续三项范围误写成全部缓存历史。首 network metadata 的锁漂移虽进程 exit 0，仍按原收据保存失败，依赖处置与运行资格分开。

## 守卫与剩余范围

最终输入守卫 SHA `c35f5c16f00f9e008322c52fd669903d1828416b80fe90e2a6401ab190e4d17f`：原 SDK327、冻结57、旧合同、Linux、原148增量基线和 C15 封存包均通过；原 R 和 HEAD 在准备交付前未变。候选源码 delta SHA `cb5d0f4deff1c945ee10c934264a53d62f458332f2861d903b5896997ecf248f`。文档仅添加本轮块，C15 与更早历史保持原字节。该守卫与限定回归不等于全 SDK 的新的正式冻结验收。

production sandbox、真实 ProtectedSession/DPAPI、desktop runtime 都 `NOT_RUN`；认证 helper/network policy、真实 GUI 产品流程、其他平台与 SDK26/G04 仍 OPEN。同步 Runtime Drop 不保证有界等待，未知 facts/job 或丢失 owner 的债务不能假装结束。当前资格只覆盖原 validation 的 owned scopes；不做发布、外部部署或账户/真实库操作。


## 公开发布路径说明

链接的 validation 是保留原测试结果的公开路径派生记录，不能称原字节或当前源码新资格；原始完整 SHA、派生 SHA 和替换边界见 [公开历史记录说明](PUBLIC_VALIDATION_PROVENANCE.md)。原始失败和 NOT_RUN 范围不变。
