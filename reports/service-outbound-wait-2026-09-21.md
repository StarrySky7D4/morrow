# 入站服务与出站等待的组合验证

日期：2026-09-21。基线 `a314ed0`，分支 `codex/io-safety-refactor`。应用版本仍为 `0.1.9-test.52+56`。

## 范围与实际链路

新增真实认证 loopback HTTP → managed Wasm → 原 Core 内容读取 → 实际出站 HTTP → 持久服务完成的组合测试。测试包使用 WAT 编写，不是 Rust SDK 第三方插件资格测试。

包构建后才批准出站端点，随后把真实请求帧存入获授权卡片。guest 从实际 Core 响应中读取该帧，调用既有 IO import，再把实际 IO 响应装入绑定原请求的服务完成帧。占位字节仅用于计算固定测试帧布局，不替代读取或网络返回；宿主仍验证原请求、卡片范围、端点和完成帧身份。

共三个新增测试、四种场景：

1. 上游收到完整请求后停在显式屏障。入站请求仍等待时，原拥有者提交独立卡片并在一秒内收到回执。释放上游后，返回实际 HTTP 状态和内容；同一幂等键重放及历史查询均返回同一结果，外发连接只有一次。回收并重开原库，两层意图均 Observed，独立卡片保留。
2. 出站等待期间撤销端点：不交付成功；再次请求为 409，两层意图均 OutcomeUnknown，没有再次外发。
3. 出站等待期间使历史保留期到期：不交付成功；再次请求为 410，两层意图均 OutcomeUnknown，独立卡片保留。
4. 原拥有者写入卡片后阻塞回调，此时停止服务及监听。回调释放前无法取回原拥有者；释放后等待实际退出，原 HostBinding、卡片和两层 Unknown 保留，执行／维护／断开诊断正常，没有成功响应。

## 应用包装器修正

检查应用接线时发现 `workbench_host::http_tasks::OwnedRouter` 没有覆盖新 `begin`，会退回旧同步 `route`。现转发到原 `HttpRouter::begin`，仍在转发前核验首个调用及完整原请求字节。拥有 Tokio Runtime 的包装器留在 worker，实际传输 join 完成后才可释放。

主应用服务入口仍为 `DenyOutbound`；没有通过本轮修正绕开持久端点授权。下一编码项是接入服务会话的明确出站资源选择、原拥有者上的批准与撤销，再验证真实应用交互。不能把原生库组合测试称为主应用服务已经能调用外部 API。

## 验证

- 相关网络回归：`managed_http` 20、`managed_content_service` 9、`managed_service_owned` 9，共 38 项通过。
- 相关网络 lib/tests 严格 Clippy、修改文件 rustfmt、diff check 通过。
- 从当前 `plugins/http_forward` 源码重新构建 wasm32 Release 插件，设置 `MORROW_HTTP_FORWARD_WASM` 后工作台 6 项 HTTP 回归通过（含真实 Rust guest、原库证据重开、取消回收和请求替换拒绝）；工作台 lib 严格 Clippy 通过。最初运行缺少该测试环境变量，补齐真实构建产物后重跑，不以跳过测试代替。测试构建仍有既有 `prepare_write` dead_code 警告；生产 lib 严格检查通过。
- 未重新运行上一轮全部运行时测试；本轮运行时生产代码没有变化。未重新打包 Windows、推送或发布。

日志：`build/service-http-integration.log`、`build/service-http-regression.log`、`build/service-http-clippy.log`、`build/service-http-workbench.log`、`build/service-http-guest-build.log`、`build/service-http-workbench-clippy.log`。

GLM-5.3-flash/max 提供 guest 与停止测试草稿；主代理修正导入签名、入口、地址、错误 API 和缺失的超时／断言，再接入真实环境。首次组合失败定位为夹具使用 64 KiB IO 输出容量，既有契约要求 128 KiB；修正夹具后通过，没有放宽生产校验。辅助调用结果槽满时通过官方 CLI 取消未执行队列记录并释放已过期结果，没有修改服务数据库或重放未知结果。

S2/S3 的上述原生组合子集已有证据，但应用服务出站资源、更多完成／停止竞争、真实系统输入及完整平台资格仍开放。新 IO SDK 尚未稳定。
