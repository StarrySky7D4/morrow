# 第五轮：Core 网络 batch003 限定复核

限定签收通过。独立审查源码/补丁、认证隔离、构建与二进制关联后，在 joint 隔离环境中复跑冻结原 exe，**6 场景 / 47 项计数断言通过**。本轮核验生产方编译证据，未独立编译 Core，未重跑其他批次探针或旧 43/14 测试。

P-02/J-00/G0 保持 blocked；native/Wasm 产品图 0/2，84 产品验收 not_run。本批不存在需因交接范围不实而退回的问题；它仍是选定 Core 分支的本地拒绝资格，不能扩大为成功网络或完整接管。

## 身份和范围

交接 `receipts/p02-core-network-003/handoff.json`，SHA256 `97501761be789decabc52de2a89311d1d6a877b6dd38fa8c795f649a7b91aacb`，53 绑定输入。旧 17/35/129 输入、当前成功 build/run 内部输入一并合并核验，1164 唯一路径在复跑前后完全一致；集合有交叉，计数不直接相加。

从固定 Codex 8697 文件的真实字节独立重算 blob/根树，新副本 8699 文件仅 3 修改 + 2 新增。重产 diff 与冻结的 391 行、5 文件补丁逐字节匹配，SHA256 `5daaa46c41e5711f696f623ad7a3df580e1daa6a11132bd038069cd3b329cafa`。此前 batch002 独立执行/存储工作副本及共享 host/fork 1108 文件也再次只读核验，mxc1740/nucleo42内容和Git树不变。

**batch003 没有带入 batch002 的执行补丁。** 两份副本的资格结果分别成立，不能相加宣称同一集成产品已同时具备全部行为。

独立核对本批锁 1113 包（1010 registry + 103 本地），registry 名称/版本/来源/checksum 全在原 Codex 固定锁中。生产方 `--locked --offline` build/run 的日志摘要、before/after、build-finished、compiler-artifact 路径及 exe 摘要关联一致；构建消息包含 902 个唯一 compiler-artifact 包身份。上述是证据审查，不是本轮重新执行 Cargo 编译或 metadata。

## 认证隔离和运行方式

原 exe 支持输出路径参数，因此直接运行，不改源码、manifest 或二进制。原 exe SHA256：`c9685f0394d3e559f351dc24e3ca8e80964c77937fce064bd28d8fcdbb839c70`。

联合运行器逐键读取少量明确列出的 Windows OS 字段，不枚举环境变量值；子进程使用全新的 joint home/profile/temp，PATH 限系统目录。未继承个人认证、代理、配置字段，未读取或输出其值。桥接在 ModelClient 构造前拒绝三个认证/刷新变量存在，provider 的 auth/gateway_oauth/aws/experimental_bearer_token/env_key/env_http_headers 均为 None，requires_openai_auth=false；AuthManager、attestation、contributors 为空，retry 为 Some(0)，inference trace disabled，没有初始化遥测 exporter。

原 ModelClient 会无条件收集认证环境遥测，因此这些构造前措施具有实际意义；Unavailable factory 只是额外拒绝后备，不能单独证明未读取凭据。运行源码与回执显示全部捕获的 HTTP/WS 请求都由本地后端返回错误；无真实供应商请求或付费模型调用。本轮没有 OS 网络监测，不据此宣称全局无旁路。

## 六场景独立结果

| 场景 | 后端完整调用顺序 | Core WS 状态 | 计数断言 |
|---|---|---|---:|
| HTTP direct | HTTP stream | false → false | 7 |
| WS disconnect | WS connect | true → true | 5 |
| prewarm disconnect | WS connect | true → true | 5 |
| stream426 + 同一 ModelClient 新 session | WS connect → HTTP stream → HTTP stream | true → false → false | 12 |
| prewarm426 后 stream | WS connect → HTTP stream | true → false → false | 9 |
| preconnect426 后 stream | WS connect → HTTP stream | true → false → false | 9 |

47 为 `check()` 计数，不包含认证/header 前置 assert 等额外守卫。独立检查精确调用数组、每个 Core outcome、状态、HTTP method/URL、model/stream/store/input、认证头缺席及原始正文长度/摘要，不只相信最终 passed 字段。

表中的 disconnect 是沿用用例名，其实际注入错误类型为 **TransportError::Build**（HTTP 和 WS 均如此），不是 Connection 或 Timeout。这里验证本地构造错误/拒绝的传播与选定路由，不能称为真实连接故障、超时或相应重试语义验证。

