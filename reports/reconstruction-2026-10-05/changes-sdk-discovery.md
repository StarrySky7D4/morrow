# C03 Changes metadata SDK discovery

2026-10-05。在 [C02 Windows 复验](windows-sdk-revalidation.md) 已完成并独立封存后，继续一个有界 SDK 阶段。基线仍为云端 `468ef2e` 加已复验的 C02 修复；未提交、推送、发布或运行 CI。

## 实现

在 `morrow.channel.v1` profile 中添加可选的 `payload_discovery`。它有独立 `schema_version: 1`，描述 `morrow.changes-metadata.v1` 的规范原字节摘要、版本、150／662／256／32 字节限额、最多 32 个卡片、Events-only 非双工有限窗口及所需批准和绑定。

`runtime_static_preparation_supported` 只表示当前静态 package 校验。`native_source_adapter_compiled` 使用与原 runtime 模块一致的原生编译条件，不代表 protected owner 或平台产品资格。`authority: none`、生产绑定 false、空工作台 route 和禁止自动运行均明确输出；发现信息不产生 grant。

旧 base／channel profile 原字段、SDK envelope version、四类 IO extension discovery、旧 wire／SDK327／冻结57、Core／runtime／批准实现均保持不变。新消费者在字段缺省时接受旧描述，在字段存在时严格核对已知协议及限额；未知契约、伪造权限／生产 route、数字类型伪装和扩大限额均拒绝。

接口说明见 [Changes metadata SDK 接入](../../docs/PLUGIN_CHANGES_METADATA_SDK.md)。生产 binding、长期 watch／retention／完整 cursor 恢复继续 OPEN。

## 本轮 Windows 验证

| 验证 | 实际结果 | 边界 |
|---|---:|---|
| 完整相关 Python 回归 | 184 个方法：182 通过、2 POSIX skip | 原 C02 的 178 个方法均保留，新增 6 个方法；skip 不计通过 |
| discovery 原生 unit | 9 通过 | 原 7 加新增 2；209 个过滤方法未运行 |
| preflight 原生 integration | 8 通过 | 原 7 加新增 1，失败／忽略／过滤为 0；Unix-only 方法未运行 |
| 新 Windows host | Release 构建退出 0 | Rust 1.95.0，locked／offline；真实执行文件另有摘要 |
| 兼容与静态 CLI matrix | 主组 15 个、补充 6 个预期结果 | 命令／解析判定另记，不能当作 21 个额外业务测试方法 |

实际 C02 host × C03 host 与 C02 已修复消费者 × C03 消费者四种组合均通过。删除且仅删除新 `channel.payload_discovery` 后，新旧真实 descriptor 的完整 JSON 值相同；旧 channel feature 列表与四类扩展记录没有变化。

字面云端 `468ef2e` Python 消费者存在 C02 已复现的 Windows 文件身份错误，其旧 CLI 没有本轮 Windows PASS 资格。该版本只另做原解析器的 descriptor 兼容检查，不能将绕过进程读取的解析成功宣称为旧 CLI 成功。

独立 source-only 新诊断包在源码目录外执行。三种现有 NEW SDK package 对 C02／C03 host 分别静态准备；合成 changes 包带真实旧 `channel.call` import 与会 trap 的 entry，只做 prepare，结果明确 `guest_executed: false`。Events 与 ByteStream 的无效组合以及未知 future feature 均返回结构化拒绝。

实际 legacy `20669f67` Windows host 的审计仅观察到 `--sdk-capabilities`；消费者拒绝新增 preflight，没有将新诊断 flag 发给旧宿主。准备状态不代表 source approval、安装、预算运行资格或业务效果。

每条最终命令的相关源码、工具、实际 host、消费者与分发字节前后恒同；C03 恰好修改四个实现／测试文件，旧 SDK327／冻结57及保留的旧 host、旧消费者均恒同。C02 和 C03 的来源与结果分开封存，历史失败保持原样。

## 仍未运行与后续

protected owner／真实数据库／DPAPI／账户、普通 Windows 桌面 token、GUI／catalog 生产批准链、CI／外部网络、其他平台及本 C03 的 guest 运行／预算适配没有新增资格。C02 的真实 guest 资格保持自身来源，不迁移成 C03 全产品资格。SDK 仍未冻结。

后续优先：生产 channel 的真实 owner／catalog 验收、独立 Rust／C／C++ typed WebSocket payload helpers、文件目录／blob transfer、异步组合及跨平台矩阵。上述工作需要独立契约与证据，不能凭 discovery 或当前方法数关闭整个 SDK 门槛。
