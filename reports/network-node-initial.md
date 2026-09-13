# 双向网络与 API 节点：首次原生实现

日期：2026-09-14。结果：**PASS_SCOPED**。新开发组件 `morrow-network-node 0.1.9-test.51`，主应用仍 `0.1.9-test.50+55`；未推送、未发布新应用或 Release。第一方 AGPL-3.0-only，新增 crate 禁止 unsafe。未改变现有公共 guest schema、SDK 源码或冻结二进制。

## 实际交付

新增独立原生库、可执行 `morrow-api-node.exe`、真实客户端/服务端/TLS/插件适配测试及可重复 CLI 资格脚本。客户端固定 origin 与方法，支持常用 HTTP 方法、重复业务 header、原始正文和状态；明确保留 4xx/5xx。默认只允许 HTTPS 公网；本机 HTTP/HTTPS 与附加信任根须显式配置。DNS 全答案检查并钉定实际连接，不使用隐式代理、重定向、自动重试或自动解压。

服务端提供精确 method/path 路由、Bearer 鉴权、正文/输出预算、并发限制、超时、取消和有界关闭。普通 HTTP 只绑定 loopback；TLS 模式明确配置 PEM 证书/私钥与监听地址。TLS 懒握手按连接独立执行，半头/半握手/keep-alive 连接在固定期限释放，取消检查也覆盖 rustls 已解密缓冲数据。只有正确认证的请求进入 handler。

显式 PluginService 使用所选 immutable package 的固定 bytes→bytes handler，拒绝内容能力与依赖，新建独占目录内的空内容库，零内容批准执行。网络调用者不能选择其他包、handler或资料。CLI plugin/plugin-tls 提供处理接口，relay 把入站请求送到启动方指定的上游。该适配没有开放通用 guest 网络 import、用户内容 API 或正式服务发布权。

运行方式和限制见 [节点说明](../network_node/README.md)；完整客户端与节点目标见 [网络 API](../docs/PLUGIN_NETWORK_API.md)、[API 节点](../docs/PLUGIN_API_NODE.md)。

## 实际验证

最终 Release **33 项通过**，无跳过或失败；全目标 strict Clippy 通过。

| 测试组 | 数量 | 核对内容 |
| --- | ---: | --- |
| bidirectional | 3 | 实际请求→节点→上游→C/C++/Rust旧Wasm→返回，逐字节比较；已有文件和带SQLite伴随文件目录保留 |
| cli | 2 | 错token/错TLS配置明确退出码1及network: Invalid，在创建目录前拒绝且不输出秘密 |
| cli_tls | 1 | 实际Windows HTTPS可执行文件，受信合成证书＋Bearer鉴权＋真实Rust插件结果 |
| client | 9 | 7方法/二进制/重复头、origin/method/DNS拒绝、4xx5xx正文、302不追随、已知/分块限额、取消超时并发、断连不重试 |
| client_tls | 4 | 指定可信根真实HTTPS，未知/错根拒绝，错误hostname拒绝，有界DER输入；拒绝路径handler计数为0 |
| server | 9 | 鉴权在执行前、路由/方法/参数/输出限额、并发429、超时取消、半头期限、关闭/Drop释放端口、IPv6实际监听 |
| tls | 5 | PEM大小/数量/单私钥与匹配、TLS鉴权/响应、未知CA/错主机名、半握手并行及过期、TLS关闭端口回收 |

日志：`build/network-node-tls-release-tests.log`、`build/network-node-tls-clippy.log`。debug同语义测试亦通过，日志 `build/network-node-tls-tests-initial.log`；不将debug和release相加宣称66个独立测试。

`tool/verify_network_node.py` 使用最终Release可执行文件额外启动 C/C++/Rust plugin 三次与 relay 一次，核对未认证401、认证后的精确二进制输出和上游没有收到节点认证头。日志 `build/network-node-cli-qualification.log`，每次生成新证据目录。该脚本结束指定子进程是测试清理，不能充当CLI优雅退出证明；有界shutdown/Drop由实际server/TLS测试覆盖。

冻结原件完整性检查通过：36个固定文件、13对原Wasm/包，无重编译或重打包。日志 `build/network-node-frozen-integrity.log`。本轮没有修改旧宿主/runtime生产源码，所以没有重新运行或声称完整Flutter/旧宿主/全平台回归。

## 审查和失败记录

独立审查发现并修复：未认证半头可长期占用连接；TLS握手不能串行占据listener；新库主文件之外的SQLite伴随文件风险；token/证书应先校验再创建目录；负例不能用任意非零退出当作通过。对应真实回归均列于上述组。

最初离线依赖选择遇到futures-util0.3.34与缓存sink冲突，改为已完整缓存的0.3.33并锁定。独占目录调整曾出现PathBuf借用编译错误，修正后通过，失败日志保留 `build/network-node-tests-final.log`，不能将该旧日志当最终通过证据。

## 当前产物与边界

- 可执行文件：`build/network-node/release/morrow-api-node.exe`
- 大小：10215424 bytes
- SHA-256：`82ec8ad82000c5fee53341ef7461519c24a7c187338a462bf8bfd319687e2e46`

验证使用本机IPv4/IPv6、合成证书/秘密和独立资料目录；没有真实账号/API副作用、跨设备或公网部署验收。TLS使用内置WebPKI根加明确附加根，没有关闭证书或主机名检查，也不自动继承Windows系统证书库。父目录属于可信启动环境，独占新目录不宣称抵抗同账号恶意替换。

这不是完整插件网络系统完成：正式出站/监听/服务发布声明及Registry授权、远端principal与内容范围交集、通用guest IO、OAuth/账户、完整入站正文协议、SSE/WS/大文件、长期任务、证书轮换、网络持久意图/Unknown查询/录制重放和主应用UI接入仍待完成。当前取消/超时不能证明远端没执行；无自动重试，也无持久网络结果查询。TLS可配置监听地址不等于已完成公开服务部署。

下一阶段将该已实测transport纳入正式服务契约与Manager/Pool授权，继续NET与NODE任务；不能把纯转换适配当作通用插件服务已建立。
