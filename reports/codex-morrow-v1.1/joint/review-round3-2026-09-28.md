# 第三轮：完整固定源码与 P-02 最小网络拒绝探针

本轮限定签收通过。完整固定源码获取阻塞已解除；P-02、J-00、G0 仍 blocked，两产品图仍 0/2，84 产品验收仍全部 not_run。没有新产品缺陷需要据此批次退回；以下通过范围不能扩大为完整接管资格。

## 固定输入与独立源码复核

输入为插件 `receipts/p02-native-probe-001/handoff.json`，SHA256 `1fc5833e632bb3afab827a9ca3519418ab7afdf34c6e50e6cafa59ecce707020`。35 个本批次输入、原交接及其 17 个冻结输入、生产方 exe 在独立构建/运行前后均匹配固定摘要。源码、fork 原件/构建副本也在前后重复比对，未漂移。

| 固定源码 | 提交 | 文件 | 含根目录的 Git 树 | 字节 |
|---|---|---:|---:|---:|
| Codex | `44fe510ce3ee61c8ef623adcbf89b901c73ddd61` | 8697 | 985 | 86130187 |
| CC Switch | `846de29c13ac4d65f164db8c15dd5fd58e29f972` | 1322 | 133 | 48415580 |

全部文件集合、大小、SHA256、Git blob SHA1 均独立检查，并使用实际文件内容重建所有 Git 树，分别得到根树 `3b868fad63be6ac5db91402b579fab37f587d7d5`、`e373c27485681c074ddab7d4f085ee9a6667b891`，与固定提交回执一致。Codex 唯一符号链接按链接文本核验，目标保持在源码目录内。Windows 上可执行权限位来自固定 Git 清单，未声称 Windows 文件权限提供 Unix 模式证明。本轮不重新联网获取提交元数据；依据交接中的固定提交/API记录与内容寻址身份复核。

这是完整固定源码快照，不是 Git checkout 或完整历史。此前失败下载记录保留，但不能继续用“下载不完整”描述当前阻塞。

## 依赖与重定位

两个 WebSocket fork 原件分别按完整文件集合和 Git 树检查。构建副本只有 tokio-tungstenite 的 `Cargo.toml` 将固定 tungstenite git/rev 改为 `../tungstenite`；TOML 语义比较确认除此之外无改动。34 + 888 个文件中 Rust 原码均一致。另核对未进入解析图的 runfiles 五文件包及仓库许可文件；没有把这个包子集宣称为完整 rules_rust 仓库。

独立 `--locked --offline` metadata 得到 598 包：21 本地路径包、577 registry 包。后者 574 个名称/版本/来源/checksum 元组保持 Codex 原锁，另外 3 个来自 kit 原锁。生产 metadata 在 tungstenite 下有 10 条重复相同 `dep_kinds`；按集合去重并归一化 consumer 根位置、节点顺序后，依赖、目标条件及 features 一致，包身份与清单位置一致。没有将 598 个解析包误报为全部编译：实际构建为 476 个 compiler-artifact 包身份，60 条构建脚本消息、54 个唯一构建脚本包。

原 exe 的编译期路径只允许写入插件 receipts。为满足 joint-only 写权限，独立 consumer 保持 `main.rs` 与 `Cargo.lock` 逐字节不变，仅把 `Cargo.toml` 中的依赖/patch 路径转换成原只读输入的绝对路径。没有修改探针拒绝逻辑或绕过输出限制。

| 对象 | SHA256 |
|---|---|
| 原 Cargo.toml | `5d543cba59604c836118a92303ee1912ec2e0effc39e1a831a870384353cebb9` |
| consumer Cargo.toml | `4d578d700f24b269c998d39deb18f6ef577d054e8b8f3358611651cd4cd019ad` |
| 两端相同 main.rs | `c3259978d27b34f73a16ccc2df6ab1db3dd2d782f7b01980ea94e7ce2f4662f0` |
| 两端相同 Cargo.lock | `ec97e437d4ccc62025218d82202bd8593dbe7a3a137411e5878f232bf8d719ff` |
| 生产方 exe（仅核验摘要） | `e0f5d6da9587cc3722b98701c52bd23474e935e302bd77b41f4796521aa5c9ca` |
| 独立构建并运行的 exe | `cfcae4f0cfb44a7ae8ff77535a05a31b6835ef00cb44bd66a0c1be3b82283cad` |
| 两端逐字节一致的运行回执 | `209e3ea22459138afaef1d3f08e7c7d93d541bd06ad7a82fb49f0266262e75c8` |

编译期 `CARGO_MANIFEST_DIR`、输出限制和构建路径不同，两个 exe 摘要不同；未主张二进制可复现，也未把独立 exe 冒充原 exe。重定位 diff 与解析证据均保存。

## 实际重放与边界

使用已安装 Rust/Cargo 1.95.0，在 Windows x86_64 MSVC 上独立离线构建成功。实际调用链为 `ResponsesClient::stream_request -> EndpointSession::stream_encoded_json_with -> HttpTransport::stream`，不是直接调用自制 transport 的假循环。

- `host_disconnected_before_open`：真实上游请求进入替换 transport，一次 Stream.Open，断开 fixture 拒绝，零 commit；请求 241 字节及其摘要与实际 wire 字段绑定。
- `unapproved_destination_rejected`：未许可目标在进入宿主 exchange 前拒绝，宿主调用和 commit 均为零。

2 场景/15 断言通过。独立审查还从受控请求字段重算序列化正文摘要、检查方法/目标/无凭据头、调用次数、会话/attempt/epoch/request ID 和命名错误返回。003 kit 副本 180 文件及权威 12 源文件摘要在构建前后相同；未定义第二份权威 Schema。

尚未覆盖：Core ModelClient 循环及完整注入、exec/store 接管、成功 HTTP/SSE、真实宿主 IPC、原生/Wasm 产品图和包、OS 网络监测、完整无旁路证明。探针未构造真实网络客户端，不等于已完成 OS 级隔离验收。未运行真实账号、模型或付费请求，未重跑旧 43 项入口测试与 14 项 kit 测试。

## 证据与保留的失败尝试

当前有效结果为 `runs/p02-native-probe-001-review-004/result.json`，补充身份检查为同目录 `final-identity.json`；另有 `consumer-manifest.patch`、`consumer-relocation.json`、`metadata.stdout`、`build.stdout`、`build.stderr`、`commands.json`、`inputs-before.json`、`inputs-after.json` 及 consumer runtime。

此前无编号首次运行与 `-002` 保留复核工具处理 runfiles 许可文件及重复 metadata 记录的失败，不作为产品缺陷。`-003` 的实际构建在过长对象路径遭遇 MASM A1009 与 zstd 编译失败；`-004` 仅改短 joint 内 isolated target 路径后成功，源码/锁/行为未改。所有失败证据保留，没有覆盖或删除。

新增/修改仅位于联合验收目录。独立输出在 `joint/t-p02-004` 和本轮 `runs` 内；未向插件源码、receipts、target 写入，未提交、推送、发布。
