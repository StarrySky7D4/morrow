# runner003 冻结增量只读复核

2026-09-30。**runner003 在本次审查范围内可进行已授权的有限故障运行。** 前一报告中001/002的原判保留；本报告只闭合002的结果写入/关闭/发布失败边界，不授予故障场景整链通过、SDK冻结或产品接受。

本次0 HTTP，没有运行host/guest、Cargo或生产runner.run，也没有修改候选、旧证据或Git。独立检查执行冻结AST提取的封存纯函数，只在本报告目录内写明确标为synthetic的测试文件。

新冻结manifest SHA为 `d3ab74809a7bb56472e9999a8b5280e7962c94cb8df2d69d7e912dab0ff20e53`，runner SHA为 `ceb557e0a5b7da29b9c69b96949358356a7854dd5f3f5f2388934d2b75566cb0`。三份源码当前/冻结副本及pure-check前后摘要吻合，13项生产方纯测试日志与摘要一致。host manifest仍为82de408a…5696，270个当前源及冻结副本均吻合；guest manifest仍为b810526c…8106，78输入全部吻合。

002→003的完整差异已审：runner只改变seal_result；晚host/guest/runner pin失败锁存、generic异常failed、真实close after_ns<D否则unreached，以及其他运行/清理逻辑都未变。freeze工具未改，纯测试新增pending写完后close故障和实际rename完成后报错两例。

seal_result现在先完成pending manifest/result的写入和关闭，再rename为公开文件。封存或发布异常时写 `seal-failure.json`，其中 `qualification_valid:false`；保留pending文件，若result已经公开则复制原字节到 `result.rejected.json`；通过独立失败写入和同目录原子replace发布非expected结果。已有failed不会被降为unconfirmed。旧manifest若已经公开，其result预测摘要与新失败result不同，不能作为合格封存。

独立纯检查覆盖九例：正常封存、hash失败、manifest/result各自写完后报错、manifest/result各自在rename前与实际rename后报错，以及原本failed再遇hash失败。正常例的最终result和guest文件摘要与manifest吻合，没有pending或失败标记；其余八例均返回False、最终结果非expected、退出码1并有权威失败标记。result pending写后故障保留expected原pending；result实际rename后故障保留原expected字节为rejected且SHA等于旧manifest预测值，新失败result不再匹配旧manifest。首次原因19和独立残帧错误字段没有重写。证据见 [result.json](result.json) 和 [独立纯审查脚本](verify_runner_003.py)。

审查脚本首次自身故障注入的闭包选错了目标文件名，注入未触发；对应合成目录保留为 `synthetic-seal-cases`。修正后九例在 `synthetic-seal-cases-v2` 完整通过；这不是生产候选故障，也不是HTTP运行结果。两类合成目录都不能用作runtime证据。

有限READY沿用[前轮完整审查](../frozen-passive-review-003-001/review.md)的边界：G2运行前projection仍依赖冻结guest producer，整份IO/frames关联只能在运行后核；marker SHA不是认证。实际运行只有成功退出、无seal-failure、完整result/manifest摘要一致才可授予故障观察资格；nonzero、失败标记、pending/rejected结果或摘要失配均拒绝。该文件流程不宣称突然掉电后的持久性，也不把本地纯故障注入计为实际OS存储故障验收。

下一阶段由主协调按既有授权分别运行原权限自然到期、网络中途断开、pipe残帧关闭，每例新目录、一个POST。联合随后对原始帧、actual reader/worker/child回收、首次取消原因、完整IO引用、持久账本及封存结果进行只读整链审查。本报告本身的HTTP请求数仍为0。
