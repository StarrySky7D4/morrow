# 主应用网络与文件权限管理

当前主应用管理的是插件的 IO **类别批准**。它不替代具体连接地址、文件范围、凭据、监听地址和服务发布批准，也不会创建活动 IO 绑定。应用版本仍为 `0.1.9-test.52+56`。

## 已接通的流程

插件库分别展示内容权限与网络／文件权限。后者覆盖 `file-read`、`file-list`、`file-create`、`file-replace`、`file-delete`、`http-request`、`http-listen`、`http-publish`、`credential-use`、`websocket-connect`。展示声明不表示对应平台或执行后端已完成。

勾选只改变当前表单；用户明确保存后，Flutter 通过私有 Cap'n Proto 请求调用原 Manager 的 `approve_io`。这个操作保持内容批准和 enabled 状态，不自动启用插件。清空使用独立的撤销按钮。权限变更需要关闭已有表单，之后重新打开。

请求绑定原包摘要及 Registry 修订。宿主拒绝未知／重复／超额类别、过期修订、错误摘要及声明之外的权限。非空批准必须重新验证原包；清空全部 IO 权限只需核对当前选择身份与修订。因此插件文件在运行期间丢失时仍可撤权。重启时缺失包仍触发原 Registry 完整性拒绝；恢复完全相同的原包后，已清空的权限不会恢复。

Manager 是唯一类别批准来源。有效修改撤销旧实例；持久化失败不能恢复已撤销的实例。界面未知回执只刷新状态，不自动重复写入。资源批准与类别批准属于不同事务，不宣称原子保存两者。

## 私有协议与界面生命周期

`host.capnp` 新增 `pluginApproveIo`、独立 `approvedIoCapabilities`，目录增加 `declaredIo`／`approvedIo`。旧动作编号及内容批准字段不变；宿主和 Dart 均从同一 schema 生成并校验完整摘要，必须同步构建。冻结的插件 SDK 和原 guest 包未改动。

插件库以会话代数校验每个异步返回。切换 A→B→A 也会拒绝最初 A 的迟到目录、保存结果和 finally；旧表单关闭失败不写入新会话的关闭状态。关闭失败仍要求明确重试，不伪装关闭成功。中英文文案与窄屏布局使用现有按钮和主题风格。

## 后续应用接线

1. 在原 Store 增加有界的出站批准／凭据元数据列表；主应用录入、轮换和禁用凭据。列表只返回引用、修订、期限和状态，不能返回明文或密文。Windows 录入复用现有 DPAPI `seal`，禁止 Dart 另存权威配置或密钥。
2. 保持原审计 Session、HostRuntime、签名器和数据库租约的唯一所有权，完成网络作业的宿主接线。现有 IoWorker 消费 HostRuntime，而工作台 Storage/Session 与 Pool 持有原运行时及实例；禁止为绕过所有权另开数据库、创建替代 HostRuntime 或偷取 Pool 实例。先实现可验证的拥有者交接／原拥有线程执行边界，再接实际请求。
3. 私有管理协议提供短响应的 start／poll／read／cancel；不得让长期请求堵住当前串行 Flutter 通道。使用原 Manager／实例／IoBinding，再解析 Store 中的端点批准并恢复系统凭据。Ready 读取仍需最后一次授权检查；未知结果不自动重发。
4. 验证真实主应用输入→批准→网络效果→撤销→重启核对，再扩展服务发布、文件系统与三语言 SDK。当前纯 InlineUi 不接受 IO 声明包，不能用普通表单测试替代实际 IO 执行验收。

验证范围与证据见 [本轮报告](../reports/plugin-io-management-2026-09-20.md)。整个 IO-E2 和插件系统仍进行中。
