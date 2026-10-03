# Windows SDK 接管、修补与限定资格 — 2026-10-03

本轮已把远端开发线迁入本项目的独立 Windows 工作树，并完成冻结插件原件与 Windows 共享描述符的限定验证。SDK 仍为候选／实验状态，不能宣称全系统冻结。

## 来源与保留

开发分支为 `codex/windows-sdk-qualification-20261003`，基线 `63f38d4a8a5bf453248dc7532197bee6980dc86f`、tree `a8a74bc4563fae735e9b695f50db4e979ac1393d`。远端来源为 `codex/linux-sdk-reconstruction`。本地旧 stage15 `8433f0853c4b2c179f50f9b69522ed3a5484acdd` 与远端只差两份阶段报告，生产源码相同；两边历史并非简单快进。本轮未恢复已丢失的 stage16 增量。

迁移候选 51,306 项、764,318,730 字节在原候选验证前后逐字节一致。原主工作区、旧 Windows 工作树及 stage15 源码保留。沙箱曾显示旧树 357 项删除，管理员只读视图确认文件存在，属于权限可见性差异；未按该显示执行恢复或覆盖。六个 Git symlink 条目保存为原始链接目标文本，没有据此声明原生链接语义通过。

## 本地源码修补

生产修改仅 `core/src/plugin_package/registry/native.rs` 的 Windows 路径：先读取最多 MAX_CONTAINER+1 字节并拒绝超限的旧状态；同一个已写入并同步的临时文件，仅对 OS5／32／33有界重试原子发布，最多4次尝试、3次各10ms请求等待。每次失败及等待后下一次重试前都核验目标类型与旧字节；内容变化、读取失败或无法确认时返回 CommitUnknown，由既有注册表逻辑锁定后续决策。失败后不改权限或只读属性；没有重新执行插件业务命令。正常路径增加一次有界旧文件读取，30ms只约束请求等待之和，不保证整个IO调用的墙钟耗时。

两份依赖测试只将 Fixture 的临时目录字段移到最后，保证 Manager、SQLite等资源先释放，并各新增一个 Windows 删除目录回归。未改 Schema、原始插件、provider、公共接口、依赖版本或 Linux process/controller/recovery。

## 实际执行结果

| 阶段 | 结果 | 证据边界 |
| --- | --- | --- |
| 原候选 frozen base／dependency | 9＋3通过 | Rust／C／C++旧 Wasm、完整包和原 provider 保持原件；没有重编译或重封 |
| 原候选描述符 codec | 10＋9通过 | 独立 core workspace；最终阶段未重复 |
| Python SDK检查 | 30通过 | 原候选运行；最终阶段未重复 |
| Fixture 删除目录回归 | 修前各1失败；修后完整 graph15、dynamic12通过 | 实际文件锁与目录删除；修后临时文件零残留 |
| Windows publication 模块 | 12通过，41过滤 | 含同一临时文件、允许错误、耗尽、Unknown、等待期间变化、只读和预算边界 |
| 实际 publication 循环 | 单次1,000成功／1,000尝试、0重试 | 此轮未复现OS5，不能称实际重试在该轮治愈原先故障 |
| 最终修补宿主 | 9＋3、graph15、dynamic12、mapping7、objects14、reader9通过 | 69个Rust场景、7命令均exit0；子进程辅助入口不另计通过 |
| 冻结原件完整性 | 前后36条目／38实际文件一致 | 13对模块与完整包；原pin与provider保持不变 |

最终宿主绑定上述3文件精确增量及1,148项输入哈希，不能套用“纯63f原始源码”的身份。所有阶段日志、退出码、工具身份、修前与修后源码／原件哈希分别保存。

## 保留的失败与限制

首次 runtime/core 共用 target 导致 prost trait 身份编译失败；分离 workspace target 后相关实际测试通过。后续 graph13/14、dynamic10/11曾在初始化set_enabled返回Core(Io)；精确复验的通过没有抹掉完整suite失败。独立低层文件复现器在第36次publish返回OS5，旧目标字节未变；无法判断失败位于SetFileAttributesW或MoveFileExW，也没有证明外部过滤器是原因。新修补的最终完整suite通过，原始失败及首次模块测试私有trait导入编译失败全部保留。

普通受限token的Python／Dart合成目录链接预检通过。隔离Flutter准备命令虽exit0并创建15个插件链接，但工具在处理Pub离线参数前获取了缺失预缓存资源；该检查标为FAIL_OFFLINE_BOUNDARY，且执行token不同。没有把它算作纯离线或普通token的完整pubget通过。未更改系统、Developer Mode、安全或网络设置。GUI编译、应用启动、真实内容库、DPAPI、账户密钥、其他OS／架构、CI、tag与Release未执行。

清理仅覆盖旧 Cargo target 的36个已核验缓存叶目录：逻辑15.54GiB、清理时磁盘可用空间增加12.78GiB。源码、工作树、冻结原件、日志、用户数据及现有直接发布二进制保留。

## 后续编码顺序

1. W-02：Windows独立 transport-v1-rc1 原始6对插件及服务／HTTP真实宿主路径；补撤权、取消、过期、Unknown与重开，保持原件只读。
2. W-03：当前Rust／C／C++独立消费者、模板、分发与能力矩阵复验，区分guest二进制兼容、源码API和本地ABI；按manifest隔离target。
3. 与Linux开发协调共享描述符的非Windows映射后端及平台能力拒绝；Linux测试不替代Windows。补macOS／Linux／Android／iOS／Web各自证据。
4. 公共Workbench channel binding、异步依赖与组合能力预算、完整文件系统／网络流式API、跨重启Unknown核对分别闭环，之后再作分层SDK冻结决策。
5. GUI准备先核对完整本地artifact可用性与执行token，再在禁止隐式下载的独立路径验收；通过后开展Windows GUI验收。CCswitch／Codex接入在上述SDK与平台证据之后推进。

本轮交付为本地源码增量与有界可恢复日志包。没有提交、推送、运行GitHub Actions或发布版本。
