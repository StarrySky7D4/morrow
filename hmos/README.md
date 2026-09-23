# Morrow HMOS

Rust + ArkUI 的鸿蒙迁移工程，独立保存在本目录。当前交付 **0.1.0-hmos-dev.3 开发预览**，尚未与 Flutter 功能等价，不能替代正式资料库。

已提供实际 HAP、Rust OHOS 双架构构建、ArkTS 界面、ArkUI NDK 原生外观预览、N-API 异步桥与共享核心事务。已在 Pura X View / HarmonyOS API 26 x86_64 模拟器安装、启动、建卡保存；另有设备侧 Rust 自检。

## UI 源码对齐

已按活跃 Flutter 的 main.dart / appearance.dart 补齐紫灰玻璃工作台、专用分类卡片、外观与材质子页、色盘、字体、日常清单、音乐空状态和独立编辑弹窗，并实际渲染源码参照图与鸿蒙截图比对。详见 [源码对应和验证](docs/UI_DESIGN_DEV3.md)，可查看 [并排截图](reports/ui-source/compare.html)。这不是完整 UI 等价声明。

## 使用与构建

使用 DevEco Studio 打开本目录。默认包名 `dev.morrow.hmos`，不覆盖 Flutter 应用。启动后打开独立的本地试验工作区。卡片保存于应用沙箱的 `hmos-development.sqlite`，不读取现有桌面资料。

```powershell
# 在本目录执行；首次 ohpm 依赖安装可用 devecocli build 或 Studio 同步。
./scripts/build-rust.ps1 -Test
./scripts/build-rust.ps1 -Abi arm64-v8a
./scripts/build-rust.ps1 -Abi x86_64 -Runner
./scripts/build-hap.ps1
./scripts/check-reference.ps1
```

环境：Rust 标准库 `aarch64-unknown-linux-ohos`、`x86_64-unknown-linux-ohos`；API 26 SDK；Cap'n Proto 1.4.0。脚本提供本机已验证路径，`build-rust.ps1 -Sdk` 与 `build-hap.ps1 -Studio` 可覆盖 SDK/Studio 位置。Cap'n Proto 可执行文件须在 PATH，脚本也加入本机已部署目录。

HAP 输出：`entry/build/default/outputs/default/entry-default-unsigned.hap`。当前模拟器接受该调试安装；**不是已签名发布包，也不是 ARM64 真机验收**。构建脚本关闭本工程 hvigor daemon，避开当前全局 daemon 锁问题，不删除全局缓存或停止其他构建。

## 已接通范围

- 卡片标题、正文、假设、结论，新建和修改；类别、阶段、收藏、搜索和删除视图。
- V2 TaskId 待办新增、独立勾选和移除；同名任务不会联动。阶段修改保留既有独立语义。
- 直接调用现有 `cards_v2` / `tasks_v2`，通过 `morrow-core::HostRuntime` 的对象授权、版本化 CAS 与原操作幂等事务保存。
- 删除后仅允许 **8 秒内撤销**，沿用共享核心的规则，不提供永久恢复承诺。
- `u64` 修订和时间戳跨 ArkTS 边界使用十进制字符串；完整源记录随修改请求绑定，不用界面投影覆盖正式内容。
- 数据库操作在 N-API worker 执行；ArkTS 串行提交。回复未确认保留同一序列化请求；正式提交后回读错误仍保留核对状态。编辑输入不会被迟到的保存回复覆盖。

## 尚未完成的关键边界

当前使用原核心的 **未封存试验 Store**，并非生产 `Workbench` 的安全降级。现有正式宿主 `storage.rs` 在非 Windows 平台明确拒绝打开；此检查保持原样。Harmony HUKS 密钥保护、库身份、单所有者租约、审计封存、恢复与备份还需适配并单独验收。

试验库限定 256 张卡片，保留核心默认事件预算（1024 条 / 64 MiB），没有绕过队列上限或静默清除历史。达到容量会停止写入。系统自动备份关闭，避免普通文件备份冒充 SQLite 一致性快照。

当前编辑值、选中卡片、在途请求只在进程内保留；**未接入原宿主持久草稿与跨进程 Unknown 恢复**。已成功提交的卡片可重启读回。试验版不得保存唯一副本。附件、插件运行/管理、网络服务、TLS、媒体/歌词、字体文件导入、完整九语文案、宽屏设备验证和备份恢复仍待补齐。外观与日常清单现已独立保存到本机 Preferences，详见 [功能与复用清单](docs/PARITY.md)。

## 跟随主任务

功能参照：[检查项目并完成更名](codex://threads/01a085bd-7a94-7f93-8a1f-1ecf417f5ee3)。环境参照：[安装 Deveco CLI](codex://threads/01a0cc0a-76a5-7973-a6c8-eb7566100932)。

实际参照目录为 `../build/io-safety-refactor`，HEAD `ddd9cc8eec224af51f4e58654c9b297332147d96`，版本 `0.1.9-test.54+58`，包含未提交增量。没有以当前根目录旧 HEAD 代替它。

`shared/reference.json` 固定 228 个共享文件的 SHA-256，包括 V2 新增源码。`check-reference.ps1` 比较真实工作树与快照并写入差异报告；不自动覆盖正在使用的源码。后续同步必须审查差异、更新功能清单、重跑 Rust/HAP/设备验证。没有创建定时任务或向原任务发送消息。

共享源码保留原 AGPL-3.0-only 许可与 `shared/LICENSE`；快照不是另起一套业务规则。ArkTS/C++/Rust 适配代码属于本目录；原 Flutter 与活跃工作树未修改。

