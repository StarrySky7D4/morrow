# Stage 14：Linux owned VFS、监督宿主与 channel 绑定前置条件

日期：2026-10-02 UTC。应用版本保持 `0.1.9-test.58+62`，SDK 仍未冻结。

此阶段是可验证的 Linux 平台基础，尚未启用受保护 Linux 产品。
基线为 `cc3de30f762b8fd44e56769de89539058fcd9a9e`；版本、契约 schema、旧客体、SDK 不可变包与预算均未改变。

## 受保护 SQLite 的实际打开对象基础

新增 `GuardedSqliteConnection` / `GuardedSqliteTransaction`，使用公开 SQLite VFS / io_methods ABI，自有实际 fd。主文件及 journal 的实际 fd 元数据、锚定目录和 O_PATH inode pin 在安装 pMethods 与任何 SQLite IO 前比较；不猜测 unixFile 私有布局，不以 HAS_MOVED 或 /proc 路径代替实际对象证明。

本轮只支持单主库、整个连接生命周期的 Linux OFD 独占锁和 DELETE rollback journal。WAL/SHM、readonly main、mmap、ATTACH、多库、文件临时库及未知对象均拒绝，无默认 VFS 回退。SQL 控制不能绕过 typed transaction 的 BEGIN/COMMIT/ROLLBACK；公开 query_row 要求 SQLite 判定为 readonly。缓存命中读同样在前后验证；提交前再次验证，提交不确定或 rollback 失败使连接永久失效。

拒绝打开时直接关闭普通 SQLite inode fd 可能释放同进程的既有 POSIX 锁。因此 fd 在关闭前必须取得全文件 OFD 写锁，证明没有冲突锁；否则进入有界退休队列。上限64，在实际 open 前预留容量，安全时才重新回收；不支持 OFD 的平台失败关闭，不退回 POSIX 锁。

新验证包括实际 xOpen 的 A→B→A 替换、缓存读的 mode 改变、transaction callback 后提交前验证、typed SQL 控制拒绝、提交/rollback 错误后的继续调用拒绝、journal 替换后删除拒绝、readonly journal/短读补零、真正的子进程锁竞争，以及 exit86 后实际 hot-journal 回放。单元故障注入不等于真实断电/磁盘硬件故障。

四个 `Store::open_private*` / private binding 入口仍在文件创建和 SQL 前拒绝。通用 Store 不作为受保护回退。audit session、snapshot、backup、recovery、library 和所有 Store 业务操作仍须完成严格接入后才能启用 Linux owner。本 API 不声称隔离主动恶意的同 UID 进程或使 POSIX unlinkat 具备条件 inode 原子删除能力。

## Linux 监督宿主与 GTK

独立 `linux/native_supervisor` crate 验证预期 SHA256 的实际 ELF，复制到 MFD_EXEC memfd 并加 WRITE/GROW/SHRINK/SEAL/EXEC seals，再以 execveat 执行。父进程通过 CLONE_PIDFD 原子取得对象、用 pidfd 发送信号和 waitid(P_PIDFD) 核对实际回收；不从任意数值 PID 重开句柄，不以 PID 消失代替停止证明。子进程环境为空、stdin 关闭、其余继承 fd 在 exec 时关闭，PDEATHSIG 与父进程检查防止常见分离窗口。PDEATHSIG 绑定创建子进程的线程；未来生产 supervisor 必须保持该线程直到实际回收，尚未获得跨线程生命周期接管资格。

输出有界保存并持续排空。进程退出、stdout/stderr EOF 与清理完成分别记录；后代保留 pipe 时 direct child exit 仍是 pending。Drop 只是尽力清理，不能承诺进程树全部停止。此处只 pin 主 ELF；系统动态加载器/共享库未被该 SHA pin 覆盖，生产宿主的编译期 artifact pin、可信 Wasmi 执行/隔离及完整 owner 协议尚未集成。

新 Linux runner 是 GTK 前置条件探针。普通启动、profile 参数和夹带参数明确拒绝，只有显式诊断参数允许打开纯 GTK 说明窗口，不启动 Flutter、不进入旧 Dart storage。Flutter product CMake 路线也明确拒绝。已验证编译/链接与无显示器的拒绝/status 路线；未运行 GTK 窗口或完整 UI 生命周期测试。

系统根目录只读，用户批准在工作目录内准备官方工具链。Debian trixie InRelease 经现有官方 keyring 验签，Packages 索引按已签 Release SHA256 核验，385个包逐一校验与提取。实际合计1,229,845,092字节，低于批准的2GB下载及展开总量上限；无 maintainer scripts、系统目录或全局配置改动。实际编译使用 Clang19.1.7、CMake3.31.6、Ninja1.12.1、GTK3.24.49。工具链只是构建基础，不能解除 SecretService/passwd-home/真实保护身份的既有环境限制。

## 正式 channel 路由前置条件

Workbench channel 路由现可在 native source 中编译，并使用 typed OwnerBinding；Linux production variant 不可构造，synthetic variant 只存在于 test 构建。Linux 正式 open/bind 仍拒绝，discovery 的 public/workbench binding authority 仍为 false。Windows 保留既有受监督原始 owner 入口；没有本轮 Windows 产品验收。