新桥接没有调用 `force_http_fallback` / `try_switch_fallback_transport`，也没有访问 `disable_websockets`。已阅读原 Core stream、prewarm、preconnect 的 426 分支与共享状态更新：回退发生于原始控制流，同一 ModelClient 的 new_session 延续 HTTP 状态。普通 WS 断开和 prewarm 断开不会错误切换 HTTP。

三项 426 均来自本地 `TransportError::Http(UPGRADE_REQUIRED)` 注入（runtime `synthetic_426`），来源为 **local_injected_error**；不是服务器握手、真实 HTTP 状态收取或 host003 回执。prewarm/preconnect 的 setup-only Ok 后，由后续独立 stream 进入 HTTP 拒绝；setup Ok 不是成功数据流、连接或模型响应。

## 字节与 wire 边界

独立 runtime SHA256：`c6ba1b2a25886a32e166eeed949919949b0a255cd1eff817541fcf218fa4a5b9`；生产 runtime 为 `cadb20edfdd845efe21bb01ea62a5054de791af4693b8a3d5f2a899010eb49d1`。二者不是字节相同：HTTP 正文 JSON 对象成员序列化顺序变化。

本轮逐条重算 5 个 HTTP 记录的实际 UTF-8 字节、22025 字节长度和 SHA256，全部匹配记录；只有完成该检查后才解析正文并归一化 JSON 对象顺序。解析后正文及其余回执字段与生产方完全一致。每条实际正文和摘要均保存在 runtime / result 中，没有把规范化 JSON 的摘要冒充实际发送前字节摘要。

HTTP 捕获点包含真实 Core Responses 序列化后的请求。WS 捕获的是 Core 交给 connect helper 的 provider/extra/default headers 分组与 URL；它在 API 客户端最终头合并、apply_auth、socket upgrade 和 frame 处理之前拒绝。这些 WS 字段不是最终 wire，也没有声称捕获 response.create frame。

## 尚未证明的行为

HTTP `execute` 与 `stream` 的路由 match 都明确把 Injected 交给同一后端，失败没有默认网络后端重试路径；六案实际只调用 stream，不能给 execute、Realtime/unary 或其他未走该 helper 的调用点授予运行资格。默认具体后端的行为保留属于静态检查范围，未作产品回归验证。

`build_api_transport` 的 injected 提前返回位于原 account-routing header 触发 `ClientRedirectPolicy::Reject` 的逻辑之前，而且没有把 `redirect_policy` 参数传给 backend。本批始终返回拒绝，未产生成功传输；该设计不能被描述为继承或验证了默认重定向、账户路由及权限策略。未来成功传输实现必须由 M-03/M-08 明确承接这些责任并重新审查。

只有资格桥接 module 受 feature 控制，公开 ModelNetworkBackend API、ModelHttpTransport 和 Core 注入/路由改动本身未 gated。当前没有连接生产 session startup；未来产品接入仍需单独审查。

仍缺成功 HTTP/SSE/WS、真实 upgrade/frame/close、已建立连接复用与 reconnect、普通 preconnect 错误分支、auth recovery、proxy/TLS/redirect、真实宿主 IPC、完整 agent loop 和 OS 旁路资格。M-03 需要真实 HTTP/WS 目的地、头、状态和传输阶段合同，M-08 需要账户/凭据/恢复权限；M-04 持久化、M-06（关联 M-02）执行生命周期也未因本轮关闭。003 没有被扩成假成功网络后端，没有第二份权威 Schema。

## 证据

`runs/p02-core-network-003-review-001/` 包含 `result.json`、`pre-run-review.json`、`inputs-before.json`、`inputs-after.json`、独立 `runtime.json`、stdout/stderr 与实际复核源码。运行器为 `review_p02_core_network.py`，所有新增输出仅 joint。

宿主 `../host/p02-core-network-003-preflight-review.md` 用于交叉检查认证与 preconnect 分支边界。之后的 `../host/p02-core-network-003-host-review.md`（SHA256 `4cec7e0f05debfbac83dbd0924d0136bf9004c1ffbe20b805372f1529f5ac3dc`）及 `p02-core-network-003-host-input-check.json`（SHA256 `278a5f2a238d105247641971a21defdf25f4e5679e0a2b6dc09c716e0475744e`）摘要已核对，Build 错误类型与 redirect 提前返回边界也已回到实际源码独立确认。宿主报告是只读审查，不作为本轮独立运行证据。本轮没有修改插件、宿主合同或原计划，没有提交、推送、发布。
