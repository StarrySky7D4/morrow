# ROAD-07 IO 声明、持久批准与实际实例准入

日期：2026-09-15。结论：**PASS_SCOPED**。范围为 Windows 原生宿主中的包声明、批准迁移、真实实例绑定和宿主配额准入。不是正式 guest IO 执行或完整 SDK 可用声明。

## 本轮实现

基线 HEAD：`0e93e49f56dd2534db5a0cd8fb31e9d5c705dfa7`，加当前未提交工作树。应用仍为 `0.1.9-test.50+55`，network_node 仍为 test.51 独立组件；未修改发布版本、标签或 Release。

- [独立声明](../core/src/plugin_package/io.rs) 与 [元数据 schema](../core/schemas/io_manifest.proto)：10 类 IO 权限独立于七种内容权限；`io-v1`、ABI2、版本及精确 IO 摘要共同校验。重复、未知、错配和超额拒绝；新特权 field 18 单次且嵌套编码严格，旧外围可选字段原字节保留。
- [实验 IO schema](../core/schemas/io.capnp) 随构建检查，可描述分块与作业消息，但尚无 guest codec／导入。不是冻结兼容候选。
- [Registry](../core/src/plugin_package/registry.rs)：同 selection.morrow、文件锁和 CAS 保存独立批准。严格验证 v1 后原子迁移为 v2、revision 加一、IO 批准为空；保存失败保留原件。升级禁用且批准取交集，依赖不继承调用方 IO。
- [Manager](../plugin_runtime/src/manager.rs)、[Pool](../plugin_runtime/src/instance_pool.rs) 与 [IoBinding](../plugin_runtime/src/io_binding.rs)：绑定真实宿主、Manager、连接、包和 Control；非法批准先拒绝，有效变更先撤权再持久化。绑定／配额令牌没有公开构造或反序列化恢复入口。
- 同一 Control 持有唯一配额上下文，重复绑定及不同权限子集共享资源数、并发作业数和累计字节。令牌释放只退并发资源，不退累计字节；时间、期限和配额失败不污染其他实例。准入后真正开始工作及交付时仍需再次检查。
- [普通执行入口](../plugin_runtime/src/package.rs) 与纯 handler／证据计划拒绝 IO 声明包；未实现的 IO 导入不能退回普通 Wasm 执行。

## 验证与范围

共 **117 项测试通过**，以下计数来自实际日志。失败迭代保留，不把旧报告当成本轮复测。

| 范围 | 结果 | 日志 |
| --- | --- | --- |
| 包 IO／旧包／依赖声明 | 33 PASS，其中新 IO 10 项 | [package](../build/road07-package-tests-final.log) |
| Registry／依赖／IO 迁移 | 32 PASS，其中新 IO 9 项 | [registry](../build/road07-registry-final.log) |
| Registry 内部校验 | 2 PASS | [unit](../build/road07-registry-unit.log) |
| 实例 IO／Manager／Pool | 37 PASS，其中新 IO 9 项 | [binding](../build/road07-binding-regression.log) |
| 普通执行拒绝／冻结旧三语言原件 | 13 PASS，其中真实执行门禁 1 项 | [runtime](../build/road07-frozen-runtime.log) |

Windows 迁移及批准发布失败测试使用实际文件共享锁阻止替换，验证失败前后内存与文件原件。IO 绑定测试使用实际 WAT 模块、Registry、Manager 和 HostRuntime 连接，覆盖 foreign 绑定、连接替换、旧摘要／revision、到期、撤权、重启和共享配额。普通执行测试使用会 trap 的模块，验证入口先拒绝、不调用时钟、不消耗 guest fuel。

冻结 C／C++／Rust task、transform、UI 和依赖原件直接运行，没有重编或重新封装；这不是 Flutter 渲染、完整新 SDK 或跨平台资格。独立审查为只读代码复核，未发现本范围新的阻断问题，不冒充独立运行复测。

严格 Clippy 分别检查新包声明、新 Registry、新实例绑定及执行门禁，全部退出码 0：[package](../build/road07-package-clippy.log)、[registry](../build/road07-registry-clippy.log)、[binding](../build/road07-binding-clippy-final.log)、[runtime](../build/road07-runtime-clippy.log)。冻结清单复核为 36 个原件／13 对 Wasm 与包，完整性通过。

工作台宿主与包含 plugin-adapter 的网络组件均完成 Release 配置 cargo check，退出码 0：[host](../build/road07-host-check.log)、[network](../build/road07-network-check.log)。这两项只证明编译集成，没有重建 Windows 主应用或执行新网络服务。

执行入口：

```powershell
cargo test --offline --locked --release --manifest-path core/Cargo.toml --target-dir build/core --test package_io --test plugin_package --test package_dependencies --test dependency_call_package -j 2
cargo test --offline --locked --release --manifest-path core/Cargo.toml --target-dir build/road07-registry --test plugin_registry --test dependency_registry --test plugin_registry_io -j 2
cargo test --offline --locked --release --manifest-path plugin_runtime/Cargo.toml --target-dir build/road07-binding --features packages --test io_binding --test manager --test instance_pool -j 2
cargo test --offline --locked --release --manifest-path plugin_runtime/Cargo.toml --target-dir build/plugin-runtime --features packages --test io_execution_gates --test sdk_frozen_compat --test sdk_frozen_dependency -j 2
```

## 迁移和剩余工作

打开旧 Registry 会写入格式 v2；旧宿主将拒绝它。此迁移不改内容库格式，也不重新批准或恢复运行期引用。本轮只操作测试目录，没有启动主应用或迁移用户实际资料。

仍缺 IO 编解码、guest import、异步作业／背压、路径／origin／监听／路由资源授予、凭据、宿主总量准入、发送前持久意图、Unknown 查询、IO 审计／录制重放及三语言包装。IoLease 只是宿主预算预留，不证明真实副作用或实际字节已经观测；真实 broker 必须分别计量请求与响应，并拒绝超过预留量的交付。

出站 HTTP、入站 API 节点和文件读写继续按完整 NET／NODE 范围推进。当前不能运行声明 IO 的业务插件，也不能以元数据测试替代实际第三方 API 对接。主应用、设备、Web、平台生命周期和渠道均未因本轮通过获得新资格。