明确合成 HostRuntime/Store fixture 通过真实私有 wire 调用 prepare、append、run、status、Close。原003 reusable 与 matched one-shot 不可变包分别运行 bytes/events，四次都达到实际 Wasmi Ok(0)、11calls、163840bytes、ACK5及两个实际 join；原持久化 ACK/cursor/hash 再开库核对。

随后原 storage maintenance 在 Linux 仍不支持，因此 wire 业务输出被抑制，状态保留 Unknown/repair，不把 guest execution 成功冒充产品任务成功，也不重放。另一个原始 owner catalogue-stop 控制是执行前撤销：实际 Err(PackageBinding)、0calls、ACK0，不声称在 suspended import 内撤销的资格。

## 复查与最终验证

最终代码检查点为 `652623fb883de5807082793fd5847e49d127c6aa`，代码树为 `dc54415f1f36698d39412c65ab48928f66d17583`。下列结果来自此代码的32条冻结源码命令；1977个相关仓库输入在执行前后及命令之间均无漂移。源码清单 SHA256：`a89741f9ded3c03ae290a0754936ffa7250453956d87172f741591b594ae2de1`。本报告随后追加，不改变已测代码。

固定 Rust1.96.0、Cap'n Proto1.5.0；Rust 使用 locked/offline、DEV/TEST_DEBUG=0。Flutter3.44.0/Dart3.12.0使用已锁定依赖和 no-pub；CI=true 仅短路 Flutter bot 检测，不运行 CI/Actions。Core authority 测试使用独立合成 XDG_STATE_HOME；生产身份没有改用环境变量。

| 检查 | 最终观察 | 限定范围 |
| --- | --- | --- |
| 独立 target 的 SDK | 91通过；Wasm guest/C两次检查通过 | 不改变 SDK/schema/pins |
| runtime controls/compat/faults | 40通过 | 原 Control/终止原因/实际 join |
| Host native lib | 80通过、2忽略 | 下行单独显式执行被忽略项 |
| Host实际SDK wire | 2测试通过；4次实际Wasm执行及1个执行前撤销 | 消费成功与执行前 catalogue stop 分开 |
| owned VFS integration | 14通过 | 包含2个 subprocess probe入口，不另行叠加子进程次数 |
| Core合成lib | 48通过、1既有忽略 | 含6个新VFS ABI/故障控制单元测试 |
| 原Linux metadata/private入口 | 7通过 | 不代表 private Store 启用 |
| 独立监督基础 | 11通过 | 实际 sealed ELF/pidfd/stdio/EOF |
| Linux preflight | 7通过 | 输出/时间限制与拒绝控制 |
| GTK | 新鲜配置、编译、链接及参数检查通过 | 普通/夹带参数/Flutter product路线如预期拒绝；未跑窗口 |
| Audit默认套件 | 35通过 | 不重复计入子进程 helper；真实 SecretService 正向未跑 |
| Python既有工具 | 56通过 | SDK声明/模板/基线/pins |
| Dart调用/catalog/UI | 97通过；9文件analyzer无问题 | fake-backend单元范围 |
| real Wasmi003严格比较 | 3通过、4次实际运行 | 两transport×bytes/events |
| Core evidence/receipt基线 | 4通过/6失败；3通过/4失败 | 同原10个失败名称，未修复/删除 |
| Wasmi001/002 stress | 6通过/2失败 | 同原fuel边界，未扩大预算 |

32条命令均达到各自明确预期；这包含既有失败和故意拒绝的非零退出，不等于32项全绿或完整产品合格。额外的真实仓库 baseline verifier 核对36个 pinned文件及13对原Wasm/package（未重建/重包）；schema sync检查通过。生产 host wasm32 --lib 在现有已核验Clang19.1.7的进程局部环境下编译通过，不称Web功能验收。

早期记录完整保留：默认capnp路径1.3.0的先行channel运行未冒充固定1.5.0；缺少Clang的wasm检查保留后用已批准工具链修正；第一次整组脚本的C++测试缺-Ilinux/runner属于命令错误；共享SDK target发生同版本capnp crate identity冲突，隔离target后同一源码通过。未因此修改SDK或隐去失败。

保留 test58 的10项 Core 基线失败与2项 Wasmi stress 限额失败；不扩大20M fuel/16MiB/16call预算，不删失败，不称完整 Core/SDK/Product 全绿。Clippy 在固定 Rust1.96.0 工具链中没有安装，本轮未执行或另行安装；编译、格式与实际测试结果单独列明。

独立循环复查覆盖 VFS、监督/GTK 与 channel gate，冻结源码重跑和修复后复查没有遗留的本范围P1/P2。复查发现的关闭 stdio fd 冲突、EINTR 无界重试、SQL transaction 控制绕过、commit/rollback 不确定后继续使用及 preflight 输出无字节上限等问题均已修正并回归，不能以测试夹具绕开。

## 尚未获得资格

受保护 Store 全调用点、真实同 UID SecretService/owner churn与 passwd-home identity、生产 owner/监督传输、进程树隔离、GTK/Flutter产品界面、Linux产品包、网络 SSE/WS、Cloud change source、异步依赖和 SDK 冻结均未完成。provider/Codex 工作未展开。

本阶段只更新开发分支；不修改 main、tag、Release，不运行 Actions/CI。最终 GitHub commit/tree、源码与证据备份 SHA 以及 Drive/Library 下载复核记录由外部交付回执绑定；不在本报告中提前声称尚未完成的发布。
