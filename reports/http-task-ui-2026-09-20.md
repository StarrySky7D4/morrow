# HTTP 任务界面验证

基线 `2b587dcf8fb3208b24d6cbd06155771c2b9e4be6`，隔离分支 `codex/io-safety-refactor`；应用版本仍为 `0.1.9-test.52+56`。结论 **PASS_SCOPED**：Windows 原生 HTTP 表单和会话内任务观察通过，不代表插件系统、全平台运行或 IO SDK 已稳定。

## 实现与边界

- 插件库接入明确提交、状态查询、一次结果读取、取消、原实例恢复与完成确认；只选择当前批准且明确声明 HTTP forward handler 的包。私有目录增加 ioHandlers；原插件 SDK 不变。
- 结果区区分 HTTP 状态、执行错误、取消及远端结果未知，保留重复头部和二进制字节。未知启动/读取没有自动重试。
- 设置卸载不销毁已消费结果或未知状态，原后端会话保留最多五条观察历史；任务身份改变时归档原结果。独立后端及 A→B→A 的迟到返回隔离。
- 只有原宿主新鲜 Local/无任务状态才允许显式结束未知启动观察；归档旧身份，不宣称回滚，不修改宿主或发送请求。跨进程恢复仍须持久证据核对。
- 中英文和窄屏继承现有控件样式；公共模型/校验与原生协议编解码分离，避免 Web 引入无法精确表示的原生 schema 常量。

## 本轮证据

| 检查 | 结果 | 本地日志 |
| --- | --- | --- |
| 宿主目录 Release/all-features | 10 passed | `build/http-ui-catalog-tests.log` |
| 宿主真实 HTTP/协议负向回归 | 6 passed | `build/http-ui-host-task-tests.log` |
| 宿主 Release 构建、全目标严格 Clippy | PASS | `build/http-ui-host-build.log`、`build/http-ui-host-clippy.log` |
| Dart 模型、HTTP页面、端点、凭据及插件库 | 85 passed / 0 failed | `build/http-ui-widgets-final.log` |
| 真实表单及原生 HTTP（均含受保护凭据） | 2 passed / 0 failed，无跳过 | `build/http-ui-native-widget-verified.log` |
| 既有宿主 HTTP 与受控子进程关闭 | 2 passed；HTTP后又纳入上行，不重复累计 | `build/http-ui-existing-native.log` |
| 改动 Dart 静态分析 | PASS | `build/http-ui-analysis-final.log` |
| i18n、私有生成一致性、冻结 SDK | PASS；36固定文件/13原包对，未重打包 | 对应生成器 `--check` 与 baseline verifier |
| Flutter Web JavaScript Release | PASS，Wasm dry run 通过 | `build/http-ui-web-final.log` |

85项中HTTP页面为22项，覆盖未知启动、未知读取、修复/确认、Ready非退出、卸载期间完成、父组件真实setState、错误端点混排、二进制与重复头、320px中英文。原生表单测试通过真实Rust guest向临时loopback服务器发一次POST，校验受保护Authorization及正文，显示422和实际响应，再卸载重挂、确认回收；服务器请求数始终为一。非widget原生测试也增加真实凭据，另验关闭活动请求与原库重开。

`build/http-ui-native-preview.png` 是上述原生组件测试的渲染截图，已查看布局和响应正文；它不是完整Windows发布应用或安装包截图。所有临时网络服务器只绑定本机回环。

## 失败与修正

首轮分析暴露本地化参数顺序及测试Dropdown读取错误，已修。独立复审发现设置重挂时可同步触发父setState，现改为帧末通知并补真实父重建测试。Web初次构建失败于原生schema的64位常量，拆分公共模型和原生codec后构建通过，没有降精度或修改协议身份。

原生widget测试初始每轮推进整一秒，恰好从上次poll完成跳到下次poll开始，使读取一直落在忙状态，直至真实交付期限到期；服务器约1.1秒已响应，不能将该失败解释为网络未返回。改为20ms步进后真实响应通过。可选截图的异步渲染移入runAsync，并在操作前重新等待按钮；一次停滞测试经核对PID后停止其专用flutter_tester，最终完整用例通过。失败日志保留在 `build/http-ui-native-widget*.log` 与 `build/http-ui-web.log`，不覆盖为成功。

## 下一项

1. API节点配置/发布/停止与只读请求历史接入主应用，沿用原Store与实际授权，不因保存配置自动监听。
2. 重启后的Unknown证据核对、因果链及证据退休；界面内观察缓存不能替代此项。
3. 平台文件选择/枚举/写入/替换/删除后端与撤权、冲突和故障结果验证。
4. 上述契约验收后形成C/C++/Rust IO SDK候选；完整独立插件与平台资格分别验收。

本轮仅本地实现与验证，不推送、不发布、不关机。DeepSeek辅助得到用户许可，但当前没有可调用的SubagentBridge工具，未宣称使用该模型；已有子代理参与实现、测试设计和只读复审。
