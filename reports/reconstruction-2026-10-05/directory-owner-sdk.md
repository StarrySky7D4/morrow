# C07：Windows 原 owner 的目录命令接线

2026-10-05。当前限定资格为Windows普通合成Store／新建临时目录、原Manager／HostRuntime／ManagedInstance／IoBinding与原IoWorker线程上的可信native命令。代码资格已实际封存；这不是生产protected owner或完整SDK冻结。

## 实际结果与计数边界

| 范围 | 最终实际结果 | 计数范围 |
|---|---|---|
| 原owner九组 | 115方法PASS，0失败／ignored／filtered | cancel7、directory owner14、native directory16、owner commands16、file owner12、io owner10、io jobs17、managed jobs12、brokered jobs11；7/14/16已经包含于115 |
| 原SDK回归 | 42方法PASS，5harness | 与C06修正名单逐名相同；frozen-region过滤14、remote-reader过滤1，child helpers另列不增加42 |
| 网络回归 | 100方法PASS，11target，0失败／ignored／filtered | 与C06逐名相同；24原guest/provider包只读，rebuild/repack为0 |
| 格式与编译检查 | 五个实际rustfmt文件PASS、strict library Rustc PASS | `directory_io.rs`、`io_jobs/directory_commands.rs`、`io_jobs/owner_commands.rs`、`tests/directory_cancel.rs`、`tests/directory_owner.rs`；原`io_jobs.rs`全文件17个旧格式hunks保持，不宣称其formatter clean |
| 整库Clippy | FAIL，实际exit101 | 10诊断来自保持bootstrap字节的旧channel／file_target native／mutation_commands源，owned诊断0；不声称整库lint通过 |
| 实际产物身份 | 26份实际使用immutable executable pins已核对 | 9 owner、6原件harness/helper、11network；这是产物数，不是方法数 |

代码资格`qualification-code-001.json`为82174 bytes，SHA256 `773e36fa49013d79071a6cb9f9de4500cb74d99c5aec8042b70e39290ccdea7f`，绑定105源码hash和candidate006的`b97d4295ae89a9848ddde529bf1a047bb595c783f1b9d6cec8c4d55430b83a89`。封存执行由原始逐名终态、argv／cwd／raw exit、stdout／stderr、实际使用exe身份和输入before／after决定。证据目录内的相对记录及SHA如下；本表不构成额外方法计数。

| 记录 | bytes | SHA256 |
|---|---:|---|
| `native-owner/execute-005/qualification-result.json` | 188135 | `8b818f8031872ff2e31f76b6b85b4037b707da6f439ab97f2fc26e5acc244f2a` |
| `originals-regression-001/qualification-result.json` | 236163 | `47838ad8d0f2aee167c402275db71d7c0d2c4e94f7adbc9246144a81636f7362` |
| `network-regression/qualification-final-001.json` | 202426 | `88d2fbd9edd706cb6a5a7fb540a758236f371d0c0473dfd92827168dadf729fb` |
| `owned-checks-002/qualification-result.json` | 42009 | `c629bd4355b587279eda951d06b72cb7a69bacf4b0d30cc2d16fa620a82f8291` |
| `native-owner/reproduce-003/reproduction-result.json` | 148833 | `74024eeed3c96a2b3b5a476566c5ad9e26a317c9feb924d4e23bf91b9a44b348` |

结果保持分层：C06目录库13、blob库23、138vectors、独立Rust7和19去重families仍是其历史证据，本轮没有重新给这些库／corpus增加测试信用。旧114、旧候选115重跑、负向exit101、26个原clock源码采样位置、编译命令、重复子案例和helper不相加。零方法和仅阅读／构建均不是执行PASS。C06历史详见 [目录/blob资格](directory-blob-sdk.md)。

## 原批准链、有效预算与生命周期

Windows `IoWorker<O: ManagedHostOwner>`增加可信native `capture_directory`、`next_directory_page`、`finish_directory`。输入是原已打开的selected目录File、收紧CaptureLimits和可信调用者的新鲜secret；guest不提供路径／handle／secret。原Manager、HostRuntime、ManagedInstance、IoBinding、FileList、package/digest/revision、lease／deadline、Stop／撤权／时钟与累计预算仍逐步验证；manager缺失保持默认拒绝，没有新Store/token bool或备用owner。

原Authority／Control的时间、授权和交付校验方法保持；Control新增有界目录Admission bookkeeping。capture有效job ceiling先扣固定命令费用，再取调用者、worker和原声明的最窄限制。队列计入capture allowance、page请求与builder／encoded-wire并存／owned metadata／root查询、finish固定费用；broker继续从原实例账本收取native query／buffer／name metadata／delivery费用。admitted错误／取消不退累计字节；释放root或tombstone只释放容量。`directory_usage().1`的metadata allowance是保守计费allowance，不是完整进程RSS或所有allocator峰值上限。

