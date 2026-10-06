# Codex 插件相关 SDK 候选冻结门禁

日期：2026-10-05。开发线 `codex/windows-sdk-convergence-20261005`，本轮增量基线 `679e16a6a44f8aac13f85da05204d9cd8afb30b8`。本轮推进可重复核验的候选身份及完整性门禁；完整 SDK26、G04、Codex G0 与生产插件资格仍 **OPEN**。没有新协议修订、安装包或 Release。应用版本仍为 `0.1.9-test.58+62`。

## 候选身份与接口范围

唯一 schema 权威仍为 `contracts/experimental/agent_host_v1`，wire major **1**、revision **1**。本轮没有修改该目录、schema、锁文件、归档 companion、历史 kit 或 consumer receipt。

| 身份 | 原始字节 SHA-256 |
|---|---|
| `agent_host.capnp` | `da0ac7a42e4b0f6aec0cfbdd2358cf86688b08e862626186bb9de4914f355a7f` |
| `host-kit-003-copy/manifest.json` | `5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01` |
| `host-kit-review-003/review.json` | `664b5802af5c3a83973730df4fa3b721c0ccfd9e724f9b71edbdcd4f9fdb0d5f` |
| 当前 canonical 12 文件清单摘要 | `9d67874586c25eccd06f119a4bc040d1f8cdec1657c7dea60a0bcc07e5ecfef2` |

源码清单摘要按排序后的 `{path, bytes, sha256}` 数组，以 `json.dumps(sort_keys=True, separators=(",", ":"))` 的 UTF-8 字节计算。具体文件清单及本轮检查结果见 [validation](codex-sdk-freeze-validation.json)。以上是此候选的可复核身份，不是稳定版本发布或生产批准。

已实现的四个实验 family 仍为 `native-session-v1`、`stream-io-v1`、`session-events-v1`、`tool-dispatch-v1`，其中 backend 为 opt-in qualification fake。`agent-content-v1`、`stream-view-v1`、`native-package-v1 + bundle-v1` 仍是 draft，没有 callable wire tags 或生产能力。接口／语义变动仍须新 experimental revision、重新生成 kit 和 consumer review；不能更新清单后沿用旧资格。

## 本轮实现

- [宿主 kit 校验器](../../tool/agent_host_build.py)核验外层与源码清单、baseline、生成绑定清单与 `SCHEMA_DIGEST`、完整19对向量与1项UnsupportedVersion拒绝，以及每个decode和三条Cargo命令的证据文件。命令须指向同一记录cwd下的schema／Cargo.toml，使用独立target；默认check不得启用qualification或all-features。缺失许可证、非普通文件、链接／重解析点、重复字段／条目、逃逸路径、未列出的文件、失败或zero-test资格证据均拒绝。
- 新增[当前宿主 Codex 候选门禁](../../tool/verify_codex_sdk_candidate.py)：绑定consumer review摘要及qualification_only状态，再将kit全部源码的成员、大小和原始摘要与当前canonical完整源码比较，核对wire身份并调用宿主kit校验器。自行重算kit与review不能掩盖与当前canonical的源码漂移。检查期间review／canonical变动也会拒绝。此工具不加载或执行kit里的Python工具。
- [目录静态准入测试](../../plugin_runtime/tests/directory_request_profile.rs)仅保留packages feature条件，使原有9项Package／Runner静态检查可在Linux实际执行。没有开放Linux目录backend，也没有新增原生选择授权。

消费入口在仓库根目录执行：

```bash
python3 -B tool/verify_codex_sdk_candidate.py \
  --kit companions/morrow-codex/sdk/host-kit-003-copy \
  --review companions/morrow-codex/receipts/host-kit-review-003/review.json \
  --json
```

当前输出验证180个kit文件及12个canonical源码文件，并明确 `status=qualification_only`、`sdk_freeze=OPEN`、`p02_qualification=NOT_RUN`、`production_binding_available=false`。此检查绑定调用者提供的review与当前源码，不对review签名或审批人进行认证，也不证明历史记录中的命令实际执行过。`verify-kit`单独运行只承诺kit内部完整性；其 `evidence_execution_verified=false`、`sdk_frozen=false` 保持。后续消费使用当前宿主入口；归档 `companions/morrow-codex/tools/build_plan.py` 保留历史原件，不能代替该入口。

