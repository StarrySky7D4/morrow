# C28 本地阶段 18：生产根接入与编译记录

本阶段把已验证的启动防重放修正接入新的生产源码载体，并完成原根包 Windows 库与程序的实际编译。**Cargo 退出 0，但外层验证退出 1，尚未通过完整构建资格。** 新程序没有运行，本阶段没有执行测试或真实沙箱流程。本文用于源码变更和进度审阅，不构成 SDK 验收或发布资格。

## 实际源码接入

新的载体保留原有 1651 个文件，只替换原生产根 `companions/morrow-codex/qualification/c28-basic-harness/src/` 下两份文件：

| 文件 | 改动与来源 |
| --- | --- |
| `formal_cleanup.rs` | 接入阶段 16 的最小启动尝试门：调用前锁存，错误或未决结果禁止再次启动；成功返回任务 key 后保留原身份约束，兼容合法的 session join/ACK 后进入 native lane |
| `formal_cleanup_tests.rs` | 接入对应回归源码，覆盖错误后无任务、状态查询失败、已知任务身份及合法连续两 lane 等情况 |

原始前像、两文件差分和完整源表均保留。物化及独立字节审核确认：全部 20 份锁文件和 60 份 Cargo manifest 字节不变，其余 1649 个文件字节不变；新载体仍为 1651 文件、282 目录，合计 23,615,148 字节。未改 Linux process/controller，也未改原载体或原依赖缓存。

旧生产 metadata/实际图只作为原字节与依赖声明的依据；没有将其称作新载体的 fresh metadata。阶段 16 的 20 项独立合成回归保留原有通过范围，**没有据此宣称新生产载体已重新运行测试**。

与 GitHub 开发分支此前提交相比，本次还同步三处已有增量：`formal.rs` 的三处 include 改为仓库内固定夹具的相对路径；`sealed.rs` 提取原收据解析并加入六项边界回归；会话测试增加三项保留规则回归。它们已属于本次生产载体，不能与上述“相对原载体只换两份文件”混为一谈。六项解析回归已有实际通过记录，三项保留回归包含在此前通过的 27 项会话测试中，不重复累计。新发布副本没有重跑这些测试；本次本地原根编译包含固定夹具接线及原始合成夹具。

公开载荷复核发现，原 session Wasm 的 data section 内含本机源码路径，不能仅删除 custom section 消除。本次不公开该二进制，不修改或重建原始资格字节；在固定夹具目录补充构建前提说明。因此**公开 Git 树仍缺 session 夹具输入，不是可独立完整构建的快照**。它需要从审核后的合成 guest 源码另行构建并复验；本地完整载体的编译结果不能冒充公开 Git 树的编译通过。此前已在分支中的其他夹具保持历史状态，本次没有重新验收。

## 实际构建结果

使用固定 Rust 1.95、Windows MSVC 环境、现有 sealed registry 配置与新独立输出，执行原根包 `--locked --offline --lib --bin` 构建。没有运行产物，没有下载新依赖，也没有执行下游测试。

| 尝试 | 实际结果 | 保留的边界 |
| --- | --- | --- |
| build001 | 外层退出 1，未启动 Cargo | 原 Git 缓存名称预检遗漏标准 tag 文件；未创建构建运行目录或产物 |
| build002 | Cargo 退出 101，外层退出 1 | 深层 checkout 路径被拒绝；新输出中的 seed 后置守卫也失败。原始输入字节前后相等，不能称完整资格通过 |
| build003 | **Cargo 退出 0，外层退出 1** | stdout/stderr 均真实 EOF，已保存原根 lib/bin；最终新 seed 守卫因 `mxc` checkout 的 `.git/objects/info/alternates` 缺失而失败，后续完整物理集合检查未完成 |

build003 保存的原始输入前后表完全相等。该事实覆盖记录中选定的源码、锁、配置、工具和既有依赖字节；它不抵消新 seed 后置守卫失败，也不证明该缺失的根因，不能将 Cargo 0 改写为整个验证通过。

独立读回已完成：完整的 49,813 项记录守卫前后相等，当前 8,917 项非 vendor 源码、工具及原 Git 输入哈希相符；没有第三次重读原 vendor 有效载荷。三个新 checkout 各有一份 config 改变和一份 alternates 缺失，另有 2,922 个新名称，均位于新 checkout 的 `.git` 内。这些变化与本地检出刷新相符，但具体转换来源尚未完整核验，原守卫没有被放宽或补记为通过。原构建的后置物理守卫仍为未完成。

真实 compiler events 记录原根的一个非测试库产物和一个非测试程序产物。已独立保存：

