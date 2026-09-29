# M03 下一切片：真实撤权交付与系统背压

当前状态：本切片限定完成。fixture002 的 Core14/native31项本地回归及 Windows 编译通过；A011真实Core撤权抑制、B011真实系统写背压各单次运行通过，联合分别完成原始记录与原生持久账本的独立只读复核（未重复运行HTTP）。A010原失败不翻判，历史过程保留下文。完整M03、OS部分写故障、多平台和产品门槛仍未完成，SDK未冻结；本轮无Git提交、推送或Release。前片限定结果见 `m03-stream-001-plan-2026-09-29.md`。

## 两个独立目标

1. 实际 ModelClient/Core 已产生且已排队或预留交付容量的事件，在用户消费前遇到宿主实际撤权。屏障不持有状态锁或取消门。记录同一 operation/attempt/事件序号与内容摘要、宿主持久撤权、客户端接收及交付门关闭；再放行消费者，确认该事件被抑制，而此前已交付事件仍保留。
2. 实际数据管道停读导致同一个系统 WriteFile 持续未完成，独立控制通道仍能处理撤权并正确回收。不能以应用队列满、信用耗尽或仅暂停 Core 消费替代系统背压。

这两个目标分别使用独立场景与批准。真实网络仅限新建合成 loopback fixture，不访问账号或用户内容，不自动重试或重发 Unknown。

## 固定观测要求

- 读暂停由 owned-I/O 线程在完整协议帧边界发布：先前 ReadFile 已完成/回收，没有在途读，发布后不再 issue。不得通过永久 cancel_all 后假装恢复通道来实现暂停。
- 同一写操作需记录不可复用 operation ID、issue ordinal、偏移、提交长度、首次 ERROR_IO_PENDING，以及至少三次实际 ERROR_IO_INCOMPLETE 的单调采样时刻与间隔。用 pipe-owner 独立固定 origin 的 `Instant` 域即可，ns 的10^9比例只是时长编码单位，不是硬件频率或精度；分辨率未测须明示。重复提交不同写操作不算。
- 原有 16KiB 信用窗口、1024 字节管道配置和消息/队列限额不为制造测试结果放宽；达不到实际 pending 就报告未合格。
- harness 用同一个计时来源测量撤权发送至对应 ACK 接收，先定义该 ACK 的含义。宿主应用、客户端取消门关闭及 Close ACK 分开记录，不混用；不同进程的 elapsed 值不直接相减。
- 至少三次采样只证明这些点未完成；pipe-owner收到取消请求时必须再对同一操作poll并记录原时刻。若已完成，不能判为“撤权到达时仍pending”。harness在reader收到完整匹配回执行时即时记录接收时间，后续解析另列；host应用ACK要求persisted/runtime_applied且无application_pending，并与source1/reason19应用事实对应，预锁往返目标≤500ms，不冒称guest取消门延迟。
- 追踪同一个 OVERLAPPED 的终态与实际 reap，然后分别记录 worker 退出/join、RequestClosed、子进程退出与双 EOF、持久 owner Released。CancelIoEx 成功不等于回收完成。
- fixture 服务、请求处理和控制读取线程保留可 join 的句柄，记录实际 join 或仍存活；daemon、server_close 或最终进程退出不能替代线程证据。

## 实施顺序与当前授权

宿主和插件先提交简短可行设计，列出复用固定产物、必须派生的新候选及可信测试屏障如何绑定执行配置。联合按上述要求只读审查。此刻只准备设计，不改实施源码、不构建、不启动新进程或 HTTP。

随后由主协调固定最小实现与运行范围；测试开关不得变成普通插件的新权限或任意生产协议命令。沿用原单调期限和一次发送规则；超时/Unknown/未确认资源保留原证据与归属，不自动接管。OS 部分写入故障、完整产品门槛和 SDK 冻结仍另列未完成，不因本片启动而升级。

## 设计签收与最小实施范围

主协调已读取宿主 `host/m03-stream-001/REVOKE-BARRIERS-DESIGN-009.md` 和插件 `receipts/m03-fixture-design-001/revoke-backpressure.md`。最后收紧慢IO隔离、有界join及采样硬门后，最终绑定摘要为宿主 `39bf723fd17cf96d836dde7aaf432d18fbe7d4390c59aebfba58497fe0526db6`、插件 `c69823c6e9f210dcf8b9de3bc4207a0ce7feba1c45ed869bacc11244ffd5df31`，主协调已核对。联合只读对齐认为设计可行；原设计准备阶段的禁止实施安排由本段更新，仍无运行通过结论。

