# 失败启动隔离：Dart 接线候选

状态：**SOURCE_NOT_RUN**。本目录是供审阅的惰性候选归档，未接入生产路径，也没有运行其中的15个内存测试。

三个生产文件和一个测试文件保留已冻结候选的原始字节。`manifest.json`以相对路径记录每个候选文件、三个基线文件的SHA-256，以及验证范围；`exact-tests.json`列出15个精确测试名。

本候选只在原始 `RustWorkbench.open` 的启动异常分支标记失败。资格模式的私有通道立即关闭本地业务入口，停止未来及已排队的心跳，让原supervisor按原租约时钟观察控制端丢失；不伪造异常状态、不修改时钟、不kill或重放。只有原接收路径验证 `ClosingUnconfirmed` 与整数版本1协议后，才单次发送空kind4。已请求的正常关闭不会被改作隔离请求。

准备扩展严格核对11项原始身份（含恢复历史）、资源标志、reader joins、子进程退出码和摘要格式，并保存独立、深不可变的私有副本；公开ValueNotifier不能提供控制能力或修改最终凭据。摘要是原可信supervisor的提交记录声明，Dart不读取账本，也不能凭wire重新计算未提供的持久化原文。

仅原supervisor实际退出2且stdout/stderr双EOF，配合已写出的单次请求与受验扩展，才能产生独立 `failed-startup-quarantined` 快照。业务仍为Unknown、owner仍为Retained、normal为false、accepted为false。正常kind3/Released及原exitCode语义不放宽。原主异常和堆栈仍重新抛出，关闭及观察失败分别保留；坏frame、传输失败、缺少闭合或重复终端不会成为隔离成功。

测试源码使用内存FakeProcess并经过真实channel接收路径，覆盖错误身份/历史、假扩展、公开通知器及nestedMap注入、缺EOF、错误退出码、重复终端、传输失败、正常与非资格关闭、主异常及次级关闭记录。没有启动真实Process、调用native API、访问真实库或连接VM。

**尚未验证**：本Dart候选的静态分析和15个测试均NOT_RUN；与完整354 Rust运行时的联合编译及验收均NOT_RUN。此前独立的Rust局部测试不代表这里的联合运行证明。SDK26/G04与namespace保持OPEN，release=false。此包不提供恢复、执行、清理或发布许可。