| 产物 | 字节数 | SHA256 |
| --- | ---: | --- |
| `root-bin.exe` | 95,087,616 | `043656683ca0fa5e3cbd9c1ab25e40603032ac62162805bfa027aaa2cb6b629e` |
| `root-lib.rlib` | 9,929,112 | `497693d7d2e77f755516e181eb8cda1a14fb0b128821b742108279fa261735d6` |

独立产物读回另行核对了完整 compiler JSON 日志中的两个原根事件、非测试 profile、源码路径、成功结束事件，以及输出原件与保存副本的 SHA256、AMD64 PE 和 rlib 文件头。该读回没有执行产物，也没有消除缓存守卫失败。这两份字节不是已批准的 live 安装包。构建使用 `CARGO_PROFILE_DEV_DEBUG=0` 和 `CARGO_INCREMENTAL=0`，不宣称与旧 debug 产物字节相同。

## 可核对的证据身份

以下为相对证据名称及内容哈希；本公开记录不包含机器路径、私人操作记录或运行环境内容。原始证据保留在本地，不因列出哈希而自动成为公开发布载荷。

| 证据 | 字节数 | SHA256 |
| --- | ---: | --- |
| `formal_cleanup.rs` | 9,275 | `ba05b3a25be0df31af103bf63c691d2e0df2d13e9e474d240f728547181fe295` |
| `formal_cleanup_tests.rs` | 16,837 | `d643302d854003cae6ce072d8194b3811ecbeaa2b2c8d27df6db38a096ed5e33` |
| `c28-stage18-production-integration-prep001/source-manifest.json` | 315,129 | `4ece4ffd1375a2f0fb257bfaff37115bc32d657f875f9d1ea0ee3127c07ed899` |
| `c28-stage18-integration-source-peer001/review.json` | 3,428 | `c48728b946ec17b11e03d2a95cfc23f4ac09583b065be337b4aa48d30d830fe6` |
| `c28-stage18-production-build-prep003/runs/build001/receipt.json` | 3,226 | `cf5444a2f1665b8a98066468f10c901ce3ffa7b176a4d4d39bf7ad286d57599d` |
| `c28-stage18-production-build-prep003/runs/build001/compiler-artifacts.json` | 4,494 | `8389721fd169d24e191c9695c0b2952e1f3b8fc29983df92f0b2152873b413d0` |
| `c28-stage18-production-build-prep003/runs/build001/root-actual-exit.json` | 1,553 | `f851452c13e888b4a18397d924477617ca98246758d8146127cbac3ff6d6e6f4` |
| `c28-stage18-build003-actual-peer001/review.json` | 321,360 | `2f6823d2e6282696fddc872aadb0641a70a488bd05109622654e0168a396b65c` |
| `c28-stage18-build003-actual-peer001/artifact-readback.json` | 9,579 | `22a7549ccdb59ebecbee438ee349c2332a3930e4f02ece7a14117a64f433d418` |

## 尚未完成的接入与资格

Stage19 已准备 disabled adapter003 SOURCE：固定捕获库来源、一次安装保留规则、原业务命令语义与待绑定项，并完成限定静态审核。四个修改入口仍保留首行无条件拒绝。新生产产物与实际环境及 once 权限尚未形成完整、获准的 live 包；新 root 或新操作标识不授予执行权，也不允许重放旧 Unknown。

旧 runner/setup 的文件哈希、PE 和各自原编译事件已重新核对；但对应库与 PTY 模块有 12 处实际源码哈希变化，不能据两份 main 入口字节相同认定它们匹配新生产根。后继需明确并验证 helper 来源，三程序 manifest 继续 PENDING；旧编译器/链接器哈希等历史缺证也没有用当前值替代。

本次同步同时保存阶段 17 的自主读取库及合成测试源码：七项读取用例和两轮真实 Poll/Receive 桥接保留各自已通过范围。捕获库不自行 Start、kill 或重连；真实 EOF、主机保存 ACK 与业务 join 分开处理。它是内部资格工具，不是公开插件 ABI。阶段 19 的控制器源码保留无条件禁用入口，未接入产品运行路径。

本阶段未运行新的 lower11 测试；四项额外 dev 依赖仍为 PENDING，没有补齐或运行其下游测试闭包。真实 native Start、owner finish、factory release、join、原清理与断连恢复仍需各自实际证据。没有通过新增 opcode 绕过原 Finished 条件。

Stage18 增量归档目前仅有候选清单，尚未封包或宣称自足恢复；它排除完整源码、缓存、工具链和产物载荷，恢复仍需外部固定前提。是否归档及公开哪些材料须在实际构建审核后确定。

`SDK26_G04=OPEN`，`release_eligible=false`。本文不表示 main 合并、tag、Release 或 CI 已通过；开发分支源码与文档的发布状态另按实际 Git 结果记录。