已放行新fixture适配层/原生客户端与宿主观测候选的最小实现、编译及必要本地测试。upstream Core、旧qualification002/native003、固定host004、v3 wire和pipe平台不改。A复用host004；B需新的pipe-owner采样观测。原6个CLI值之外只增spec摘要一对，保持8参数上限；spec绑定实际字节、mode、nonce和固定相对文件，不赋予新权限。

A实际事件A经原permit入队后被真实next_event取出，在原交付门前hold；实际事件B在reserve之后的原producer门前hold。撤权后两门分别拒绝对应原事件，序号/摘要贯穿证据。B在真实DataBound帧完成且无在途Read时hold，不cancel_all再恢复，继续Write/control/poll和取消。全部marker慢IO移到锁外有界writer，证据缺失判未合格。guest控制/证据及fixture线程仅在既有预算内观察结束再显式join，不阻塞异步控制，不续期。

本地回归与构建完成后提交固定候选供联合源码复核，真实场景由主协调再安排A一次，收口后B一次。当前不启动真实host/guest配对或HTTP，不自动重跑旧套件，也不新增部分写故障场景。

## B 宿主观测候选

host005 已完成最小实现并固定，harness与新fixture客户端仍待交付。主协调核对 `host/m03-stream-001/native-host-candidate-005/manifest.json`（`395521127ff99358702e57cc13604808eb885ff740b79e02341bb70465d8cb15`）和exe（`9b2a2412ce9fe617729757b8be7eddb34a84718ad64aa507f2bfdc9de926d6af`），读取修订说明及3/3本地测试输出。

相对004只改pipe_driver、supervisor观测转发和新增观测测试。真实同进程owned pipe在8192字节写/1024buffer下记录同op三次≥25ms采样、取消前补poll、995完成/reap及join；提前取消不补造三样本；completed/error分类为纯数据测试。该结果没有child、真实插件或HTTP，不授予完整B资格。新候选已交联合只读复核，旧v3/pipe/Core/ledger/限额保持不变，真实A/B仍未安排运行。

联合host005限定复核无新阻塞，主协调读取并核对 `native-host-review-005/review.md`（`74b54acbde3f38c15604baf174d28ed9dc1a58954f40abd7eee296693f8e4455`）、manifest（`116773bf378799e21169da39ef14e690c27852fa5e727815e3e29b923fd5019e`）及result（`91e88ff063a9cfad9ce2d6c0fe9a184098548e3b1e94dc79373111c97aec299c`）。取消补poll取得完成时，原结果由reap分支消费一次，不重复poll；其时间为原poll区间，不代表CancelIoEx导致完成。实际三采样间隔25.1034/25.9258ms，取消补poll仍incomplete，随后实际995回收。本地完成-at-cancel与partial-tail路径仅源码/合成分类核查；未有真实guest停读、撤权ACK时延或完整B运行资格。插件fixture接缝仍在实现，候选未READY。

## Fixture001 与 harness010 固定候选

插件 fixture001 已就绪：manifest `06a90c8d1adb8f09d13e83e177d55a4728792f2214000954c296728962c5b689`，exe `515ed83d1b27d45df5d4cdcc18c41244b5b6ac930528452cc45a64fe0ada89ad`。主协调已核验全部55项输入摘要并读取接口；本地 Core 11项、native 18项通过，均不代表真实 A/B 运行资格。

联合发现 producer reserved 事件可在取消信号唤醒后先于锁外 gate marker 发布抑制记录。冻结009保留，harness010 (`b90c9059a3dbb573a4ef0060c80d6d76da7b01cde1b66cc40d11051dbc41115b`) 修正这一异步日志时序假设，仍要求同事件、原门拒绝、permit释放、无业务交付，以及独立宿主持久撤权/客户端接收和最终取消门证据。A消费者仍在观察到取消门后才放行，顺序约束不变。纯回归接受两种合法顺序并拒绝六种异常变异。

主协调已读取010完整运行路径；联合正在完成 fixture001 与010最终只读复核。当前仍未安排真实 A/B；候选通过后先 A 一次，审查收口后再 B 一次。

## A 一次运行安排

主协调已读取 `joint/m03-stream-001/native-fixture-review-001/review.md` 和 `harness-review-010/review.md`，限定复核无剩余阻塞。已安排宿主仅执行 A `core-revoke` 一次：host004/fixture001/harness010，新profile/op/grant/spec，最多一次合成loopback POST，期限和限额不变。失败保留证据并停止，不自动重试；联合仅只读审查原始结果与持久ledger。B `data-pending` 仍未安排，须A收口后单独推进。

## A010 首次真实运行失败（保留）

