# C08 原owner目录秘密factory

2026-10-05。当前源码已新增有限可信宿主入口；Windows原生普通合成Store／临时目录资格已限定通过；不宣称生产或完整SDK通过。C08源码新增仍为本地未commit／push，上一轮已发布文档检查点为 `772466177fe589cee53bc633e69f411c34610104`。当前项目范围见 [项目状态](../../docs/PROJECT_STATUS.md)，后续门槛见 [SDK顺序](sdk-next-gates.md)。[C07目录owner记录](directory-owner-sdk.md)及其原始失败、过滤和封存身份保持，历史115／42／100不自动成为C08结果。

## 实现与入口

Windows原生 `packages`下的 `IoWorker<O: ManagedHostOwner>::capture_directory_fresh(file: File, limits: CaptureLimits)`返回原 `DirectorySession`／`DirectoryCommandHandle`。它接收可信宿主已选择并打开的File，不接收秘密、entropy provider、guest clock或授权布尔。原 `capture_directory(file, limits, secret)`和 `DirectoryBroker::new(secret)`保持源码入口及legacy调用者责任；调用者仍需保证显式秘密新鲜性并管理自己的保留副本。

新请求沿原owner命令队列、prepare、Manager／HostRuntime／ManagedInstance／IoBinding精确身份、FileList准入、原clock、Control与Ticket运行，未创建第二个owner/runtime/manager。生产生成仅调用 `getrandom::fill`（锁定0.4.3）；它已在锁定依赖图中，本轮只补Windows runtime直接使用边及消费锁关系，没有引入新随机库版本或下载fallback。

原clock样本与对应FileList校验在同一短step完成，生成前、填充后派生前以及broker边界保留原取消、期限、身份与授权核验。系统随机调用、SHA派生、native查询、解析、编码及实际drop不持原clock／Control／Ticket锁。短step不能保证不当缓慢的原clock closure不阻塞共享时钟，合作取消也不能抢占正在运行的OS调用。

## 新鲜性、拒绝与秘密所有权

每次fresh请求生成32字节OS熵，并用固定版本域、非零原worker身份、checked不重用session serial和熵派生broker秘密。checked计数耗尽不回绕或退回；熵来源是CSPRNG，跨进程／重启及全局碰撞不存在性仍是概率性边界，不能由少量不同ID断言数学唯一性。

熵错误、部分填充或全零熵拒绝；全零派生值也拒绝。返回受限 `DirectoryCommandError::Entropy`，不输出底层OS错误、秘密或部分随机字节，不用时间、目录名、旧key、弱随机或自动重试替代。原queued取消与started后未知交付保持原错误层次；失去交付资格时保留Unknown，不把未发布selection解释为业务回滚证明。

熵缓冲、派生秘密和broker持有秘密由 `Zeroizing`管理，消费式移交保持所持秘密所有权。普通Drop／unwind清理所持缓冲；legacy caller副本、SHA实现内部状态、OS／编译器临时副本、abort与进程终止后的擦除不在此保证内。没有生产secret读取接口，诊断不格式化秘密。仅 `cfg(test)` 的每worker私有探针可选择受控合成填充、观察调用线程／次数与still-live buffer的wipe布尔；这不是生产可选provider，也不读取释放后的内存。

## 原预算与真实清理

fresh请求复用global8 live＋queued＋resident／retired tombstone额度、单selection一次pending／未读交付、effective capture ceiling和原累积账本。Request／Response固定reservation按当前类型大小计算，加上受原限额收紧的capture ceiling；这不是OS随机源或SHA全部临时内存的RSS测量。熵失败、取消、丢弃、native拒绝与清理不退还已admitted费用。

File／root／broker／lease／spool的真实拥有者drop先于resident slot返还。取消或丢弃未读结果不能触发重放；idle维护与Stop须沿原worker完成真实释放，poll、Ready、EOF或信号不代替实际join及原owner归还。私有门控只模拟生成边界的延迟，不是同步OS抢占或GUI响应验收。

## 本轮实际Windows验证

全部运行使用新本轮Windows x64 Release／locked／offline产物。各组按真实唯一方法名单分别报告；没有把历史重跑、子案例、命令、文件hash或exe数量当作新增方法。

