# 托管插件真实 HTTP/HTTPS 出站验收

日期：2026-09-19。结论：**PASS_SCOPED**。基线 `6b4923e1801f3f6005fd0fb392548cacc5440b7b`，本地隔离分支 `codex/io-safety-refactor`。本轮将前一批持久作业接入具体原实例资源批准和现有原生网络客户端，没有另建网络传输栈；完整目标仍见 [网络方案](../docs/PLUGIN_NETWORK_API.md)。

## 实际新增

- `plugin_runtime::http_io::HttpGrant`：可信宿主显式发给原 Manager/Host/ManagedInstance/IoBinding 的运行期资源批准。HttpRequest、CredentialUse 取实际批准交集，原包相同也不能跨实例借用。资源只计resource，不重复计job或字节；clone/revoke保留仍被持有的租约，最后释放才归还。
- `network_node::managed_http`：固定origin/方法/profile/字节/期限/TLS根与凭据引用，生成端点引用，接入`submit_brokered`。guest业务头不能覆盖凭据或连接分帧；真实secret仅在传输前注入，受保护请求仍是guest原帧。
- 原作业持久顺序贯穿实际网络：请求原件/Prepared → OutcomeUnknown发送边界 → 单次真实传输 → 有界结果/响应原件/Observed → 最新授权交付。已观察/未知操作不能重发。HTTP4xx/5xx/302属于响应而非传输失败，禁止自动跟随跳转。
- 同时监测绑定撤权、作业取消和端点独立撤权；新增资源guard保留到Ready最终read/drop，修复“端点撤权但旧Ready仍可领取”缺口。同步网络已到远端后不能宣称回滚；取消/超时/断线保留Unknown，等待提供者核对。
- 新增`Client::send_raw`/`RawHttpResponse`，响应头保留原bytes、重复项及合法obs-text；旧文本send保留严格转换。避免真实200响应仅因合法非ASCII头值被误记为Unknown。

资源配置API与具体限制见 [托管HTTP契约](../docs/PLUGIN_MANAGED_HTTP.md)。当前是可信宿主显式批准的内存资源，不是已完成Registry持久资源策略或用户授权界面。EndpointRef与命令摘要不是独立授权来源。

## 本轮验证

Windows；release、locked、offline。所有网络测试为本机临时端口、合成凭据和临时库。使用实际TCP/TLS和Wasm，不访问用户账号、真实第三方服务或公网监听。

| 检查 | 结果 | 日志 |
| --- | --- | --- |
| network_node全量，plugin-adapter | **47通过，0失败** | `build/managed-http-network-final.log` |
| plugin_runtime全量，all-features | **331通过，0失败；1个由父测试调用的子进程入口ignored** | `build/managed-http-runtime-full.log` |
| 两crate全目标严格Clippy | 通过，`-D warnings` | `build/managed-http-network-clippy-final.log`、`build/managed-http-runtime-clippy.log` |
| 默认runtime wasm32库检查 | 通过，仅验证编译边界 | `build/managed-http-wasm.log` |
| 冻结SDK完整性 | 36固定文件/13原Wasm及包对通过，无重建/重封装 | `build/managed-http-sdk-baseline.log` |
| 冻结SDK原包执行 | 9项基础+3项依赖通过，已包含在runtime全量内 | `build/managed-http-runtime-full.log` |

47项包含13项新增managed_http集成和1项新增底层raw响应回归，不与前一批33项累计。331项包含3项新增资源批准测试，不与专项57项累计。新增wat测试依赖锁定至仓库已有版本，只新增其5个离线可用包；未升级现有依赖版本。

真实受管路径覆盖：

- GET/HEAD/POST/PUT/PATCH/DELETE/OPTIONS全部从实际Wasm进入网络，校验服务器实际收到的方法、路径/重复查询、二进制正文与单次调用，最终读取Store的Observed及原始帧。
- 201、404、429、500和302保留状态/正文；重复头、Content-Length/Connection、合法0xE9头值原样；302未访问第二监听端口。
- 错引用、未准方法、错误凭据在网络与持久意图前拒绝；同Manager同Host同包的第二个仍活实例不能使用第一个实例端点。
- 服务端收到请求后断线与请求期限到达留下Unknown，同操作再次提交不增加服务端调用数。发送后端点撤权不交付；Ready后撤权保留已记录Observed但清空载荷。
- Content-Length与chunked正文超限、过多头、单值过长、完整协议帧超预留上限均拒绝成功交付；HEAD/204/304大广告长度不被误判成实际正文超量。
- 凭据只出现在真实传输，guest请求原件不含注入secret；真实TLS正确根/主机名通过，错误根/主机名拒绝且业务handler0次。
- 资源only准入在job满额时仍可建立；resource满额第二批准失败不改usage；撤权/clone不会提前归还资源；缺类别批准、错误所有者/到期均拒绝。

专项先通过22项（12项托管+10项client），网络首轮全量46项；补七方法端到端向量后最终47项全量通过。最终检查覆盖新增测试。两名实现/测试代理与独立审查协同，所有实际Cargo验证由主代理统一运行。

## 仍须推进

1. IO-D2：接入guest服务发布批准、远端身份与插件能力交集、路由冲突/撤权/节点停止及真实服务请求；不以现有纯转换API原型代替完整服务节点。
2. IO-D1产品接入：Registry持久资源配置、主应用授权/任务/恢复UI、路径范围、凭据库及真实提供者幂等/状态查询；当前只支持内联64KiB正文、16KiB头和128KiB帧，缺流/大型上传下载、SSE/WebSocket/OAuth。
3. IO-D3及SDK：受控文件选择/枚举/创建/替换/删除，三语言类型化IO扩展、录制隔离重放和各平台资格。未将旧SDK兼容等同新网络SDK稳定。

本轮没有变更core Store/schema、冻结SDK和应用版本，没有迁移用户数据、构建安装包、打开公网服务、推送或发布。历史核心/宿主测试未重跑，未计作本轮证据。默认Wasm编译不说明本原生HTTP模块已支持浏览器。
