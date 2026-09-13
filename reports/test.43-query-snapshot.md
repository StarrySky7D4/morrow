# test.43：默认查询的一致快照与冻结原件授权

日期2026-09-13。应用 `0.1.9-test.43+48`，核心／宿主及打包清单test.43；审计包保持test.41（本轮链接新核心验证），运行时／SDK test.11、guest test.13；数据库格式11，无新迁移。第一方AGPL-3.0-only。本阶段无新增unsafe，仅本地开发，不推送、打标签或发布Release。

## 实现

Store捕获可信原生WAL来源与私有运行期身份，独立只读Connection持有BEGIN DEFERRED。源连接和读连接在固定事务内核对格式、最高永久事件及审计身份，期间发生提交导致读点不一致时明确拒绝。不是再开一个Windows Session，也不释放原.audit-lock。

全部Card类型按ID升序分批枚举，页预算只限制每页内存、不裁掉总候选。返回完整原Card PB、类型／格式／修订及最近内容Commit身份／序号／完整容器摘要，保留未知字段。全局读点可为内容、其他记录或读取事件，不混同于每卡修订。只有实际EOF才得到Census数量与固定编码有序摘要。

HostRuntime::read_snapshot_content先核对Store、snapshot及条目归属，再复用现有ReadContent授权前后检查，从冻结字节返回Card；没有回到最新Store补读。默认Workbench通过这个来源执行已有query_plan，跳过非idea类型但仍计入完整枚举，结束时显式close再返回owned结果；失败Drop释放读连接，原宿主仍持有资料库租约。

独立检查修复三处边界：首游标遗漏损坏空ID、最高事件只hash未分类解码、永久content操作指向缺失Card。损坏后snapshot标为不可用，已返回FrozenCard也不能继续用于授权读取。额外EXPLAIN检查发现兼容首行的OR条件令后续分页SCAN，最终改为首、后页两个固定SQL，后页恢复主键range SEARCH，仍全部参数绑定。

## 专项结果

核心独立18项通过：另一个Store正式并发写后的原集合／原字节稳定，精确8MiB含未知字段Card，分页字节边界及可重试预算、129条跨类型枚举、独立Census编码、头事件与每卡最新content区别、跨Store／snapshot／实例／对象拒绝且不消耗时钟、撤权／到期／第二次时钟、poison／显式close／源Store关闭后的只读句柄、非支持模式及完整性异常。

宿主实际Rust guest专项5项通过：旧snapshot后正式EditContent改标题／正文、新增和软删，旧结果[b,d,a]不变，新默认query为[c,b]；128个非idea整页不提前EOF且census129；foreign／非法条件／合法Card超64KiB查询帧失败后可继续查询及创建；实际guest trap后显式换回原业务包可恢复；空库也需要当前Read能力。各组验证封存及Windows Session租约保持、finish后释放并正常重开。

失败证据分别保留：`build/test43-card-snapshot-initial.log`为首轮15PASS／2FAIL原工具输出转存；两处修复后17PASS，加入缺失Card回归后最终18PASS。缺失Card项首次运行时修复已到位，不声称有先失败的运行证据。初期内存库、容量和包降级夹具错误已修正，未改变生产规则迎合测试。最终独立日志 `build/test43-card-snapshot-final.log`、`build/test43-card-snapshot-clippy.log`；宿主专项 `build/test43-query-source-host.log`。

## 全量与实际产物

本阶段为 **PASS_SCOPED**。分页索引修正后，核心、审计、宿主完整回归和严格检查已重跑通过；修正前通过日志以 `*-before-index-fix.log` 保留，不混作最终结果。

| 范围 | 最终实测结果 |
| --- | --- |
| 核心 Release＋fault-injection | 288项通过；独立清单288；全目标严格Clippy通过 |
| 审计 Release＋fault-injection | 60项通过；独立清单60；全目标严格Clippy通过 |
| 宿主 Release＋fault-injection | 93项通过；独立清单93；全目标严格Clippy通过 |
| Wasm核心 | 普通及web-storage两配置编译通过；不宣称新快照在浏览器可用 |
| 生产 Rust bundle | 宿主、实际guest及不可变包构建核验通过 |
| Flutter与实际生产宿主 | 39项回归通过，包含查询协调器、编辑器与原生插件路径 |
| Windows Release | 构建50.8秒通过；实际0.1.9-test.43+48，退出码0，四项自检通过 |

完整日志 `build/test43-{core,audit,host}-{full,clippy,list}.log`；Wasm为 `build/test43-core-wasm.log` 与 `build/test43-core-web-storage.log`；bundle、Flutter、Windows为 `build/test43-bundle.log`、`build/test43-final-flutter.log`、`build/test43-windows-build.log`。SQL定位诊断保留于 `build/test43-pagination-plan.log`（Python SQLite只读EXPLAIN；非吞吐量基准）。未重跑完整运行时测试，也未用Windows或Wasm编译替代其他平台运行验收。

实际程序 `build/windows/x64/runner/Release/morrow_studio.exe`，运行时保留整个Release目录。Windows内宿主及插件包与生产bundle的SHA-256一致。

实际合成库／截图：`build/workbench-host/test43-final-62572d2711ec4d15972ab828bc9ce9b0/` 下 `result.md`、`result.png`。已目视检查实际工作台、侧栏、外观和卡片正常显示，无停留加载或错误页。自检包含桌面合成API接受blur 0/1/12/40和关闭、静音WAV实际解码及播放时钟、定位／互斥／恢复不自动播放、Rust工作台渲染无Flutter错误；不是桌面背景像素对比。

元数据 `build/test43-final-verification.json`：库格式11，SQLite integrity为ok，6份内容Evidence和6个引用、34个唯一块／144个块引用，缺失块及孤立块均为0。这里仍是内容证据，不能称作默认查询自动持久化的读取证据。

| 产物 | SHA-256 |
| --- | --- |
| morrow_studio.exe | `f3971c2fcc73cc4ad4c945a498d9d941a9e5051770915514875d3c05cc4d3aa4` |
| morrow-workbench-host.exe | `d15007f268798cd6007a80654c8bb5bbc6cc6e1327fc4a5ccd454dacfce4157d` |
| workbench.morrowplugin | `68b1cb9b343275fb2b7e208e2d793c7689db29ed3e34a71b57eb24292bc217b2` |
| morrow-content-replay.exe | `1248b20de0a8edb7c9fe0cf592dd3c991a1bdae772cdf568361ef1b639466695` |
| morrow-audit-check.exe | `650b5a4c2e72f050bb64e08bb29950f48d3f59414bff0d7c2f8f456c8f8bbd32` |


## 验证边界与后续

这是默认Windows查询的一致来源接入，仍未将候选原件／全部实际调用／最终目录持久化为自动读取日志。Census和逻辑读点能核对内部来源关系，不单独证明宿主未遗漏源库，不替代完整历史审计签名核验或操作系统文件句柄身份。打开期间逻辑头匹配也不宣称防御恶意同身份文件替换。

现有对象grant在读取后撤销，未变成贯穿整个查询的权限上下文；查询最终存证与历史交付需单独复核当前政策。Web/OPFS及原生EXCLUSIVE/DELETE不支持本轮owned多连接快照，明确拒绝，不回退为最新逐页读取。普通Wasm编译不等于浏览器能力验收。

后续继续实现有界来源／调用分片与原子最终目录、稳定operation重试、结果修订和历史分块交付，再推进全查询授权、独立来源验证与Web适配。完整M3–M7及0.2.0目标保持未完成。
