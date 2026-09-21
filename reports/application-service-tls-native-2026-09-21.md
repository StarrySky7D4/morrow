# 应用服务 TLS 原生启动验证

日期：2026-09-21。基线 `05c38c1`，分支 `codex/io-safety-refactor`，版本 `0.1.9-test.52+56`。

## 结果

可信 Rust 应用入口现可通过明确选择的证书/私钥启动已有 TLS 监听器。准入严格匹配原 publication，启动前重新检查所选文件及证书摘要，活动身份固定在内存中。原有无 TLS 入口、原拥有者命令、停止回收及持久请求记录流程保留。

本阶段没有修改私有 Cap'n Proto 协议或 Flutter UI，因此用户尚不能从现有运行面板选择 TLS 身份。契约与下一步见 [TLS 接入](../docs/PLUGIN_SERVICE_TLS.md)。

## 验证

在本机 Windows 执行：

- `cargo test --locked --manifest-path workbench_host/Cargo.toml --lib --target-dir ../integrate-track-a/build/io-codec`：78 passed，0 failed，0 ignored。设置三个实际 Wasm 路径，未以缺失环境变量跳过 guest 测试。日志 `build/tls-native-full-tests.log`。
- 随后增强 TLS 主测试，增加运行时原拥有者设置保存及原库重开断言；TLS 定向组再次执行：10 passed，0 failed，68 filtered out。日志 `build/tls-native-final-tests.log`。不是额外累计 10 个独立测试。
- `cargo clippy --locked --manifest-path workbench_host/Cargo.toml --lib --target-dir build/tls-clippy -- -D warnings`：通过。日志 `build/tls-native-clippy.log`。
- 既有 lib test 的未使用 `prepare_write` 警告仍存在；未将 lib Clippy 通过描述为所有 target 无警告。

七项加载器测试覆盖：真实 PEM 配对、摘要与重新选择、不同密钥拒绝、空文件/超限/目录/相对路径、精确大小边界但非法 PEM 拒绝、私钥损坏/删除后重读、Windows UNC/设备/ADS/父目录回退，以及真实叶/父目录 symlink 拒绝。

三项应用测试覆盖：

1. 原应用批准 TLS 后，真实 TCP/TLS 客户端执行认证请求，实际 Wasm handler 返回 202。错误主机名、不受信任根及错误 bearer 被拒绝。运行中替换磁盘证书仍只接受旧身份；显式停止、重新检查并启动后只接受新身份，同持久请求键仍能得到结果。运行期间通过原 owner 保存语言设置，回收时原绑定保持，重开同库核对配置摘要/修订和保存值。
2. HTTP publication 配 TLS、TLS publication 不配 TLS、检查后证书替换均在拥有者移交前拒绝；修正后相同 submission 仍可正常启动。
3. 保持未完成握手的 TCP 客户端时停止服务，监听和 worker 实际结束，连接关闭，原拥有者恢复。该测试不声称在 TLS 库内部某一指令位置实现确定性暂停。

测试使用合成证书与 loopback，不读取用户真实证书/私钥，不开放公网监听。不代表 Flutter 窗口验收、Linux/移动平台或公网证书信任验收。本阶段未重建完整 Windows 应用、推送或发布。

## SubagentBridge 更新与审核

实际部署 revision 为 `sessions-2026-09-21`；本会话无 Bridge MCP 工具，通过部署 CLI 使用更新功能。GLM-5.3-flash/max 编写加载器候选和测试；加密会话 `session_task_05efee78f3e69a6f3b3f1fc3` 连续两轮完成，revision 0→1→2，第二轮供应商报告缓存 hit 256、miss 1048。缓存命中不等同于预算抵扣。

主代理审核发现并纠正候选中的 Unix/Windows 条件错误、canonicalize 抹除链接、错误详情直接传播、PEM/DER 混用、错误 API/路径类型及密钥拒绝断言。候选成功返回不作为语义正确证据。

DeepSeek-flash/max 独立审核真实完成，任务 `task_c16610d4c88c6c719c8046d7`。其关于密钥配对可能缺失的意见已通过实际不匹配密钥测试排除；叶/父目录并发替换属于已记录的不提供敌对文件系统隔离的限制。没有自动重发未知结果。原始结果保存在本地 build JSON，不包含用户私钥。

本地证据：`build/bridge-tls-loader-tests.json`、`build/bridge-tls-loader-tests-v2.json`、`build/bridge-tls-independent-review.json`。
