# 独立 channel payload 源码分发

新的 `morrow-channel-payload-sdk-source-v1` profile 包含不变的原 SDK 库、WebSocket／SSE 两个独立类型库、C11／C++17 headers 和六个示例源码。它不扩大旧 `morrow-sdk-source-distribution-v1` 或 sdk.lock.toml 的合同，SDK327／冻结57保持原字节。当前是本地实验源码包，未公开发布或完成 SDK 冻结。

## 导出与校验

在可信仓库根目录执行：

```text
python -B tool/export_channel_payload_sdk.py ../channel-sdk.zip
python -B tool/verify_channel_payload_sdk.py verify-zip ../channel-sdk.zip
python -B tool/verify_channel_payload_sdk.py extract-zip ../channel-sdk.zip ../fresh-sdk
python -B ../fresh-sdk/tool/verify_channel_payload_sdk.py verify-directory ../fresh-sdk
```

Python 3.11+，导出／解包目标必须不存在。源码闭包为 90 文件，另有 `CHANNEL_PAYLOAD_SDK_MANIFEST.json`；Core、runtime、host、compiler、cache、数据库、凭据及预编译客体不在包内。保留 sdk/ 与 extensions/ 的相对关系，编译输出放在源码目录之外。

导出前后核对 SDK 与 Core 的唯一原 schema／版本、两个 payload 与原生 raw schema；原 SDK lock 使用固定字节身份，扩展的 13 registry tuple、依赖边和 path package 逐项核对。独立验证器检查精确 Cargo 图及 TOML 类型、大小、普通文件／重解析点、ZIP／路径、重复／大小写冲突与摘要，拒绝现有目标覆写。

校验只证明有限字节清单一致性及观察到的输入图；清单未签名，不认证第三方来源、不审计任意 Rust/build.rs 语义，不构成构建沙箱或宿主批准。普通 trusted build 是代码执行。修改文档并自行重新生成清单可能仍是字节一致的包，不能据此推导可信来源。

## 编译与宿主

包内 README 提供两库、Rust guest 与 C／C++支持源的编译方式。Rust 需要已安装的 toolchain、Cap’n Proto compiler、离线依赖和 `wasm32-unknown-unknown`；C／C++另需 Clang、WASI sysroot、C支持对象及 C++ noeh runtime。当前 Windows 使用 Rust1.95／Capnp1.4／Clang WASI 路线实际完成项目外构建与执行，见 [限定资格](../reports/reconstruction-2026-10-05/channel-payload-distribution.md)。

原 SDK-only 项目 CLI、仓库 guest 准备工具和 Core 包工具未纳入这个闭包；不得照搬旧 source-only CLI 的 profile 或声称新包已提供独立 host 封装。库和示例可以独立编译，实际安装／执行仍需要可信宿主的包、目录、当前 grant、deadline、预算和生命周期。

实际消费新的编译产物已另用可信宿主工具包装和运行，结果与编译清单分开。Six Wasm、语言组合、用例和四个运行方法分别计数；四方法属于既有网络100的再验证，不能记作104。生产批准、普通用户 token、账户／TLS、其它平台及完整第三方产品链仍未验收。
