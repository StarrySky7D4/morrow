# API节点管理私有消息验证

基线 `fb91f956ddc88067e3c577a4d51b9a5be9d0dab5`，隔离分支 `codex/io-safety-refactor`；应用版本保持 `0.1.9-test.52+56`。**PASS_SCOPED**：七个管理动作已接Rust宿主、私有Cap'n Proto、Dart模型和原生适配。没有新增服务管理页面或主应用监听入口，不称作常驻API节点完成。

## 实现范围

- 配置分页/保存/停用、授权分页、认证签发/轮换、授权停用和发布批准保存，全部沿用原Storage与Busy门禁；没有第二份存储、数据库升级或用户数据迁移。
- 原生字段与嵌套总量有界，配置最大64主体/128范围/64批准引用的历史记录可在128 KiB帧内返回；两个分页游标分别使用文本和二进制，快照变动拒绝续页。
- 元数据不包含验证摘要或令牌。一次令牌使用独立解析器和可dispose的字节所有者，Rust/CLI保护序列化缓冲区，Dart清理其持有的帧、临时令牌副本及可写接收缓冲区；不承诺操作系统或不可写运行时缓冲区可清零。
- 错误回复从空消息重新构建，失败写入封锁通道，迟到回复不能被后续请求接收；不自动重发。尚未等待的pending Future仍消费错误，避免普通请求出现额外未处理异步错误。
- Dart元数据独立持有只读副本，修订使用BigInt，原生编解码与平台无关模型分离。历史过期/停用记录、TRACE政策和TLS IPv6数值scope保持可读。

## 验证

| 检查 | 结果 | 本地证据 |
| --- | --- | --- |
| 宿主完整Release/all-features | 171 passed / 0 failed / 0 ignored，27个顶层目标 | `build/service-wire-host-full.log` |
| 新协议定点回归 | 6 passed；修正Clippy后复跑通过 | `build/service-wire-host-tests-final.log` |
| Dart模型、Windows真实服务/端点/凭据/HTTP任务与关闭回归 | 22 passed，无跳过 | `build/service-wire-native-final.log` |
| 全仓Flutter分析 | No issues found | `build/service-wire-analysis-final.log` |
| 宿主全目标严格Clippy | PASS | `build/service-wire-clippy-final.log` |
| Web Release和Wasm dry run | 构建成功；不是浏览器运行验收 | `build/service-wire-web.log` |
| 私有绑定同步/冻结SDK | 生成器check通过；36固定文件、13原Wasm/包对通过 | 本地检查输出，未重打包原包 |
| 修改文件格式与差异 | Dart格式、Rust修改文件格式、diff check通过 | 本地检查输出 |

宿主数量按Running/Doc-tests顶层区块统计，不重复累计内部子进程测试。22项Dart回归包含13项模型测试和9项原生/受控进程测试；受控Python进程仅证明断流与关闭行为，不能代替Rust存储回收证据。真实Rust服务管理测试使用声明合法但不执行的guest包，验证原库重开、修订冲突、令牌轮换、停用以及占用端口下仍可保存；并非入站执行测试。

## 发现与修复

独立复审发现并修复失败flush后连接复用导致迟到回包错配，以及普通请求pending Future未消费错误；新增普通/服务请求两条真实子进程失败测试。补充stdout可写输入块清理与历史IPv6 scope兼容回归。

首次全仓分析发现独立Demo缺少本地package配置；在Demo恢复依赖后分析通过，恢复依赖产生的锁文件/注册文件变动未纳入本次提交。首次严格Clippy发现新测试6处多余借用，修正并复跑通过。全crate格式检查仍报告既有 `workbench_host/build.rs` 数组排版差异；该文件未改，本轮所有修改Rust文件单独格式检查通过。测试记录保留初次失败，不将其计为成功。

## 下一阶段

先接双语配置/认证/发布批准页面与一次令牌呈现，再完成常驻监听租约、单请求预算、原Storage统一调度、监听监督与worker真实join回收。现有30秒限制和常驻期间内容Busy仍是待解决项；不能通过自动重绑定或第二个数据库规避。Unknown证据核对、文件系统、三语言IO SDK及跨平台运行资格仍按原门槛推进。

本阶段只有本地验证与提交，没有推送、发布或关机。当前会话未提供SubagentBridge调用工具，未声称完成DeepSeek调用；使用既有内置子代理协助实现与复审。
