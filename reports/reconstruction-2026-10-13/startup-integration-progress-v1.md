## 启动隔离源码与验证准备更新（2026-10-13）

Dart 候选 r002 修正了正常关闭与失败启动收尾共用标记的问题：只有原通道明确请求正常关闭后，才允许接受 Released 与实际退出 0。原有 15 项测试保持原字节，另增 1 项“未请求正常关闭却收到 Released”的反例；共 16 项测试均为 `NOT_RUN`，并未据此宣称缺陷已通过运行验证。

Rust 候选 r002 保留此前隔离、心跳和账本改动，删除 binary 中三行遗留模块声明，由 library 持有 supervisor 实现。这是源码层最小修正；完整 crate 的编译、测试和 Rust／Dart 联合运行仍为 `NOT_RUN`。此前 Stage355 编译通过的是 Stage349 基线，不覆盖本次候选。

Stage359 的 A／B 两套隔离测试数据准备已分别退出 0，各生成 667 个源码文件及 2 个配置文件；未运行 Dart、Flutter 或原生程序。A 计划仅运行旧 channel 的新增反例，B 计划运行修正版 16 项，计划结果不能算作实际测试结果。

历史 Stage353 九项合成回归、Stage354 五项原逻辑摘取测试、Stage355 基线编译及产物核验的限定范围不变；Stage341 仍为 `CLOSED_NOT_ACCEPTED`，旧 sealed Unknown 不重放。SDK26／G04、namespace 与祖先目录持续 reparse 防护仍 OPEN，`release_eligible=false`。

下一步：执行 A／B 对照并核对原始退出和机器测试事件，随后编译完整 Rust 候选、验证联合运行与 Windows 原资源闭合。本次仅同步开发分支的源码候选和进度，生产入口、应用版本与依赖锁不变，不创建 Release。

见[Dart 候选 r002](source-candidates/startup-quarantine-dart-r002/README.md)、[Rust 候选 r002](source-candidates/startup-quarantine-runtime-r002/README.md)和[机器可读进度](startup-integration-progress-v1.json)。
