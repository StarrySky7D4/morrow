# C11 Windows 可移植复验与三语言目录 SDK

2026-10-06。本阶段基于云端开发提交 `e83cdf1d3506b001d07ffceaaa9f98b688bcc150`（树 `abcdee6e582ce18954118430e514f1d0805ce095`）在隔离源码和测试目录执行；限定资格为 Windows x64、普通合成 Store 和临时目录。新修正仍为未提交增量，原 R1/R2 封存与 SDK327/冻结57不改。本轮不推 main、tag 或 Release，不运行 CI。机器可读身份与原始日志摘要见 [验证记录](windows-sdk-c11-validation.json)。

## 变化与边界

历史记录的路径改用 `PurePosixPath`／`PureWindowsPath` 按产生平台解析；合法消费者相对依赖只做词法解析，混合锚点、UNC、非法遍历和输出逃逸拒绝。要求当前 executable 核验时，外国平台路径在本机磁盘访问前拒绝。普通文件、链接、归档成员、大小、原始 pin 和配置边界保留。验证原 Linux 归档不等于在 Windows 重跑其中 Linux 程序。

R2 的 28 个可移植安全执行案例改用 TempDir 中固定合成 artifact 的实际绝对路径和实读 SHA；Unix 专用案例继续保留真实 `/usr/bin/printf`。生产代码、协议和批准／Claim／Unknown／回执语义不改。这些 Windows 方法不证明真实 Windows 外部进程执行器已实现。

新增独立 `guests/ffi-support` workspace 与自身新锁，通过单一 Rust static archive 合并原目录 codec 和原 SDK `wasm-c` 分配导出；C／C++ 复用原分配桥和 C++ constructor adapter。原三语言样例、SDK 源码、既有锁与冻结插件/provider不重建或替换。新构建入口严格检查固定三个 import、签名、真实内存与函数类型索引、重复段、无 start/WASI；这是构建与静态 ABI 资格，真实执行由原 managed owner 另行证明。

`directory_sdk_guests` 显式启用 `directory-guest-qualification`，分别钉扎 Rust／C／C++ 实际新 Wasm。每种语言四案：70 个合成项的三页完成与最后原响应一致；FileList 批准不能由声明或 FileRead 代替；跨 worker selection 拒绝且两方各自原选择仍能执行；Ready 后取消为 Unknown、不重放、累计费用不退。读取交付与原任务租约释放分别检查，最终回收核验原 owner/HostBinding、同一 worker 的 prepare／最后一次 finish 和实际 join。独立观察器核对终页六条及其字段，不声称独立汇总全部 70 项；执行中取消仍依赖此前 WAT/owner 范围，不由这十二案证明。

## 本轮结果

| 范围 | 结果 | 计数限制 |
|---|---|---|
| R2 Windows | 106 PASS：authority6、codec48、logical safe-exec28、session24 | Unix 实进程本轮 NOT_RUN，不能继承 Linux 资格 |
| 真实三语言目录 guest | 12 PASS，每语言4案 | 早期 Rust4与该组重叠，不累计 |
| 原件与目录/任务回归 | 70 PASS | base9、dependency3、shared14、reader9、owner9、profile9、bounds11、task bounds2、task failures4；reader child1过滤 |
| Windows 只读映射 | 7 PASS、84过滤 | 实际拒绝写入探针与接收句柄资格；两个 child entry不另计，首轮 raw9保留 |
| 四组 Python 门禁 | 102运行：99 PASS、3 FIFO SKIP | Windows 不代替 POSIX FIFO/零跳过资格 |
| 新构建工具负向组 | 15 PASS | 纯字节格式检查；不与真实插件执行混算 |
| 原 Linux R2 归档 | PASS：573 source、156 Rust记录、32 Python记录、19 command记录 | 没有重执行／当前工具实文件核验；旧 pin 不给新候选冻结身份 |
| 原 R1 归档、原件/transport pin、合同同步 | PASS | 原字节及合同不更新 |

70+7为77个实质回归方法。不得将这些数字与 C07–C10历史、Linux记录、编译命令、子进程入口或真实客体执行子案例相加作为完成度。普通 `packages` 的选定回归不要求新外部 artifact；完整 runtime、Workbench/Flutter 构建、Clippy、真实用户库与设备本轮 NOT_RUN。

## 新产物身份

| 语言 | 字节 | SHA-256 |
|---|---:|---|
| rust | 175621 | `e17fc41bf5e456410735bf0fdfeef6542525d3c2831f0d65220c1035363e09df` |
| c | 120188 | `42444642fefd3adf17bb3b32877f977388cf1daacedf7fda9a30be57d75adbbc` |
| cpp | 121122 | `5b99c83fbecadce91a814f236529faeb86113f41b8d3306c3ed8d13d24c2e860` |

构建修正后 C/C++ 新目录复建结果与已实际执行的两个产物 SHA 相同；源码、工具、sysroot 原输入前后均一致。具体构建记录和原始输出保存在本阶段有界交接包；公开摘要不包含私有云端标识或凭据。

## 保留失败与后续

首次三语言组3 PASS/9 FAIL是测试把每命令的 prepare 错写为整个 owner恰好一次；改为允许所有原命令准备并要求最后一次 finish后完整12重跑。新构建工具首次10 PASS/4 FAIL/1 ERROR暴露缺内存/索引/重复段防错，修补后15重跑并复建两个新样例。上轮 Windows archive和安全执行夹具失败亦保留，成功记录不覆盖原日志。

SDK26／G04仍 OPEN。下一门槛是产品批准链、ProtectedSession、系统 picker及anchor以上来源、第三方安装／分发、blob耐久后端／历史／上传／watch／rename、异步组合与恢复，以及逐平台资格。旧 Core IO FileList与conditional Replace仍Unsupported；新目录profile只消费原批准活selection，不产生路径或新的文件权限。R2真实 Windows执行器、扩展执行、认证transport及Codex产品接线继续独立开放。
