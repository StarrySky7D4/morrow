# M03 到期终态修复与新批次006（2026-09-30）

当前分支 `codex/m03-stream-revocation-backpressure`，HEAD `04b060ef2a8ae7e7806af7b0dcc772308e9b07e4`。本轮未提交、推送、合并、部署或触发 Actions/CI。环境恢复后已实际构建与运行；最终进程核对无残留 Morrow/Python 场景。原暂停检查点、Unknown run 与失败尝试保留。

## 生产修改与安全边界

生产 host 修改 `native_session_stream_001/src/supervisor.rs`、新增 `src/expiry_teardown.rs`、`src/main.rs`；协议 README 补充实现语义，Capnp Schema 未改，SDK 未冻结。

authority 到期首先持久撤权并关闭业务 gate，原 deadline 不续期。Stop 后仅在完整 Welcome 已实际写出、撤权成功且无原残帧时保留有界 teardown 控制输入。仅允许原身份、原预算、递增序号的 Query/Cancel/Close/zero-credit；新业务与正 credit 拒绝。固定 close_ms 截止和每帧截止均不能更新。RequestClosed 必须来自实际 pipe reap 与 network join；匹配 ACK 完整写出后关闭输入。错误身份、序号、残帧、超时、Close 后尾部、pipe/network/stderr 错误均保持失败并撤销通道。

CLI 仅在实证终态齐全时令 host exit0。first cause20、业务 Deadline/Unknown 和 `business_success_claimed:false` 保持；缺 ACK、错误序号、迟到 ACK 或错误尾部仍 exit2，未放宽成通过。

## Fixture 与工具修改

`src/bin/morrow-native-close-peer.rs` 是合成测试 peer，不是生产 guest。guest fixture004/005/006 位于独立 `morrow-codex` 与仓库 `companions/morrow-codex`：真实结束观察并发共享原500ms预算，线程 join 真实收集，EOF writer 正确退出，006补充实际 main 顺序下缺 control_end 的负例。原失败不能靠测试之后的额外清理升级。

新 `tool/m03_host_expiry_006_check.py` / `freeze.py`、`m03_expiry_control_006.py`、`m03_passive_fault_006.py` / `freeze_006.py` 与对应纯测试保留旧入口。新 runner 要求实际 host0、Close/ACK/EOF、真实回收及匹配 ACK；不把聚合业务错误改成成功。运行后追加冻结入口与归档明确称 postrun，不冒称早于运行的独立冻结。

## 实测结果

| 范围 | 实测 | 证据 |
|---|---|---|
| 生产 host default/feature | 13组命令全部exit0，60次定向测试调用（含重叠），271项源码前后不变 | [check003](host/m03-expiry-006-check-003/receipt.json) |
| 真实 control-only | 14/14预期结果；负例均有具体协议/期限失败，owner全部Released；0 HTTP | [result](host/m03-stream-001/expiry-control-20260930T162848180293Z/result.json) |
| guest006 | Core18/native37通过，离线Windows构建通过 | `companions/morrow-codex/receipts/m03-fixture-006` |
| 严格 runner | 15/15纯回归通过 | [日志](host/m03-expiry-006-final-review-001/runner-pure-tests.log) |
| authority-deadline-006-001 | expected_fault_observed，host0，1 POST，ACK seq7、控制干净、全部join、Released，业务Unknown保留 | [result](host/m03-passive-fault-006/runs/authority-deadline-006-001/result.json) |
| network-abort-006-001 | expected_fault_observed，host0，1 POST，ACK seq7、控制干净、全部回收 | [result](host/m03-passive-fault-006/runs/network-abort-006-001/result.json) |
| pipe-partial-close-006-001 | expected_fault_observed，host0，1 POST，ACK seq7、控制干净、全部回收 | [result](host/m03-passive-fault-006/runs/pipe-partial-close-006-001/result.json) |

[最终离线核验](host/m03-expiry-006-final-review-001/verification.json)核验三个seal全部文件及guest文件摘要、271项host源码/二进制、guest源码副本，未重复HTTP。guest原证据另存带原路径/hash provenance的副本。

host candidate manifest SHA256 `7454d4e915ffcb3127c92d3bfb48e2ccbd29c862a01c36928eb49b8f057f15b6`，feature exe `b5b85cd35864166f939eb95edf18c4d75781e3ef503880496e0969bd71fd0a1f`；guest006 manifest `d7c6ca55ce97adbb447270765483a838d00d7022c0787e5d4f64911bd0d8aadb`。[runner postrun归档](host/m03-passive-fault-006/runner-candidate-postrun-001/manifest.json) SHA256 `4b3a653d00d42b28e54ddf95e62ad731284ad7cc14026ccd8973431bc0067530`。

check001的编译失败、control脚本首次错误路径和no-close证据JSON中断失败均保留；修订后使用新目录，没有覆盖或重判原尝试。原authority001仍unconfirmed/host2。

## 独立只读复核

`/root/review_m03` 未写文件或启动场景。复核促成完整Welcome物理写出门槛、teardown fatal错误拒绝两项修正；最终未发现新阻断。独立核对candidate全源码/两exe与authority006全部本地/guest seal。原wire RequestClosed ordinal60 → Close seq7 received68 → State ACK sent72；ACK完整436B，10.0354746s完成 < 固定11.0246677s期限。原authority10s不变，gate fence ordinal3后无新data write issue。14control场景负例有精确原因，不仅看exit2。

## 后续边界

本次完成被动故障三场景及到期终态缺口，并补齐可重用runner冻结/回归入口。不能宣称完整M03、产品G0或SDK冻结。真实非零OS短成功completion未注入；并发/崩溃恢复owner、同用户隔离、完整产品图与跨平台仍开放。下一可执行切片应先固定真实OS短写可观测方案（不能用partial-close模拟当短成功），再转M04持久writer和M06执行边界；安装/权限/外部服务变更须另行确认。本机已有packager三文件纳入版本跟踪且62项本地回归通过；完整单ZIP外部交付未确认，未重复上传。
