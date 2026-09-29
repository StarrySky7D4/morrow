# P02 Core network 003：宿主限定复核

日期：2026-09-28。交接 SHA256：`97501761be789decabc52de2a89311d1d6a877b6dd38fa8c795f649a7b91aacb`。

结论：源码、冻结输入与生产方回执支持六个实际 Core 拒绝/合成 426 回退分支的限定交付。未发现阻止接收该限定证据的问题。宿主本轮独立进行只读哈希、源码和回执字段核对，没有独立重建或执行 probe；6 案 / 47 项计数断言通过仍是生产方运行结果，不是本轮运行复现。P02/J00/G0、产品 native/Wasm 0/2 和 84 项 not_run 不升级。

## 核对结果

同目录 `p02-core-network-003-host-input-check.json` 与 `check-p02-core-network-003.py` 保存独立检查结果和可重复的只读检查器。

- 本批 53 项输入以及旧批 17、35、129 项输入全部匹配。
- 原始固定 Codex 8697 项与清单匹配；新工作副本 8699 项全部匹配，无清单外文件或缺失文件。仅 5 个预期源码差异，未带入 batch002 执行补丁。
- 补丁摘要 `5daaa46c41e5711f696f623ad7a3df580e1daa6a11132bd038069cd3b329cafa`；可执行文件摘要 `c9685f0394d3e559f351dc24e3ca8e80964c77937fce064bd28d8fcdbb839c70`，均与交接一致。
- 生产方 build/run 回执均 exit0，输入前后不变且当前仍匹配；记录耗时分别 333.88s、1.11s。未在本轮重跑 Cargo metadata/build/run。
- runtime 摘要 `cadb20edfdd845efe21bb01ea62a5054de791af4693b8a3d5f2a899010eb49d1` 匹配；独立复算全部 5 份 HTTP body 的 UTF-8 字节长度和 SHA256，并核对实际 POST 目的地、model/stream 字段、调用顺序、状态和错误结果。
- 宿主 003 的 180 项 kit 文件、12 项 canonical 文件及 manifest 全部匹配冻结版本；没有修改 003。

检查器初版把 Git mode120000 的 bubblewrap/LICENSE 按目标文件正文计算，造成一次本地检查失败；核实其原始与新副本均为指向 COPYING 的符号链接后，按清单语义改为计算链接文本。最终通过，不是上游文件被修改，也没有更改该链接或目标。

## 六案与实际代码

| 实际入口 | 回执调用与状态 | 计数断言 |
|---|---|---:|
| Core stream，WS disabled | HTTP stream 拒绝；false→false | 7 |
| Core stream，WS enabled | WS connect 拒绝，无 HTTP；true→true | 5 |
| Core prewarm | WS connect 拒绝，无成功 frame；true→true | 5 |
| Core stream，合成426 | WS426→HTTP拒绝；同一 ModelClient 的 new_session 再次 HTTP拒绝；true→false→false | 12 |
| Core prewarm，合成426 | setup-only Ok，后续 stream 被 HTTP拒绝；true→false→false | 9 |
| Core preconnect，合成426 | 独立 setup-only Ok，后续 stream 被 HTTP拒绝；true→false→false | 9 |

桥接代码只调用既有 Core 方法及读取 enabled 状态，没有写 disable flag，也没有调用 fallback helper。stream 案的持久回退使用同一个 ModelClient 创建下一 session。预审指出的独立 preconnect 426 臂已加入；通用 preconnect 失败仍未测。

`build_api_transport` 在具体客户端构造之前选择注入后端；ModelHttpTransport 的 execute/stream 都是显式分派，错误没有切换回默认具体 transport。WebSocket override 保留外层 Core headers、timeout 和结果处理，位于 API client 最终 header merge/auth/握手之前。拒绝后端没有制造 ResponseStream 或 WebSocket 成功对象；426 是 local_injected_error，两个 setup Ok 不代表网络或模型成功。

认证预审要求已体现在 runner 与 bridge：runner 逐键读取允许的 OS/build 字段构造子进程环境，使用本批隔离 profile/home/temp；bridge 在 ModelClient 构造前检查三个认证环境变量不存在，固定 provider 认证来源为空、requires_openai_auth=false、request/stream retry 为0。未接 AuthManager、配置加载、attestation、contributors、trace writer 或 exporter。源码和回执支持本批选定启动路径的约束，不是全进程凭据访问审计。

## 必须保留的边界

1. 所谓“disconnect”具体注入 `TransportError::Build`，是本地拒绝错误；没有测试真实 Connection/Network/Timeout 错误或连接中断。其消息经 Core 转为 stream disconnected 不改变错误来源。零重试夹具也不证明重试或 auth recovery 语义。
2. 六案实际仅执行 HTTP stream。execute 只有源码分派/拒绝审阅，无运行覆盖。Realtime/unary、成功 HTTP/SSE/WS、已建立连接的复用/重连、代理/TLS/重定向、真实宿主 IPC、完整 Core loop 与 OS 全局旁路均未验证。
3. HTTP override 在原账户路由 header→redirect Reject 处理之前返回；接口也未把 redirect_policy 交给后端。因此本次始终拒绝没有产生授权缺口，但不能声称注入路径继承了默认 transport 的目的地、重定向或凭据策略。生产能力需要 M-03/M-08 的明确合同与后端执行证据。现有接口注释“implementations must enforce”是责任约定，不是实现证明。
4. WebSocket 回执是分组 Core 输入，不是最终 wire headers/真实握手/已发送 frame。HTTP body 哈希证明截获字节，不证明任何请求已通过宿主提交或服务器接收。
5. 只有 qualification bridge 受 feature 控制；新增 backend API 与 Core routing patch 未 gated，且未接生产启动。默认后端保持原调用的静态结构，不等于默认分支或产品行为已经回归验证。

本轮只新增宿主报告、输入核对结果和检查器，未改插件封存资料、joint、生产代码或合同；未触发账户操作、外部模型请求、提交、推送或发布。独立运行复现若由联合验收执行，应单独报告其可执行文件身份、隔离环境、前后输入和运行结果，不能由本文代替。
