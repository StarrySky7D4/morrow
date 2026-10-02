# Stage 12：channel SDK 交付工具检查

日期：2026-10-02 UTC。此阶段是在 test58 检查点之后新增的有界工具修正。

基线提交：`03606019fc4f9e279ec21c7276b83e6ff71865b8`，应用版本仍为 `0.1.9-test.58+62`。本阶段未改变版本、schema、SDK/core/runtime 实现、旧 guest、不可变包或源码锁，也未执行提交、推送或部署。

## 修正范围

- `tool/morrow_plugin.py` 原有 `sdk_channel` 已由 channel 的 `new` 和共享工程预检调用；并非完全没有 channel 检查。原检查使用原始 schema 字节和未锚定的首个 VERSION 匹配，缺少重复、类型、范围和真实顶层声明约束
- 现在 channel schema 仅归一化 CRLF；额外 schema 内容和裸 CR 不属于同一契约
- host 和所选外部 SDK 必须各有一个精确、公开、顶层的 `pub const VERSION: u32 = 数字;`，数值必须匹配并适合 u32。超过十位的数字在整数转换前拒绝；匹配的 `4294967295` 只作源码检查控制，不代表新协议已实现
- 小型词法遮罩排除注释及普通/raw 字符串，随后检查分隔符深度。只有嵌套 module、function 或未调用 macro 中的 VERSION 不能代替顶层声明；重复的错误类型或私有 VERSION 也不能被首个有效匹配掩盖。此检查不解析/编译完整 Rust，也不解释宏或证明 SDK 行为
- Rust channel codec/schema 在三种语言路径上均被检查，因为 C/C++ 的构建同样使用 Rust codec。C channel 头文件只在 C/C++ 路径要求，C++ 包装只在 C++ 路径要求；不把 C 的 `MP_CHANNEL_VERSION` 描述符 ABI 与 Rust wire VERSION 混为一谈，也没有新增头文件兼容资格声明
- `tool/sync_plugin_sdk_contracts.py` 的明确 inventory 加入 `channel.capnp`。channel 的 `--check` 采用 CRLF-only 比较；实际写入同步仅在一次性复制树中执行，未修补仓库内契约
- `tool/inspect_build_inventory.py` 新增 channel schema/wire 观察，镜像数从 8 到 9，协议版本数从 7 到 8。明确记录 channel 为 u32，其他已列协议仍为 u16；旧 u16 范围及零填充字面量行为保留。这不是所有公开协议的完整清单，mutation 的历史 inventory 范围没有扩大

## CLI 边界

channel 的 `new`、`validate PROJECT`、`build`、`pack` 和 `lock-sdk` 都在编译器/宿主工具查询、工程输出或锁写入前执行检查。工程路径从 `plugin.toml` 取得 kind，不能依赖不存在的 `args.kind`。

`check PACKAGE` 是独立的宿主 package-preparation 路径，既无工程 kind，也不把 SDK 当作输入；其原行为保持不变。普通非-channel 外部 SDK 没有新增 channel 支持要求，缺少所有 channel 文件的旧普通 SDK 仍可生成并预检三语言 transform 项目。

声明检查不运行插件、不授予权限、不证明源代码行为或平台兼容性。生成 inventory 的 NOT_RUN、旧二进制未执行、authority 和历史报告边界保持不变。

## RED / GREEN 证据

先捕获三份原工具的精确源码，再以一次性复制的真实 SDK/core 文件运行新增控制。最终扩展后的同一批测试又针对这些捕获字节重放，未改写工作树中的工具。

| 最终 focused 测试 | 原工具 RED | 修正后 fresh GREEN |
| --- | --- | --- |
| channel SDK 工具准入 | 17 个方法，41 个 failure 记录、2 个 error 记录 | 17/17 |
| 契约 sync CLI | 11 个方法，3 个 failure 记录 | 11/11 |
| source-only inventory | 33 个方法，36 个 failure 记录、1 个 error 记录 | 33/33 |

failure 记录包含失败子测试，不能当作独立方法总数或通过率。RED 中编译器查询及锁写入被测试哨兵拦截，没有实际编译；另两个准入 error 是原工具对 Rust-only channel 头文件路径、CRLF schema 的拒绝。inventory error 是缺失 channel 观察的控制结果。

真正新增的拒绝控制包括重复/混型/私有声明、注释或字符串伪声明、macro/module shadow、匹配的 u32 溢出、长数字，以及 sync 对 channel-only 漂移/缺失/裸 CR 的识别。缺失、单独错误类型、表达式等若干原本已拒绝的情形保留为回归控制，不声称都是新修复。

其他 fresh scoped GREEN：既有 plugin project/preflight/source-lock 测试 62/62，channel project 6/6，profiles 8/8。失败的 channel `lock-sdk` 和 `lock-sdk --update` 保持一次性项目中真实 62 项源码锁的原字节，且不创建 build/dist。

完整 Python discovery 在 Linux 上为 **PLATFORM_LIMITED / RED**：238 个方法，2 个失败、1 个导入 error。两项失败由实际 Windows 检查提前拒绝；另一个依赖 `msvcrt`。未修改 Windows 所有者代码、guard、skip 或导入以制造通过；此结果与 focused GREEN 分开记录。

仓库实际 `sync --check` 和 source-only inventory CLI 通过。固定基线 verifier 核对 36 个 pinned 文件、13 对原 Wasm/package，未重新构建或打包旧 guest。8 个既有源码锁的 SHA-256 全部保持不变；工具及测试最终 held hashes 复核通过，`git diff --check` 通过。

## Held 源码指纹

| 文件 | SHA-256 |
| --- | --- |
| `tool/morrow_plugin.py` | `ac26ca6da3f16d8442c5871ae41d9c455d3a2893f8293007b61567816c3503f7` |
| `tool/sync_plugin_sdk_contracts.py` | `0040aec0c652882166d54442354b9b54c0f23096819992173ce40cf8aba45108` |
| `tool/inspect_build_inventory.py` | `c54ab3a9a6b04254eea0a0ab6fcd9e105174d778f6e9b777a7c91a9387830eae` |
| `tool/tests/test_channel_sdk_tooling.py` | `daeaaa729afd8afad5767cbdaac918489fe0b882a2ff917b8181a58c389b0687` |
| `tool/tests/test_sdk_contract_sync.py` | `88836900c52d1729fcbaf4d310630713daea4b86df741b42178160626e074596` |
| `tool/tests/test_build_inventory.py` | `98a4f5e20122eb8ac7201a28bffc44214b9143be8452bc6d2722430a27156dcd` |

命令、原工具复制件、最终 RED/GREEN、平台受限 aggregate、源码 inventory 和 source/log SHA-256 在仓库外的 stage12 验证记录中保存；没有二进制、媒体或凭据。只有此源码说明及六个工具/测试文件属于本阶段提交候选。独立复查、提交及 GitHub/Drive/Library 同步由后续协调步骤完成，本记录不提前声称已发布。

## 未扩大资格

本阶段没有新增 channel 运行、guest 执行、兼容资格或完整 Linux 产品验收。Workbench 绑定、网络/Cloud 适配、Linux 受保护实际打开对象及宿主/GTK 生命周期、Windows 原生退出验证仍依照 test58 检查点记录，不能由工具检查的 GREEN 推导为完成。
