# 服务拥有者分段帧验证（2026-09-21）

基于 `3a2a222`，版本仍 `0.1.9-test.52+56`；仅本地开发，未推送/发布。

## 实现

[合同](../docs/PLUGIN_SEGMENTED_OWNER_FRAMES.md)补齐服务期间64–128 KiB完整私有请求：原拥有者中的单个有界暂存、32 KiB有序块、完整长度/SHA-256绑定、一次取件及原业务处理；原64 KiB命令队列与插件ABI不变。客户端使用原task及最终submission，保留总截止、QueryFailure、Unknown与敏感结果的原缓冲寿命。

停止/关闭清除暂存；过期拒绝使用并按合同惰性清除；最终效果未因取消或Abort回滚。允许的本地128 KiB私有请求范围与旧有guest输入/响应/附件上限分开，不宣称任意大消息或流式IO已完成。

## 已验证

| 检查 | 结果 |
| --- | --- |
| workbench_host 全部 lib 测试 | 68通过，无跳过；含新2项缓冲边界/清理和2项原生服务组合 |
| 客户端四文件组合 | 37通过，无跳过；新增最终Unknown用例后，分段/原路由文件单独9通过（较组合新增1项） |
| 实际Dart→Rust Release进程组合 | 4通过，无跳过：大配置、原业务/真实HTTP共存、两种已提交回执丢失 |
| 严格Clippy | workbench_host lib通过 |
| Schema生成一致性 | generate_workbench_client.py --check通过 |
| 构建 | Rust Release宿主和完整Windows Release通过 |

原生服务用100 KiB完整请求修改语言，直接64 KiB命令路径先明确拒绝；分段完成后只增加一次修订，重复完成拒绝，同库重开仍为原值。另一用例拒绝外层直接暂存、嵌套调度/暂存，且原worker回收后无法继续原上传。

真实客户端构造**71,408字节**配置请求，包含128条附件读取范围，经服务期间分段保存。原运行task不变，实际认证HTTP仍返回202；停止、回收、关闭、重新打开原库后，配置修订仍为1，摘要、命名空间、认证引用及全部范围的cardId/attachmentId/kind一致。该配置只是期望状态，保存未启动另一个服务或授予插件新权限。

客户端受控进程覆盖80,000字节业务载荷的完整重组、严格偏移回执、错误回执后Abort、最终完成Unknown保留原submission且只执行一次。原有小帧路由、敏感令牌所有权和关闭路径继续通过。修正测试同步夹具在Windows短暂共享占用期间读取回执的有界等待；没有重试应用请求。

日志位于 `build/frame-staging-*`：host-tests-v2、dart-regression、dart-final、real-native-v3、clippy、windows-release。构建收据见 `build/frame-staging-build-receipt.json`。本轮未重跑窗口输入/视觉验收，构建和真实进程测试不替代它；Dart分析器上轮环境故障没有被本轮结果宣称修复。

## Bridge与审核

DeepSeek Max生成暂存模块，首次返回截断后只另行请求缺失方法；主代理修正不存在的错误类型、元数据错误清除有效槽等问题并执行测试。GLM Max获得8次有界授权，本轮用1次复核失败语义。

GLM建议移除失败通道清理限制未采纳：本地源码中 `_failure` 标记传输/帧顺序失败，QueryFailure并不设置它；继续发送不能证明安全。其余两点是已文档化的双层清理与一次消费语义，不代表重复业务执行。新增最终Unknown测试验证未重发，执行成功不作为语义正确证明。

## 后续顺序

应用服务TLS/证书生命周期与实际用户路径 → Unknown持久核对/因果证据 → 完整文件系统 → C/C++/Rust IO SDK候选及各平台资格。分段帧完成的是应用私有业务范围；guest流式IO、SSE/WebSocket和公共SDK保持开放，不把该子项标成IO-D2b/IO-E2整体完成。
