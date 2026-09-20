# 应用服务命令协议与Dart低层客户端

基线`601df45`，分支`codex/io-safety-refactor`，版本保持`0.1.9-test.52+56`。**PASS_SCOPED**：服务运行调度、有界命令身份与Dart低层客户端已接入；现有业务自动路由、Flutter运行页面和真实用户操作路径尚未完成。

## 实现和交付语义

私有Cap’n Proto动作追加61—66，不重用原动作号。ServiceRunStart携带原配置摘要/修订、发布引用/修订、包摘要与Registry修订及明确有限运行/作业/网络预算，复用原生准入。ServiceRunStatus分别提供原任务/提交身份、监听地址、绑定/监听/监督结果与原存储回收状态。丢启动回执可只读观察单个当前服务并核对submission，不自动重新启动。停止/修复/确认仍走原IoCancel/Repair/Acknowledge。

每服务的外围注册表最多保留8个未消费命令句柄及512项提交历史。非零submission与完整内层请求SHA-256绑定原随机command key；同身份同字节返回原状态，异字节冲突。失败准入不占历史，满额拒绝新增，不淘汰历史。任务确认后旧身份失效，不跨任务或进程恢复运行授权。

CommandStatus可以按command key查询，也可以仅携带原submission查询丢失的提交回执。后一条路径不需要正文，不入队、不消费Ready；未知身份返回错误，不意外执行尚未被接收的请求。CommandRead保留原句柄的最终授权复核，至多交付一次。原生读取/取消之后只留摘要、started与终态，成功已读、Unknown和取消都不能重新返回原敏感回复或自动重跑。

内层命令在入队前验证完整私有契约和64KiB上限，拒绝所有嵌套调度动作。原业务回复仍限128KiB；仅合法外层CommandRead成功响应允许256KiB封装，避免有效内层回复被元数据挤出原帧上限，其余私有响应仍128KiB。公共guest SDK不变。Rust清理内层输入、返回结果、Cap’n Proto封装payload和CLI缓冲，错误重新构建干净回复。

Dart新增ServiceRunRequest/Snapshot、OwnerCommandSnapshot/Read及WorkbenchServiceRunControl；所有UInt64先按BigInt检查再转换，身份和参数在排队前拥有独立副本。单次快速交换沿现有串行传输，命令查询不等待业务结束；新接口不自动接管现有业务方法。调度观察不覆盖界面的writable/maintenance状态，Running与已请求Stopping的合法短暂组合可以显示。仅CommandRead按256KiB解析，内层payload另限128KiB。

命令提交的嵌套凭据帧与其他发送暂存分开擦除；外层敏感回执在解码后清理。检查确认capnproto_dart的数据字段getter会分配副本，新增read的finally同时清理该临时payload，调用方拥有的结果须dispose并保护后续副本。结果dispose对调用方先前持有的可变别名也生效；不声称能清理OS或不可控语言运行时副本。

## 验证

| 检查 | 结果与证据 |
| --- | --- |
| Windows宿主release/all-features完整回归 | 195项不同测试通过：库65、CLI 3、集成127；`build/service-command-host-regression.log`，不重复累计崩溃子进程 |
| 最终submission只读查询补充后服务专项 | 15项通过（上述测试子集），`build/service-command-lookup-regression.log`；原7、注册表4、新协议4 |
| 宿主所有目标严格Clippy | 通过，`-D warnings`，`build/service-command-clippy.log` |
| Dart相关模型、请求、会话及客户端回归 | 55项通过；首次因未指定Python跳过6项，`build/service-command-dart-regression.log`；6项另按真实Python路径补跑，见下 |
| Dart受控管道与既有进程关闭回归 | 6项通过、0跳过；`build/service-command-dart-process-regression.log`。合计61项不同Dart测试；新管道3项此前也单独通过 |
| Flutter静态分析 | 改动涉及的7个手写实现/测试文件无问题，`build/service-command-dart-analyze-final.log` |
| 冻结SDK与生成契约 | 36固定文件/13原Wasm包对未变，生成Dart绑定check通过；没有重建guest或重打包 |
| 格式/差异 | 定向rustfmt、Dart format、git diff检查通过 |

实际服务测试在两个真实HTTP请求之间通过同一原State执行原Rust工作台创建命令，确认原卡片只到修订1；重复提交只返回原key，冲突/foreign key/嵌套调度拒绝。未知submission只读查询不产生key，已知submission恢复原身份。8项Ready占满后第9项拒绝，读取释放后同一未获准身份仍可提交；Ready取消保留Unknown且无重复payload。

认证旋转经原命令将已有记录从修订1改到2并撤销当前服务，接受明确业务成功或已开始Unknown，后续查询/重复读取不重新签发令牌。大回复测试通过受控诊断字段构造近128KiB原业务回复，再经真实ServiceHost和私有CommandRead逐字节核对，外层超过128KiB但不超过256KiB。该定向夹具不代表大内容吞吐基准。

Dart管道测试使用实际Python子进程交换二进制帧，检查按submission查询没有正文或command key、重复Ready不消费、响应大小规则及结果独立持有/擦除；不是实际Rust服务与Flutter界面端到端验收。既有关闭测试真实等待子进程超过5秒，检查正常/异常退出及关闭输入后的请求隔离。

首次Rust编译的类型可见性/错误转换问题已修正，失败日志保留；首次Flutter默认依赖检查遇GitHub连接重置，改用已有解析依赖和--no-pub完成。子代理直接dart analyze遇Analysis Server perf文件关闭错误；主代理通过Flutter分析入口运行，修复8项括号规范提示后退出0。没有修改SDK安装或降低lint规则。

按用户授权通过SubagentBridge官方工具取得一次有界DeepSeek配额，使用deepseek-flash/max完成合成设计审阅（SUCCEEDED/COMPLETE）；未发送源码/秘密、未重置旧账本、未自动重试。外部审阅不是正确性证据，结论由本地代码审查和测试核实。

## 下一阶段

先把现有业务调用接到原服务命令，并在等待结果时释放传输队列让停止/状态可插入；缩小当前部分64KiB上传块以满足完整内层帧64KiB预算，保留草稿、查询终态、令牌和Unknown语义。再接Flutter服务启动/状态/停止/恢复界面及真实HTTP与内容编辑并用路径。

应用准入仍限单个已批准loopback HTTP有限服务；没有自动续租、应用TLS/出站资源新适配或长IO抢占保证。Unknown持久核对、完整文件系统、三语言IO SDK和全平台资格继续按原门槛推进。没有Windows安装包、远端推送或Release；IO-D2b/IO-E2和整体持续推进目标未完成。
