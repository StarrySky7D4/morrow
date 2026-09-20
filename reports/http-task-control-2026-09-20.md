# HTTP任务控制与关闭验证

基线 `38967c60f4bd603e8dd93bf4f71eabb61ed2a418`，分支 `codex/io-safety-refactor`，应用版本保持 `0.1.9-test.52+56`。本阶段 **PASS_SCOPED**：Windows 原生控制接口、真实 Rust guest 和关闭回收通过；完整任务页面及插件系统整体仍进行中。

## 实现

- 增加原 Store 端点引用／修订到 `start_io` 的可信入口。原实例准入后解析系统凭据；Tokio runtime 随路由对象由 worker 持有。限定 `morrow.http.forward.v1` 输入 profile 和一次调用，额度与包声明相交。
- 独立 Rust guest、固定 IO 输入帧、打包工具；不链接可信 core、不修改冻结 SDK。声明 profile 不是信任捷径：独立审查发现 guest 可改写请求，现路由保存宿主原帧并在派发前逐字节比较、核对调用序号。
- 追加私有启动、状态、轮询、读取、取消、恢复和确认消息；Dart 使用有界二进制模型和精确 BigInt 修订。启动身份关联可只读查询，同进程失败尝试也不得重用；无自动重放。
- CLI EOF／输入输出错误统一停止待回收，Dart 不再五秒强杀，保持同一个关闭 Future、拒绝新调用并等待真实退出。

## 已验证

| 检查 | 结果 | 本地证据 |
| --- | --- | --- |
| 宿主完整 Release/all-features | 157 passed / 0 failed / 0 ignored，25 个顶层目标 | `build/http-task-host-full-final.log` |
| 新增真实 HTTP 与协议用例 | 6 passed，已包含于上述157项 | `build/http-task-tests-second.log` |
| 新增 CLI 关闭用例 | 3 passed，已包含于上述157项；覆盖8种关流路径 | `build/http-cli-tests.log` |
| 全目标严格 Clippy | PASS，无新增豁免 | `build/http-task-host-clippy.log` |
| Dart IO模型及插件库 | 25 passed / 0 failed | `build/http-task-dart-unit.log` |
| Dart→真实宿主、原端点／凭据／类别、受控子进程关闭 | 6 passed / 0 failed，无跳过 | `build/http-task-native-final.log` |
| Rust guest 独立单测／Wasm构建 | 5 passed；真实 wasm32 Release 成功 | `plugins/http_forward/src/forward.rs` 与下列产物 |
| Dart分析、私有生成一致性 | PASS | `build/http-task-analysis-final.log`、`build/http-task-native-analysis-final.log`；生成器 `--check` |
| Flutter Web JavaScript Release | PASS，Wasm dry run 同时通过 | `build/http-task-web.log` |
| 冻结SDK原件 | 36固定文件、13对原Wasm／包校验通过；未重打包 | baseline verifier |

Rust 数量按每个 Cargo 顶层 Running／Doc-tests 区块最后结果统计，不重复计算审计子进程。网络模块本轮未改动；完整宿主测试包含已有真实网络与受保护存储组合回归，不将前轮网络测试重复算入本轮新增结果。

真实 Rust guest 测试覆盖：保存批准后实际 HTTP、Ready／Busy／最终读取、原证据及重启核对、失效摘要／修订／停用／缺profile拒绝、取消后真实回收、重复提交和失败尝试关联。最大合法结果为64 KiB正文与约15 KiB头部，逐字节交付且私有帧低于128 KiB。负向 WAT 仅用作恶意输入：在合法解码并计费的导入中改写操作身份、路径、GET→端点也允许的DELETE，均在真实派发前拒绝，服务器连接数为零。

CLI 测试使用真实受管 worker 阻塞路由，观察关流仍等待且原存储锁被持有，随后释放并确认回收及重新打开。Dart 的真实 HTTP 测试检查 POST、404及正文、重复请求拒绝、活动请求关闭与原库重开；实际 HTTP 适配器支持协作取消，因此关闭可以早于服务器回应。独立受控 Python 子进程仅验证 Dart 的六秒退出等待、同一关闭Future、拒绝新调用及 exit 7 报错，**不作为受保护 Storage 运行证据**。

## 失败及修正记录

初始最大响应 fixture 使用单个15 KiB头值，超出 core 每值8 KiB限额；改为两个合法7680字节字段，未放宽生产校验。早期恶意DELETE fixture的整数编码也经修正，最终断言确保进入已解码计费的导入，避免以畸形输入冒充路由拒绝测试。旧失败保留在 `build/http-task-tests-initial.log`、`build/http-task-host-full.log`。

首轮 Dart 关闭测试错误假设真实HTTP取消必须等待服务器完成，见 `build/http-task-native.log`。核查5ms撤权检查后，修正为验证干净退出与原库重开；额外子进程才用于独立证明超过五秒仍不强杀。最终全套通过。分析阶段反馈的花括号和新测试未使用导入均已修复。

## 产物与后续

- Rust Wasm：`build/http-forward-guest/wasm32-unknown-unknown/release/morrow_http_forward_plugin.wasm`，SHA-256 `b1bd7435f6d67d817462ec43ca0a728b0dcdc0c5bae7221ca55c87fd0fc2477e`。
- 示例归档：`build/http-forward-integration.mplugin`，SHA-256 `91bb36d60896870932787b6f00ef05b90c5920317f3cb105bd91d241847b7139`。只是构建产物，未安装到用户内容库。
- 尚未接入用户任务表单／状态与恢复页面，没有手工UI验收，也未构建新的Windows安装包。Web仅构建，不宣称浏览器IO后端可用。
- 接下来补明确profile展示、任务提交／状态／取消／确认界面与中英文，再完成真实用户路径和重启后的Unknown核对。API节点管理、文件系统、完整因果链和三语言IO SDK保持原门槛。
- 本轮仅本地实现、构建与测试，没有推送、发布或关机。

契约见[HTTP任务接线](../docs/PLUGIN_APP_HTTP_TASKS.md)，范围见[开发看板](../docs/DEVELOPMENT_BOARD.md)。