## 当前实际验证

环境为Linux x86_64云端，Rust 1.96.0、Cap'n Proto 1.4.0。各范围单独计数，不继承Windows历史资格。

| 检查 | 当前结果 | 范围 |
|---|---|---|
| `test_agent_host_build` | 22 PASS | 导出失败保护6、kit完整性／证据回归16 |
| `test_codex_sdk_candidate` | 18 PASS | review、完整canonical来源、真实kit、漂移与路径反例 |
| 原baseline及contract sync Python tests | 22 PASS | 原件兼容门禁11、同步检查11 |
| canonical contract `--features qualification` | 14 PASS，0失败／忽略／过滤 | 普通fake字节交换、相关性、版本、取消／重试、事件、tool dispatch |
| canonical contract默认feature `cargo check` | PASS | 默认不启用qualification的库编译；不是运行时生产backend验证 |
| `directory_request_profile` | 9 PASS，0失败／忽略／过滤 | Package／Runner静态准入，未驱动原生目录owner |
| guest frozen baseline | 36个清单成员PASS | 原13组Wasm/package，不重建／重打包 |
| transport frozen baseline | 17个清单成员PASS | 原transport输入，不执行网络guest |
| contract sync `--check` | PASS | 只读检查，没有更新快照 |
| 两处原kit与consumer review | PASS | 全部已列叶文件重新核验，归档身份保持 |

36与17是两个清单内的成员数；连同各自manifest及root pin，原冻结输入合57。原SDK327未因本轮发生修改；本轮没有重新执行其全部历史资格。

Python命令：

```bash
python3 -B -m unittest tool.tests.test_agent_host_build \
  tool.tests.test_codex_sdk_candidate tool.tests.test_sdk_baseline \
  tool.tests.test_sdk_contract_sync -v
python3 -B tool/verify_plugin_sdk_baseline.py
python3 -B tool/plugin_transport_baseline.py verify
python3 -B tool/sync_plugin_sdk_contracts.py --check
```

Rust命令先激活本云环境的 `/workspace/.morrow-tools/activate.sh`，在仓库根目录执行：

```bash
cargo test --locked --offline --manifest-path contracts/experimental/agent_host_v1/Cargo.toml \
  --features qualification --target-dir build/sdk-freeze-c11/agent-host
cargo check --locked --offline --manifest-path contracts/experimental/agent_host_v1/Cargo.toml \
  --target-dir build/sdk-freeze-c11/agent-host
CARGO_TARGET_DIR=build/sdk-freeze-c11 cargo test --locked \
  --manifest-path plugin_runtime/Cargo.toml --features packages --test directory_request_profile
```

首次canonical离线test因缓存缺少锁定的 `cfg-if 1.0.5` 返回101；以 `cargo fetch --locked` 补齐依赖后，原命令离线重跑14 PASS。失败与重跑日志分别保留在当前workspace的 `build/sdk-freeze-c11/logs`，没有覆盖失败或更新Cargo.lock。directory测试首次误选其他target的中断退出130另存、不计为执行资格。Rust unit及doc runner各自0项不计入14。整库Clippy、本轮Windows owner及产品构建未运行。

## 尚需闭合的冻结门槛

1. Codex真实native／Wasm产品图、host连接／进程身份、真实批准／撤权与P-02执行仍需独立实现或验收。当前fake的PASS不能替代生产plugin，历史receipt不升级为本轮通过。
2. 持久事件／内容／视图／包安装及恢复、真实网络／账户、跨进程故障身份和生产UI仍按原M阶段及完整SDK26要求推进。draft不升级为已实现family。
3. G04继续验证可信picker／anchor以上来源、真实三语言目录guest、Workbench／ProtectedSession、blob耐久backend与完整平台矩阵。Linux静态准入9项不能关闭Windows owner或产品门槛。
4. 完整候选实现后，以最终源码／构建／consumer身份执行全部验收，再确定稳定冻结和分发身份。当前只交付可运行的冻结门禁及限定证据。