| 当前候选范围 | 真实结果 | 计数规则 |
|---|---|---|
| fresh factory私有单元 | 14通过，0失败／忽略，69未选方法过滤 | 每worker受控探针与原owner线程、前后门禁、熵错／全零、计数耗尽、额度／drop、legacy兼容与redaction；子案例不另计 |
| 原owner／目录生命周期 | 九组115通过，0失败／忽略／过滤 | directory_cancel7、directory_owner14、directory_io16已经包含在115；其余owner_commands16、file_owner12、io_owner10、io_jobs17、io_jobs_managed12、io_jobs_brokered11 |
| 原始SDK定向parent方法 | 五组42通过，0失败／忽略 | frozen-base9、dependency3、frozen-region7、remote-reader9、shared-objects14；region保留14项过滤（含两child），reader保留1 child过滤，helpers不额外计方法 |
| 网络回归 | 十一组100通过，0失败／忽略／过滤 | lib28、client10、managed_sse14、managed_ws10、sse13、sse_envelope3、sse_sdk2、stream6、websocket9、ws_envelope3、ws_sdk2；真正消费原sealed合成guest字节，不重封旧包 |
| 限定fmt与strict库Rustc | 三个文件fmt通过，strict library Rustc exit0 | directory_commands.rs、directory_io.rs、secret_factory_tests.rs；不是整库fmt资格，原io_jobs旧格式差异保持 |
| 整库library Clippy | exit101，10处原有诊断，0处本轮owned诊断 | 仅library lint范围，不宣称私有单元测试或all-targets Clippy通过 |

本轮实际消费27个新native exe，保护106个code／test输入pin；这些身份数量不增加方法数。原始SDK327／冻结57、旧guest／provider／载荷包以及已封存C07记录保持；原件无重建／重打包。本轮资格记录 `qualification-code-002.json` SHA256为 `011b66e80ae13e87a71b2300824a71480f1d164799df55adebd257d88b8cbaf5`；质量分析 `execution-002/quality-001/record.json` SHA256为 `38f3f3675271ac5d5bdfd39540cc4acb1406d2769508e39217bbb251437471db`。这两个记录属于本地证据身份，不是公开下载、授权或产品签名。

最终execution-002的factory实际invocation已有driver／common-helper的执行前后pin，以及actual binary／source／build、原始stdout／stderr／exit和方法名单。旧execution-001首次invocation的driver／common-helper没有单独采集各自hash；该历史记录仍保留原单写者与driver来源限定，不以当前pin倒推旧执行身份。实际编译记录直接固定cargo.exe，环境选择Rust1.95、RUSTC／RUSTDOC及MSVC／Capnp路径；没有逐command固定全部transitive rustc／linker／Capnp工具。当前工具hash不能倒推执行瞬间身份。准备期reader失败及后续reader修补记录保留，不伪造测试重跑；C07旧失败、负向复现与其115／42／100资格继续按原日期阅读，不累计为本轮新credit。

最终候选仅把plugin_runtime/Cargo.toml的CRLF恢复为原LF，TOML解析值等价，未改变依赖或生产语义。该格式修复仍触发MSVC重新链接，Cargo exit0并不意味着旧27产物字节相同；bridge的byte-identity断言失败及旧execution-001完整保留。因此最终execution-002重新构建，并对最终产物实际执行factory14及115／42／100全部方法。上表只记最终唯一方法数，不累计两轮执行、构建recipe或重复调用，也不借旧exe结果证明当前候选。

## 保留边界与下一步

SDK327与冻结57继续保持原输入身份；未重建或重打包原Rust／C／C++guest、provider及历史载荷客体。C08没有新增目录Wasm、C／C++宿主入口或公共request/schema、required feature／import协商。原Core公开FileList和conditional Replace仍Unsupported，不以普通覆盖或先删后写替代条件操作。

bare File只有已打开对象身份，不证明picker、workspace政策或祖先路径来源。生产protected owner／真实session／StorageIoWorker／GUI、可信选择adapter与生产secret生命周期、公开目录协商、blob耐久backend/history、upload/watch/rename、账户／TLS／真实库／DPAPI及其他平台仍未由本轮普通keyless Store／临时目录资格覆盖。blob的VerifiedBytes不是Store提交。SDK26／G04保持OPEN；下一步按 [完整门槛](sdk-next-gates.md)分别推进，不能缩为本次factory或已通过子集。
