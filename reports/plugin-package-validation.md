# 插件包验证记录

日期：2026-09-11。平台：Windows x64。版本基线：0.1.9-test.10；运行 Cap’n Proto v6、guest ABI v1、包 schema v1。应用版本与资料库格式未修改。

## 已执行

- `tool/verify_core.ps1 -Web`：格式、Clippy 严格检查、核心默认测试及故障恢复测试、CLI、自检、wasm32 核心编译通过。新增包测试 8 项，包含未知可选字段原字节保留、拒绝损坏／超限／不兼容输入、并发安装、篡改不覆盖与能力上限。
- `tool/verify_plugin_runtime.ps1`：Clippy 全目标／全 feature、运行库 16 项测试、SDK 14 项回归通过；三语言重新编译。新增 4 项运行库测试验证包绑定、限额取交集、限额实际终止和准备阶段拒绝不合法入口／导入。
- 四个实际 `.mplugin` 均经过打包、不可变目录安装、重复安装、重新加载、运行准备及独立连接。测试缺权限拒绝、授权提交、重复请求、另一实例拒绝、新版本不能借用旧连接、撤权、取消回复和停止后操作拒绝。
- 每个示例关闭后重开 SQLite，保留已提交修订 2，事件共 2 条（创建和重命名），完整性检查通过。低层独立消息编码对照与首次提交后取消／trap／fuel 故障测试同时通过。
- C++ 非法回复访问及 128 MiB 分配失败均在 guest 内 trap，零宿主提交。
- `core-web` 携带实际 Web 存储 feature 的 wasm32 编译通过，并更新依赖锁定；此项没有运行浏览器插件执行器。

实际包输出目录：`build/plugin-packages/8c9d07553f44469487e50db15c6abaa7`。完整日志：`build/core-test.10/package-verification.log`、`build/plugin-runtime/package-verification.log`。复现脚本每次创建新目录，不覆盖旧产物；同一源码、工具链和 manifest 的包摘要一致。

| 文件 | 字节 | SHA-256（完整归档） |
| --- | ---: | --- |
| rust-rename.mplugin | 42779 | `4461d1d45d3fdbe6b81cb5f758342a44ce7d2b7dbe484d8c72c912c9115ad0c3` |
| c-rename.mplugin | 42133 | `9bb54c84ffa4768a9b68754ded1d256611f8ed0bdbc61d7abae839b77f0c29f1` |
| cpp-rename.mplugin | 43384 | `a68c022ef28d0c219134544781b687ca4c2844ff68fec75e24c22e25fc9d3dcf` |
| cpp-allocator.mplugin | 44538 | `ca9077a4f6d259b6c0915b97a22cc237aff7abb6a83f0ad8e55f09d9e65b111e` |

模块摘要见 [三语言 Wasm 记录](plugin-sdk-wasm-three-languages.md)，本次重建一致。包 SHA-256 用于完整性和不可变标识，不是作者签名。

## 验证边界

当前只有 Windows Wasmi 原型和合成任务；未完成包启用／更新管理、作者签名、依赖锁定、长期实例、异步调度、共享映射、UI 对接或全平台运行资格。打包／安装只验证容器，不执行代码；运行准备进一步检查 Wasm，初始化资源上限在实例化时生效。核心断开连接阻止新内容操作，但同步原型的取消／fuel 不等于进程或 Worker 强制终止。

第一次整包资格运行中，测试工具在重新授权时误传固定旧时间，被核心单调时钟检查拒绝。已修正为同一个宿主时钟并完整重跑通过；核心时间检查未放宽。

下一步见 [插件包开发流程](../docs/PLUGIN_PACKAGE.md)。未推送或发布 Release。
