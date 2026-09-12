# 受管理的插件连接与撤权

原生 `morrow_plugin_runtime::manager::Manager` 将持久注册表批准绑定到实际宿主连接。该模块依赖 `packages` feature，复用现有 HostRuntime；不创建第二份内容库。

## 可信宿主接入

1. 使用 Catalog 校验／安装不可变包，打开 Registry，将 Registry 所有权交给 `Manager::new(registry, limits)`。
2. 通过 manager 的 `select / approve / set_enabled / remove` 修改选择。沿用全局修订 CAS 和批准／启用摘要确认，只有只读 `revision / selection / selections` 可用，不导出 Registry 可变引用。
3. 使用 `manager.connect(id, &mut host)`，仅接受已启用的选中包，重新校验归档并准备实际 Wasm。返回的 ManagedInstance 持有 PreparedPackage、真实 Connection、撤权与取消控制。
4. 使用 `instance.parts_mut()` 对真实连接发放对象授权。`connect_package_approved` 把已批准能力集合固定到 Connection ceiling；宿主不能通过这条连接发放声明以外或声明内未批准的种类。即使种类获批准，具体卡片／附件授权仍需独立授予。
5. `instance.run / run_task` 使用同一 HostRuntime。可信 UI 适配器可借用 `package / connection`；连接的 `binding()` 是包含宿主和代次的不可解析标识，同包不同实例也不能替换它。
6. 完成后 `instance.close(&mut host)` 释放宿主有限实例记录；Drop 只做不可逆撤权和取消，不持有 HostRuntime，不能自动删除其记录。长期服务必须执行 close，或结束整个 HostRuntime。

## 变更顺序与失败

有效的选择、批准、启用状态变更或移除在持久发布前先撤销该 ID 的所有既有实例，再请求取消。批准扩大也要求新连接，不原地扩大已有连接上限。其他包实例保持自己的生命周期。升级仍默认禁用，批准集合仍只保留交集。

旧修订和旧摘要确认在撤权之前返回冲突，不误停新实例。真正变更若后续校验或持久发布失败，旧持久选择及修订保持，但旧实例可以已经停止；撤权不可逆，不自动恢复、重连或重试。调用者必须报告失败，重新读取设置并作可信决定，不能把失败当作升级成功。新连接仅反映当前仍有效的持久选择，不代表之前失败的撤权意图已保存。

Manager 持有实例控制的弱引用；ManagedInstance Drop 和 Manager Drop 都请求撤权、取消。现有连接在后续运行前被拒绝，宿主读取与提交的最终授权边界也检查撤权。纯 guest 计算仍以 fuel 有界，取消不承诺即时中断。已经越过最后授权并提交的内容不会因取消、Trap、超时或管理器销毁回滚；要查询权威操作回执，不自动重复写入。

## 保证边界

这是可信生产入口，低层 `PreparedPackage::connect`、`HostRuntime::connect_package` 等仍是可信适配和资格工具 API。任意修改宿主代码或绕过 Manager 的同进程代码不受本模块安全隔离；不向 guest 暴露管理入口或可选连接身份。包签名、作者信任、跨设备依赖协作、进程强制终止仍是独立任务。

当前 Manager 不拥有后台 Worker，不自动等待正在执行的 guest 结束；撤权和取消信号用于防止后续授权与交付，不是线程终止证明。持久性范围继承 [注册表说明](PLUGIN_REGISTRY.md)，未宣称物理断电恢复资格。

## 验证

Windows 合成数据：10 项 manager 集成测试，覆盖真实 Wasm 输出、启用门控、声明内未批准 grant 拒绝、全部旧实例撤权、读取释放阶段拒绝、持久失败不可逆停止、旧确认不误停、foreign host 拒绝、Manager Drop、纯转换撤权、guest Trap 后保留已提交回执，以及超过实例容量的正常 close 复用和损坏包拒绝。

```powershell
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --target-dir build/plugin-managed --test manager
cargo clippy --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --target-dir build/plugin-managed --all-targets -- -D warnings
```

实际工作台 UI 与运行交互证据由宿主接入验收另外记录，不从本模块测试推断。


## test.25 工作台接入

Windows 主应用已通过此入口连接随包工作台，设置页支持启停和在线文字工具。批准状态位于活动库登记根，恢复内容快照不回滚该策略。首次安装策略、故障只读语义及实际运行范围见 [test.25 记录](../reports/test.25-managed-plugin-ui.md)。


## test.29 依赖批准与原连接钉定

Manager将Registry中的依赖批准重新绑定到自身创建且仍活跃的caller/provider。Control保存原ConnectionBinding；通过parts_mut错误替换连接后，归属、run、run_task和close检查拒绝错配，避免旧控制对象被误用于新连接。持久变更前按旧图停止传递必需消费者，可选消费者可保留但旧provider撤权仍使相关route／输出失效。记录不包含scope、期限或运行句柄，重启必须重新建立。见 [依赖锁](PLUGIN_DEPENDENCY_LOCKS.md)。
