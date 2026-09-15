# ROAD-01 固定源码构建回执：原生节点资格

日期：2026-09-15。结论：**PASS_SCOPED**。已建立原生节点实际构建与源码副本、日志、产物的关联；不是主应用发布、网络运行或设备／渠道资格。

## 实现与证据

[构建工具](../audit/src/build_receipt_main.rs) 复制非忽略工作树到新目录，从副本离线锁定依赖构建，验证源码集合未改变，再写入独立的 [Protobuf 模式](../audit/schemas/build_receipt.proto)／LZ4 回执。当前配方为 network-node-release、plugin-adapter、显式 rustc 主机 target。操作和边界见 [BUILD_RECEIPTS](../docs/BUILD_RECEIPTS.md)。

本次实际复制 **916 个源码文件**；HEAD 为 `0e93e49f56dd2534db5a0cd8fb31e9d5c705dfa7`，工作区包含未提交修改。源码集合摘要为 `5d8ac6586a9e54a9e4f81683908141538d070e3bbbeb64ff758be9f4512983f7`。副本保存的是构建时输入，随后原工作区的文档更新不回写这份副本。

原生节点 `0.1.9-test.51` 针对 `x86_64-pc-windows-msvc` Release 构建成功，Cargo 退出码 0，实际编译耗时日志为 **2m 38s**。回执另记录复制、校验等在内的起止时间。没有运行新节点或将新产物冒充已完成旧运行测试。

- [原回执](../build/road-01-network-capture-001/build-receipt.pb.lz4)
- [派生可读报告](../build/road-01-network-capture-001/summary.md)
- [实际构建日志](../build/road-01-network-capture-001/logs/build.stderr)
- [构建产物](../build/road-01-network-capture-001/artifacts/morrow-api-node.exe)

| 对象 | SHA-256 |
| --- | --- |
| 二进制回执 | `1bf355c72b085887e07767c11d08ccbb46ab2f0951447bc409e4422f38733830` |
| 本次节点产物 | `bee96de534a2723e471e10c70c2985e843298bea1e662c9d3a5a1997e01b71b1` |
| 回执采集器 | `d058aa61dce2481b624c23d14dcd5554698d562819f3949ca91b3a5bac1cbe60` |
| 实际构建 stderr | `4da90be456ca9b65b8b6bd8a233c24141e739adf858d5a6f16ee71dffe165f78` |
| 回执 schema | `1572057e0dc48c86e1af23dccf586fd41ff3918b431639f30fd61440fd16d7e6` |

## 验证

```powershell
cargo test --offline --locked --release --manifest-path audit/Cargo.toml --target-dir build/audit --test build_receipt --test build_receipt_cli
cargo clippy --offline --locked --release --manifest-path audit/Cargo.toml --target-dir build/audit --bin morrow-build-receipt --test build_receipt --test build_receipt_cli -- -D warnings
build/audit/release/morrow-build-receipt.exe capture . build/road-01-network-capture-001
build/audit/release/morrow-build-receipt.exe verify build/road-01-network-capture-001
```

**17/17 通过**：14 项编码／边界测试、3 项实际 CLI/Cargo 进程集成测试；[测试日志](../build/road-01-receipt-tests.log)。严格 Clippy 退出码 0；[检查日志](../build/road-01-receipt-clippy.log)。实际回执在 capture 后独立重新打开，逐项核对源集合、日志、产物及配方，verify 返回 0。

合成 CLI 项目验证真实编译成功／失败、日志被修改、产物被替换、额外源码、已有目录拒绝覆盖及无回执的不完整目录；这些不是产品网络测试。编码检查包含解压上限、畸形字段、重复／未知字段、源路径及排序、语义摘要、时序和矛盾的成功记录。

冻结兼容样本仍为 36 项／13 对原件，完整性检查通过；没有重编或重封旧 guest。只读静态清单在新增 schema 后仍成功生成：[当前观察](../build/road-01-inventory-after-receipts.md)。

## 限制和剩余任务

记录未签名，不证明发布者身份、恶意宿主下的真相或可复现构建；全局 Cargo 配置、缓存和外部工具链并未封闭。没有最终回执的中断目录不能推断构建成功或确证失败。

当前只把节点构建接到实际采集器。主应用、测试运行回执、跨平台目标、APK 合并配置、原生页面资格和渠道仍待接入。应用版本保持 test.50+55；独立节点仍是 test.51 组件，完整 SDK 和 M0–M7 未因本轮通过而完成。