同一selection只允许一页pending／未读交付。worker、epoch、ref、连续序号和after-entry ID精确核验，foreign／invalid cursor不能擦他人资源。成功read先标记delivered，单次取reply后handle清理仍保留已交付观察；terminal／finish真实释放selection。开始后交付丢失或迟到为Unknown，不自动重放、不回绕cursor；signal、Ready、EOF、cancel或ACK不代表join。14个owner方法实际覆盖原owner shutdown／join路径、取消／未读交付／global8／idle维护与原资源账本，不扩展为生产StorageIoWorker或GUI证明。

## 原子时钟与保留的失败

每个原clock样本和相应审批／liveness／lease／admit／charge验证在同一短`DirectoryClock.with` step持锁完成，不解锁后重复使用旧样本。native identity（含内部双查询）、Buffer::next同步OS与私有解析/name分配、builder／encoding、Ticket-only取消谓词和实际root／spool／lease drop均在clock guard外。legacy FnMut API保留26个原采样位置和取消ordinal顺序。idle在同一步分类固定8个opaque refs的reclaimability与原binding liveness，返回后才真正释放；空目录resource map不额外采样。

旧ready005将clock锁覆盖整段directory dispatch。保留的负向`slow_original_directory_context_does_not_block_poll_read_or_admission`在原context getter注入停留：tick2等待超过2秒，子exit101、0合格／1失败／13过滤。它是合成getter seam，不是强制慢OS或GUI测量。原114 PASS不覆盖该风险；修复后同一方法在最终14／115中PASS，子案例不增加方法。不能抢占同步OS或handler，不当慢的可信clock closure与其它旧锁仍可阻塞共享时钟。

先前Busy入队失败曾提前删除Admission slot，而原worker仍持broker/root/lease。初始blocker与输入保留；resident错误／取消现为retired tombstone，继续占全局8个queued/resident名额及metadata allowance。`Resource::Drop`先真正drop broker/root/lease，再删除Admission；queued-only Request先drop File再释放slot，capture在metadata发布前构造native cleanup guard，覆盖失败／unwind。最终global8方法保留真实8lease与Busy/tombstone子案例：未读无关Ticket释放后，新capture仍Busy，直到原owner实际drop才释放额度；与同一方法计数分开。

初始quota审查、旧clock负向、工具／候选错配、网络准备／metadata失败，以及最终formatter／Clippy定位修正均保留原raw／exit与候选，不覆盖后再宣布首轮成功。未使用的重链接exe不冒充实际产物。root codequalification实际commandexit0和所有源／工具／SDK／index fences已闭合；文档与有界恢复包另行审查，不继承为源码资格。

## 完整目标继续开放

filesystem directory不同于`channel::Directory`。name／entry ID仅描述有限observed listing，不能拼path或reopen，FileRead不推出FileList、递归／rename／read/mutation grant。bare root File只证明selected object，不证明picker／祖先来源；listing不承诺atomic filesystem snapshot。此API要求可信caller提供每broker／session新鲜唯一secret；`DirectoryBroker::new`不生成或强制唯一性。

SDK26／G04仍OPEN。下一步保持整个SDK冻结目标：生产trusted-secret factory与picker/ancestor proofs、workspace task入口、新独立Dir request/schema与SDK请求及包required-feature/import/helperprofile协商、blob durable backend／history／upload／watch／rename与完整平台矩阵。原Core公开FileList、conditional Replace保持Unsupported；不得退化为overwrite。blob VerifiedBytes／ACK不是trusted Store durable commit；Unknown不自动重放。未来调用完整SDK的入口／批准／payload／durability／恢复证据必须按原全目标门槛补齐，不缩为当前guest补丁或PASS子集。

当前普通合成Store／临时目录／原managed-owner线程不是protected生产owner9、真实session／StorageIoWorker或GUI，这些保持NOT_RUN。其它平台／TLS／CI／release／push未执行。原SDK327／冻结57、旧Core/schema/native unsafe与Linux云17文件本轮保持；新目录计账不改变旧授权校验方法。whole-runtime lint仍OPEN。

接口与历史：[目录/blob接入](../../docs/PLUGIN_DIRECTORY_BLOB_SDK.md)、[后续门槛](sdk-next-gates.md)、[完整26项／10门槛](../reconstruction-2026-10-03/sdk-scope-freeze-gates.md)。