批次 `host/m03-stream-001/revoke-010-core-revoke-20260929T010056786082Z` 仅一次POST，未自动重试。原result failed：插件 aggregate/Close/final control 为 `Protocol("unexpected data EOF/error")`，task cleanup 为 `CleanupUnconfirmed`，本地data线程未确认join。宿主source1/reason19撤权应用和最终Released、独立控制clean均不能掩盖这些失败；A不授予通过资格，B保持未运行。

原始证据显示 guest Read id9 收到0字节/error109，guest HttpCancel 已发送后才收到 host HttpTerminal。NativeFailure先关闭门，因此虽然目标事件均经原门抑制且未进入业务输出，也不能声称已验证干净的宿主撤权路径。主协调已读取原result、最终宿主progress及guest task/close/fixture/thread记录；宿主回报同harness时钟撤权ACK15.2387ms，单POST、host0/guest2、双EOF与宿主Released，fixture线程join。

当前只读定位跨lane关闭竞态及错误路径提前返回cleanup的问题，禁止白名单Protocol、原失败翻判或放宽清理要求。后续修复使用新候选、新批次，原fixture001/host004/005/010冻结；不重复外发原Unknown。

## Fixture002 最小修复范围

已安排插件派生新fixture002，不改旧固定候选、host004/005、v3、pipe或upstream。对完整帧边界的已知disconnect，停止新IO/驱动交付/Commit/正向credit，独立控制router继续，在原authority截止与既有500ms界限的较小值内核实同tuple合法撤权/RequestClosed；不得续期、重新打开pipe、重发Unknown，不能先记NativeFailure再清除。部分帧、其他IO错误、坏控制或无法解释的超时仍保留sticky错误。须检查等待区间对原Core排队/预留交付门的影响，不能新增输出窗口。

数据线程工作结果与结束/join分离：错误路径也仅对已finished句柄执行join，原错误保留，不把线程回收成功当业务成功。补充宿主控制应用前后取消门和first_cancel_reason因果证据，后续A须证明由真实HostCancelled首先关门。定向本地回归和编译已安排，真实HTTP尚未重新安排。

宿主只读核实表明effect fence自身即可触发pipe owner取消；仅调整supervisor先后无效。即使控制写出先发生，独立接收线程仍可能先处理EOF。因此此轮不改宿主顺序。harness011等待新接口后派生，仅纯检查，010失败保持原判。

### 修复实现中的审核补充

暂停为单调delivery暂停，独立于cancel token和first reason，等待原事件/permit时不持锁；只走后续真实取消或失败，不恢复live。主协调提前检查发现控制writer在分配sequence后再拒绝尚未发送帧可能形成空洞，已要求并由插件修正为同一短锁中的最终准入、序号分配、request登记，之后才锁外encode/write。该准入记录不是OS issue时间；已逻辑准入帧可完成，最终权限仍由宿主effect fence约束，本片不改同步控制IO架构。

首次本地回归Core14项通过；原生编译初轮发现MutexGuard作用域跨await问题，修正后生产方回报native26项通过。正在补齐必要因果与暂停准入观测并冻结新候选，尚未安排新的真实HTTP。

### Fixture002 预审 F002-ACK

14+26本地回归与首轮构建通过后，联合预审发现单调pause错误屏蔽整个credit生成分支，连真实HostCancelled之后window0的已消费结算也会阻断。A010原始数据表明parser消费可先于最终ACK，不能把未确认offset伪补一致。已安排插件仅修正零窗口生成与准入，暂停期正向额度仍禁止，不修改消费分类；新增“已消费dirty→pause→host revoke→window0连续序号”回归，保留前轮产物并重跑受影响native检查/构建。该问题解决前不安排新A，harness011仅纯检查、启动pin未填充。

### F002-ACK 终局链补充

联合继续核实原A010顺序：RequestClosed可先于最后HttpCredit/State。只恢复zero-credit生成仍不够，cleanup不能使用早期closed快照中的peer offset；Close也不能抢先结束writer而跳过待结算credit。已安排在既有清理预算内跟踪实际准入Credit序号/真实消费分类，并仅在正常验证的匹配State/code0且分类一致后确认结算；RequestClosed提供资源回收事实，最新已验证progress提供消费确认。未收到或非法ACK保持CleanupUnconfirmed，不补造offset、不重发、不续期。待整个终局链的定向回归完成后统一冻结，不安排新HTTP。

## Fixture002 固定交付

