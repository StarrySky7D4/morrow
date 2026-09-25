# Web 插件运行基础对齐记录

目标仍是 Web 达到当前 Windows 开发线，尚未完成。用户允许服务部署云端，同时要求业务数据存于设备本地。本阶段产物是共享运行层与本地存储适配，尚未切换正式 Flutter 启动入口。

## 已实现

- 分离插件包执行、管理和原生 IO 特性。浏览器复用 `PreparedPackage`、`Manager`、`Pool`、依赖调用及审批算法。
- 将原生注册表文件读写提取为存储接口，原生格式、迁移和审批规则保持一致；增加 SQLite OPFS 不可变包与原生审批快照持久化。
- 补充同一 Wasm 实例内的 OPFS 库所有权检查。真实浏览器测试曾证实仅设置 SQLite EXCLUSIVE 不足以阻止同 Worker 重复打开。
- 不确定提交使注册表进入不可继续决策状态；包括无变化的确认，也必须重开后才能继续。
- 浏览器使用 `performance.now()` 的单调期限，以及 Worker 私有不可变字节复制。没有假装提供原生进程映射或 socket。

## 证据

`tool/verify_web_packages.ps1` 在本机 Chrome 154.0.8037.57 通过：

- Windows 实际工作台原包的 9 组完成回执与浏览器逐字节一致，包括 Unicode、修改、查询与插件业务失败。
- 摘要不匹配拒绝、内容命令拒绝、燃料耗尽、130 次连接回收和受保护内容未变。
- 12 个审批快照与 Windows 文件注册表逐字节一致，涵盖选择、审批、启停、升级、撤销、移除和重新选择。
- 同 Worker 重复打开拒绝；关闭 Worker 后重开仍保留审批/停用状态。
- 仓库冻结的 Rust、C、C++ 依赖插件原包通过共享依赖路由执行，与原生完成回执一致。缺少依赖审批时拒绝；撤销提供方使存量调用方失效；重启用不复活旧会话。

原生回归：`plugin_runtime` 的 manager、instance_pool、managed_dependencies、dynamic_dependencies、inline_ui、io_binding 共 64 项通过，新增 manager_storage 的提交未知/存量撤权/无操作确认回归另 1 项通过；`core` 的 plugin_registry、plugin_registry_io、dependency_registry 与新增 SQLite 测试共 38 项通过，覆盖审批、版本、依赖、独占、未知/损坏数据、完整 u64 与提交结果未知。最后再次运行 manager、manager_storage、instance_pool 的 29 项及完整浏览器验证通过。

日志位于忽略的 `build/web-package-verification.log`、`build/native-managed-regression.log`、`build/native-manager-final.log` 和 `build/registry-regression.log`，测试产物在 `build/web-package-parity`。`git diff --check` 通过。

## 未完成

正式 Flutter Web 仍使用旧启动路径。工作台内容/任务、草稿恢复与交接、附件、设备本地身份/封存、备份/恢复、管理页面、UI 会话、云端服务接口和完整发布流程尚未对齐。本阶段不是产品功能等价声明，也不是全浏览器、断电、配额耗尽或生产插件隔离验收。
