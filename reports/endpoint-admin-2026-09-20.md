# 主应用端点管理验证

基线：`5bb4aa82bf5e2def671f84a5c56c13e8e99e2cac`，隔离分支 `codex/io-safety-refactor`。应用保持 `0.1.9-test.52+56`。结论为 **PASS_SCOPED**：Windows 原生端点配置管理通过，完整主应用网络任务与全平台资格仍未完成。

## 交付范围

原 Store 的端点分页、新建／替换／停用已接入 Rust Workbench、追加私有 Cap'n Proto 动作、Dart 原生适配及插件库面板。面板提供完整政策、凭据引用、HTTPS DER 根和额度；保存复核原 Registry、包摘要、声明及类别批准，记录替换保留 CAS。成功写入沿用原授权撤销，失败 CAS 不撤销；停用不依赖当前包存在。保存凭据引用只读元数据，不提前解密秘密或连接网络。

读取保留完整历史政策，两条共享记录一页，混合空页继续前进；快照变化拒绝续页。私有协议最大 DER 页测试逐字节保留三份 32768 字节历史根（两页），响应不超过 128 KiB。测试历史根并未作为有效 TLS 证书授权。

新建与实际任务批准复用同一传输政策校验。独立审查发现界面错误地将自定义 DER 限为本机 HTTPS；现已支持公开及本机 HTTPS，并新增回归。HTTP 不提供证书选择。冻结 guest SDK 未改动，宿主和 Dart 已按新私有 schema 同步构建。

## 验证

| 检查 | 结果 | 本地日志 |
| --- | --- | --- |
| 宿主完整 Release/all-features | 148 passed / 0 failed / 0 ignored，含本轮 10 项端点测试 | `build/endpoint-host-full.log` |
| 网络模块完整 Release/all-features | 100 passed / 0 failed / 0 ignored | `build/endpoint-network-all-tests.log` |
| 宿主、网络全目标严格 Clippy | PASS，无新增豁免 | `build/endpoint-host-clippy.log`、`build/endpoint-network-all-clippy.log` |
| 端点与凭据 widget | 38 passed / 0 failed | `build/endpoint-widget-final.log` |
| 原生端点／凭据／类别与插件库回归 | 20 passed / 0 failed，无跳过 | `build/endpoint-native-final.log` |
| 新增／修改 Dart 页面、适配与测试分析 | No issues found | `build/endpoint-analysis-final.log` |
| Flutter Web JavaScript Release | PASS，Wasm dry run 同时通过 | `build/endpoint-web-release.log` |
| 私有协议生成一致性、i18n 资源、冻结原包 | PASS；36 固定文件、13 对原 Wasm／包未重打包 | 生成器 `--check`、i18n `--check`、SDK baseline verifier |

Rust 数量按每个 Cargo 顶层 Running／Doc-tests 区块的末次结果统计，避免重复累计审计子进程。宿主已重新构建，新协议真实跨进程测试使用该 Release 程序；网络全部功能测试覆盖既有真实 HTTP/TLS、受管 IO、入站与出站批准场景。

新增原生端点测试经真实 RustWorkbench 子进程保存非默认完整政策、引用 Windows 受保护凭据、遍历多页、替换、拒绝旧 CAS／快照／记录种类／溢出修订，关闭后重开确认保留；移除插件后仍可停用并再次重启验证。旁路本机 socket 计数为零，配置过程没有请求。widget 测试覆盖中英文 320px、证书边界、缺包停用、A→B→A、摘要／修订变化、表单关闭、迟到 picker／save／disable、未知结果不重发。

初次分析因本机 Dart perf 文件 reparse 错误退出，见 `build/endpoint-native-analyze.log`；将 DART_DATA_HOME 指向本工作树 build 内独立目录后通过，没有删除系统 perf 数据。初次原生组合测试命令误写凭据测试文件名而加载失败（端点及类别测试当时已通过），见 `build/endpoint-native-tests.log`；修正为实际文件后完整 20 项通过。不将上述失败记作通过结果。

## 未覆盖与下一项

- widget 模拟交互与真实原生适配分别通过，未提供端点页面手工操作或视觉录屏证据；未构建新的 Windows 安装包。
- 尚无私有网络任务启动／查询／读取／取消消息或完整关闭待退出协议，因此没有从端点页面发起实际网络请求的产品验收。
- Web 仅验证构建；无原生端点后端时面板隐藏，不声称浏览器网络／凭据提供者已实现。
- 下一项是将原持久端点解析接入可信任务准备回调、短响应私有任务协议与 Flutter 状态界面；随后推进 API 节点、文件系统、Unknown 核对、完整因果链及三语言 IO SDK。
- 本轮仅本地实现与验证，无推送或发布。

实现合同见[端点管理](../docs/PLUGIN_ENDPOINT_MANAGEMENT.md)，任务边界见[应用 IO 状态](../docs/PLUGIN_APP_IO_TASKS.md)。
