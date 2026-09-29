# H1/H2 本地宿主候选001

日期：2026-09-30。实施范围为native_session_stream_001的原期限只读诊断及显式资格feature下的12B前缀关闭接缝；新增host局部检查/冻结脚本。没有启动HTTP、guest子进程、提交、推送或发布；本结果不是三个真实联合故障场景或M03整体资格结论。

候选目录：`reports/codex-morrow-v1.1/host/m03-fixture-003-host-candidate-001/`。

|证据|SHA-256|
|---|---|
|manifest.json|82de408a5f2307c8c592202de59cafc5e4fa874d42d2d3cf85d279f58a2a5696|
|feature/morrow-native-stream-host.exe|56704ee0416b84e022bd4f855be0cde512aba7b6ebd4fe5d517c084492e54bb4|
|default/morrow-native-stream-host.exe|4ddbcb5bfcf92dbcef269c2f1acf8c59dc768e40da933202dc989954fbe71c60|
|build-receipt.json|6ebc568a91d6f2fcf9a76cbca0e16ff7b7d234bed5fdd2a721e4d91046793fb1|

manifest的主executable为feature候选，features=[qualification-pipe-fault]；default_candidate有独立路径/摘要且features=[]。source_files的270个相对键对应source/下不可变快照。冻结前后与构建记录一致，包含host及Rust依赖/协议输入；两种构建用独立于旧固定候选的target/m03-fixture-003-host。默认真实exe拒绝qualification CLI，feature真实exe拒绝将plan用于init，见cli-boundary-receipt.json。manifest还记录每组测试log路径、摘要、features及通过数量。

## 改动

H1为每个Shared事件增加admission.created同域at_ns/clock_domain，保留同次采样at_us；输出actual Parent/gate原期限、首次ttl、Challenge remaining采样及取消时effect fence实际ordinal。普通完整Challenge发送仍由control_frame_sent原始字节和同域timestamp验证。

H2的plan先验证固定边界和guest spec参数绑定，加入执行配置摘要，然后接原authority绑定；同一owner不能重新批准/重绑plan。仅真实首个response BodyChunk命中，保存完整raw并对其前12B做唯一实际WriteFile，body_end=None。不缩小普通FrameWrite的请求语义，不发完整原帧完成事件。独立容量1内部通道接收紧凑见证，owner在gate锁内同次Instant检查原期限/撤销/完整绑定/实reap，匹配后真实cancel/reap/drop同一endpoint。ACK仅接受请求，真实关闭与join分别观测。主动接缝不制造OS错误或首次撤销原因，真实错误仍保留。详细字段见接口002。

## 本地验证

最新记录为m03-fixture-003-host-check/verify-003/receipt.json。默认与feature cargo --locked --offline构建均通过。52次定向测试调用通过，以下为各组数量，不能理解成52项新用例或完整产品测试：

|组|default|feature|
|---|---:|---:|
|普通真实本地pipe|7|7|
|完整frame写入状态|8|8|
|首次原因/外部撤销interleaving（模拟owner，真实账本）|5|5|
|新qualification边界/真实pipe|—|9|
|CLI输入边界|1|2|

新真实pipe用例证明：peer先真实读4B再8B，buffer=12且完整decode失败；actual prefix write只issue/reap一次，body offset=0，原frame完整完成事件=0；匹配前/错误/重复见证不关闭；witness有效时pending server ReadFile实际0B/995取消reap，drop后peer实际EOF，owner真实join。gate撤销或到期无匹配关闭/尾部重发。peer真正断开时实际109/232/233失败与join分别保留；主动关闭未伪造同样错误。越界新写在OS issue之前拒绝。

初次compile-001默认通过但feature的attempt类型错误已修，错误日志原样保留；verify-002的25次通过记录也保留，不以最终成功覆盖早期结果。wire crate既有unused_parens警告未修改。

后续由主协调检查候选/guest003/harness冻结后，运行原TTL、network-abort及残帧close真实场景；须分别核G2来源投影、实际Core/parser消费、first reason、control/data错误、网络/管道join和Released。此候选就绪不补齐旧84项资格，也不改旧A010失败或旧A/B已签收slice。
