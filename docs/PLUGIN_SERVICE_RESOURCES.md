# 服务插件允许资源目录 v1

状态：实验性、显式选择的原生服务出站资源子集。此契约不修改冻结的 `io.capnp`、`service.capnp`；三语言公共 SDK 已提供独立资源目录接口，但完整 IO SDK 尚未冻结。

## 准入与交付

插件在包的 `required_features` 中声明 `service-resources-v1`，并声明当前服务 schema、HttpPublish 与 HttpRequest。包解析器拒绝缺少这些前置声明的组合。不认识该 feature 的旧宿主按未知必需特性拒绝包；本轮没有运行旧宿主二进制资格测试。

主应用以界面明确选择的端点引用和预期修订启动有限服务，在原 Manager、实例和 Store 下重新批准。只有声明上述 feature 且有非空选择时，宿主才从实际批准的 HttpRouteSet 生成目录。旧包保留原请求语义；空选择没有目录也没有出站授权。

目录采用独立的 `core/schemas/service_resources.capnp`，包含版本、schema 摘要、所选持久政策及凭据记录的 scope 摘要，以及有序端点表。每项包含精确端点引用、可选凭据引用、允许方法、请求/响应字节上限、超时与响应帧上限。没有凭据值、DPAPI 密文或 TLS 私钥。

宿主将目录的规范编码转换为小写十六进制，注入服务 Invocation 的保留头 `morrow-service-resources-v1`。外部 HTTP 请求中的同名字段无论大小写、重复或被 Connection 提名均先删除，再注入不可变的实际值；`morrow-outbound-scope` 同样由宿主控制。业务代码不能把读取到的任意普通 HTTP 头当成受信宿主上下文。

## 配置服务的统一宿主接线

2026-09-26 新增 `network_node::service_outbound::{SelectedService, PreparedService}`，工作台 `start_service_with_outbound` 已使用这条公共宿主路径。它把原来由调用方分别传递的选择、实时依赖、重放 scope、HTTP router 和 guest 目录封装在一起，减少漏接授权限制的风险。

1. `ServiceEndpointSelection::validate` 拒绝超量、重复、零引用或零修订；空选择合法，表示没有出站能力。
2. `SelectedService::resolve` 在原 Store 解析所选端点及凭据，核对预期修订，同时给 `ResolvedService` 附加这些实时依赖并计算完整策略 scope。记录读取不打开凭据或 socket。
3. `capabilities` 给出绑定所需的能力集合；宿主仍须单独取得 Registry、实例及有限运行批准。`approve` 使用显式可信凭据提供者，Windows 工作台使用 `approve_windows` 调用原 DPAPI 路径。未选择的资源不会因已保存或已在 Registry 批准而自动接入。
4. `PreparedService::attach` 核对原 worker 与配置服务授权，生成 `ServiceHost` 和对应 `ConfiguredService`。随后由宿主 `bind_configured` 真正监听。接错 worker 或参数失效时返回原 worker，调用者负责停止并实际回收，不能重新打开 Store 冒充原 owner。

字段保持私有，目录仅从实际批准的端点集合生成，并受 guest 必需 feature 控制。资源更新一经原 worker 接受就撤销依赖它的服务授权，因此正常响应、缓存和历史查询同时受限；更新未选资源不影响当前服务。重开必须重新批准，端点或凭据修订改变会使原请求 scope 冲突，不允许复用旧键自动重发。低层 `ServiceHost` 构造接口仍保留给需要自定义宿主的调用方。

原三语言服务出站包经过这条配置路径的证据见 [统一接线报告](../reports/plugin-configured-service-2026-09-26.md)。这是原生适配器资格，不替代 Windows／Flutter 实际工作台和 DPAPI/TLS 生命周期验收。

## 边界与重放

- 目录最多 8 项，二进制总长最多 4096 字节，对应头值最多 8192 字节；各字段上限与总长上限同时成立，不保证所有字段同时达到最大值仍可编码。八项、全部七种方法及每项 64 字节持久凭据引用的组合已通过完整服务帧往返。
- 端点引用为非零、64 字符的小写十六进制；端点与方法严格排序、无重复。凭据引用为空或最多 256 字节且不含 ASCII 控制字符。当前持久提供者使用 64 字符引用。
- 当前目录数值范围是请求/响应最多 64 MiB、超时最多 300000 ms、响应帧最多 128 KiB。实际条目来自已批准适配器，仍受更小的 guest IO/端点政策约束；目录不会扩大这些限额。
- 解码检查版本、摘要、遍历/嵌套预算、全部字段与帧总长，并要求重新编码后字节完全一致。未知字段、尾随数据、非规范别名或重复保留头拒绝。
- 元数据不是权限。每次导入继续核对原拥有者、原实例、活跃批准与撤销/期限。保留目录或稳定引用不能复活旧授权。
- 目录随完整 Invocation 纳入请求身份；同记录重新批准后可读取原结果，政策/凭据变化导致旧键冲突。目录与 outbound scope 不匹配时，ServiceHost 构造失败并归还原 worker。

## 已验与后续

[本轮证据](../reports/service-resource-directory-2026-09-21.md)覆盖实际 Rust/Wasm 插件从目录发现引用、真实 HTTP、伪造头/请求体、旧包与空选择、重放、撤权和拥有者回收。测试插件复用 core codec，只用于集成验证，不能作为独立公共 SDK 的资格证据。

完整 Windows 窗口端点选择→资源发现→出站→运行中撤销/停止的限定验收已通过，见[窗口报告](../reports/service-resource-window-2026-09-21.md)。现已[按资源依赖撤销](../reports/resource-scoped-revocation-2026-09-21.md)：未选资源写入保留原服务，已选端点/凭据失效仍回收整个依赖服务并拒绝旧缓存。这些是 2026-09-21 的历史范围；当前三语言公共 SDK 与配置服务原包进度见上节和 [系统状态](PLUGIN_SYSTEM_STATUS.md)。完整业务 Unknown 核对、文件系统和跨平台资格仍开放。
