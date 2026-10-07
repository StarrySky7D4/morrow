# 草稿接续基础能力分支检查点

2026-10-07。基于已发布 `eeca59f85227939449b72c45dae28d90545fcfd8`，仅交付独立 `hmos/`，目标分支 `codex/ArkTsUI`；不合入 main。应用身份仍 **0.1.0-hmos-dev.19 /1000019**，不是 dev20 发行。

新增独立开发库的 `draft_fork` / `draft_fork_retire`：新身份先持久保存完整 raw 文字、UTF16 选区/候选和父草稿已确认附件的明确子集，再以准确父历史条件清理。保留原 source 全字节及业务 CAS 冲突，不重新激活失效身份，不借用保护存储的捕获证明。ArkTS 协调器冻结原请求，暂停父写入时仍允许完整输入 capture，验证首回执后才能建立子 writer，未知结果只显式重试同原请求。

**目前 Index 未调用此 fork 协调器，界面清理后的迟到输入接续仍 OPEN。** 本次是可复查的后端/协调层源码及构建检查点，不能宣称界面清理完成、业务来源重基或较晚输入已跨重启保留。协议和具体边界见 [接续设计](continuation-design.md)、[Rust 审计](native-fork-audit.md)、[ETS 审计](editor-draft-fork-audit.md)。

## Fresh 验证

| 范围 | 结果 | 原证据 |
| --- | --- | --- |
| 默认完整 Rust | **144 PASS /0 FAIL /7 条件 ignored**：library141，attachment-check3；包含18项新增普通 fork 用例 | [默认日志](native-default-tests.log) |
| 实际 Store 中断 | 显式 fault feature：**1项 PASS，14个 fork/retire 子进程中断场景全部通过**，核对原操作、reopen、同请求重试及附件19字节/SHA | [逐场景日志](native-fork-store-crash-tests.log) |
| 全部实际 ETS/工具/Index 方法模型 | **695 PASS /0 FAIL /0 SKIP**，9080.6508ms；新增 fork17，原普通草稿25 | [完整日志](models-final-tests.log)、[fork日志](fork-model-tests.log)、[原草稿回归](normal-draft-regression.log) |
| OHOS ARM64 native | release **SUCCESS /9.71s** | [构建日志](native-arm64-v8a-build.log) |
| OHOS x64 native | release **SUCCESS /5.20s** | [构建日志](native-x86_64-build.log) |
| 新协调器独立 API26 编译 | **SUCCESS /8.827s**；真实 import、构造及公开 API 引用，309份源拷贝/5份显式 wrapper 构建前后核对 | [独立 SDK 构建](fork-sdk-build.log)、[after核对](fork-sdk-verify-after.json)、[拷贝清单](fork-sdk-source-copy-manifest.json) |
| API26 产品 HAP | **SUCCESS /9.685s**，采用本轮新双 ABI 库，Index 既有输入/待办继续编译 | [构建日志](hap-build.log) |
| 构建输入 | **318项**构建前冻结与构建后字节一致，disk 核对 PASS；staged 核对另记最终日志 | [冻结清单](build-inputs.json)、[disk核对](build-manifest-disk.log) |
| 原生源和采用 | **264项**当前原生源/脚本冻结，构建及采用后复核，发现的源文件均在清单；双库完整字节与生产 staticlib 匹配 | [原生构建清单](native-build-inputs.json)、[采用核对](native-adoption-check.json) |
| 包内原生库 | **4/4 PASS**，两 ABI 的 `libmorrow.so`、`libc++_shared.so` 与 SDK stripped 输出全字节一致 | [包内核对](native-package-check.json) |

ARM64 staticlib **55,587,274字节**，SHA256 `F33B4EDB1AB59AA75953F1A293BE328CECAA074580CE37739DE0F17063297836`；x64 **53,994,540字节**，SHA256 `C76C25B12EAFF38437386270032F37445EF6CF8AE8F1DC86F1C2D03E49F137BB`。归档 `.build/editor-input-native/dev20-fork-checkpoint/{abi}/libmorrow_hmos.a`，保留旧归档身份。

本轮未签名 HAP **28,675,122字节**，SHA256 **2C6D37BFAD1E19C4026B5708FD50BBEA68C671687E2594C0EA559026E46D1176**；归档 `.build/artifacts/dev20-fork-checkpoint/entry-default-unsigned.hap`。**未安装**。全局 [build-manifest.json](../../../build-manifest.json) 指向此包；Git 交付源码和证明，不提交本地构建缓存或另行发布安装包。

独立 SDK smoke 使用另外的包名 `dev.morrow.hmos.draftforksdk` 和生成入口，目的是让未被 Index 导入的新协调器接受实际 ArkTS 编译检查。其未签名 HAP **25,719,630字节 /7560FEFB254E3875C261E3EEDE517E9A2BCF860D3AAC47FBABF4425B40BE3E56** 未安装、未运行，不作为产品或 native fork 运行证据；不能与本轮产品 HAP 混用。可复跑 preparation/build/verify 脚本随报告保存，旧 harness 和生产源未改。

本轮144项默认 Rust 不包含被忽略的实际 Flutter/Dart 条件比较；过去123项、Flutter对照、旧包/设备结果保留原身份，不记为本轮重复运行。14个受控进程中断不证明掉电、磁盘损坏、跨进程原子交接或保护存储。模型使用受控 provider/时间、抽取实际方法，不能代替 SDK 事件、原生 Store 或设备 qualification。早期 fixture 失败日志保留，最终验证以同冻结字节的上述成功日志为准。

## 设备和开放范围

设备仍安装此前 `eeca59f8` UI 集成 HAP **47690159…**，其安装、首页及空待办区观察见 [单独设备记录](../device-integration/validation.md)。这不是本轮 **2C6D37BF…** 包，也不是 fork 的运行证据；版本名相同不能把两包结果混用。

两个授权 journal 事务依次执行，不保证父子多对象原子提交。首 ACK 前的原 fork wire 和较新输入仍主要在模型内存，尚无新的跨重启 pending intent 资格。Index writer 接线、callback/owner barrier、最终弃稿流程、准确业务回执来源接续、legacy todos 与 V2 TaskId reconciliation、全篇选择、真实输入法、多行输入/拖动/保存重启设备闭环均 OPEN。HUKS、签名、ARM64 真机、保护存储和完整 Flutter/Windows 功能对齐仍 OPEN。