新manifest `6a8d4d07aab156d72f5fce73dea7750f8b5424707dae84595182ff7f55c211a5`，exe `b895a7d78cd6c3c3641e502ad63b167cf5b78bdfaab75bfde079710efb0a08bc`。主协调核验79项输入摘要全部一致，读取最终revision/interface与native31项通过日志；Core同源码14项本地通过。终局CreditState匹配、旧RequestClosed快照、最终小额zero-credit及preHead零消费边界均已有定向回归，尚待联合最终复核与011固定绑定后安排新A。旧A010及所有pre-final失败日志保留；没有实际运行或产品资格升级。

## 新 A011 单次生产运行通过，待独立证据收口

fixture002 与 harness011 (`641dece216bda441067123b288ae62565deb79478cdb951d307bdb04c2dc536e`) 限定联审无阻塞后，主协调安排新A一次。批次 `host/m03-stream-001/revoke-011-core-revoke-20260929T015028059792Z`，result `84636aa83df189a8fa6cacaf076c2473a0a679e95c02d0b8bf183d0d7111ae99`，manifest `888ab7bec35e60459251743171e154fad4b6aa8525a1a43a7cbe2d67ee84ad6a`。

仅1POST，实际触发data先断开pause，500ms固定上限；真正host控制令未取消门关闭，首因HostCancelled，原A/B目标均抑制且无业务交付。pause后无新data issue/Commit/positiveCredit准入，zeroCredit seq6匹配CreditState后Close7；cleanup确认197字节，全部data/control/fixture线程join，host0/guest2/双EOF/Released。Unknown与无HTTP EOF/无response material保留，未当成业务成功。主协调读取最终断言、cleanup/线程记录并核result摘要，联合正在只读核原始事件及ledger。旧A010仍失败不翻判；B暂未运行。

## A011 限定签收与 B 一次安排

联合 `joint/m03-stream-001/core-revoke-review-011` 独立只读复核通过，主协调读取result与durable ledger后接受此单案例。原始CreditState确认197先于Close，data-first pause与HostCancelled首因均证实；native ledger source1/reason19/TTL10000、sendbudget1及Released/exit双EOF一致，数据库只读校验未变。该结论不是联合重新运行HTTP，不扩展为完整M03。

已安排B `data-pending` 一次，固定host005/fixture002/harness011，新profile/op/grant/nonce、仅一次合成POST、原限额/期限不变。要求同OS写操作三次>=25ms incomplete、取消前第四poll仍pending、同操作实际reap与线程join，host应用ACK<=500ms及完整guest控制/cleanup/进程释放。达不到即保留失败停止，不自动重试/增大buffer或credit。

## B011 单次生产运行通过，待独立证据收口

批次 `host/m03-stream-001/revoke-011-data-pending-20260929T015836101741Z`，result `d86ded6671fb9dbb1fb6d48a7956cba07d8d5c10bee51641ac9620fc7ffc603a`，manifest `0559231548c887835c88202e04780fc933dd17b48be9b5756d85c9826b8619cf`。主协调已读取最终结果、原pipe-owner操作链及DataBound读暂停记录。

DataBound完整帧边界、无在途读，lastRead3/总issue2保持。宿主同写op12/issueordinal3、提交1364字节（body_end1024）初始pending，三次实际incomplete采样间隔25.4935/25.8365ms；取消前第四poll仍incomplete，取消请求之后同op完成0字节/995并reap。各时刻同pipe-owner域，不冒称硬件精度或跨域延迟。宿主应用ACK15.8194ms（harness同钟），单POST22025字节；received8186/reserved2048/issued1024/OScompleted0/peer0，guest/parser0，Unknown19且无EOF材料。

cleanupOk、data/control/evidence/harness全部join，控制clean、host0/guest2/双EOF/Released；本次控制先到，没有data-end核实pause，区别于DataBound主动停读。联合只读审证中，不新增场景。部分写故障、完整并发/平台矩阵和产品门槛仍未完成。

## 本轮最终签收与后续缺口

联合 `joint/m03-stream-001/data-pending-review-011` 只读复核B通过，主协调已读取独立result及durable ledger：同op12三样本、取消前仍pending和0B/995实际reap成立；读issue总数2、parser/peer0、ACK15.8194ms、cleanup和所有join吻合。原生账本source1/reason19、原TTL10000、sendbudget1及Released/exit双EOF确认，DB只读哈希未变。与A011各是一项真实生产方运行加独立审证，不冒称联合复跑或覆盖全部并发交错。

本切片收口，固定候选保留；下一工作顺序为：先设计并验证OS部分写入故障及取消临界完成竞态，再补相关故障矩阵与产品接线验收。同步控制写逻辑准入到OS调用间隔、同用户隔离、崩溃后owner核对、跨平台与完整产品图仍有已知边界。原84产品场景与SDK冻结不因本切片升级。没有新测试重试、Git提交/推送、tag或Release。
