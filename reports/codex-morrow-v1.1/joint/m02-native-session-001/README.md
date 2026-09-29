# M-02 原生会话首片：联合验收

本片只写本目录。实际宿主为 `build/io-safety-refactor` / `codex/io-safety-refactor`，基线 `88557916aabf2e10619b1022110035178498898a`。新 wire 只消费宿主新版本合同，不在此定义 Schema。旧工具、003、v1 canonical、旧 handoff 和产品台账保持冻结。

`matrix.json` 是 14 项独立诊断矩阵，关联原计划编号及原文行，不另造产品验收编号，也不把诊断数混入原 84 项通过数。`baseline.json` 固定旧证据、003、v1、旧 UI stdio 入口和计划身份。`check_review.py` 可运行，只读核验身份、工作区、矩阵和来源；不启动交付程序，不把文件摘要相同认定为功能通过。每次 `--run-id` 必须新建，默认退出 2 表示真实运行复核未完成；不一致退出 1。`--integrity-only` 的退出 0 仅表示静态前置通过。

```powershell
python -X utf8 -B reports/codex-morrow-v1.1/joint/m02-native-session-001/check_review.py --run-id preflight-001
```

本片需要的交接：宿主唯一的新合同/kit/向量、可复用准入与会话所有权实现、受控临时 fixture 启动方式；插件固定源码/锁/编译记录/客户端 exe 与具体受控模式。ready 后先审源与摘要，再添加独立帧解码和真实运行检查器；此处尚无宿主字段或线上权限假设。

重点诊断：实际 child/启动产物/通道绑定；握手与只读状态；伪造身份不能授予权限；旧代次与撤权；在途结果；首次准入单调期限不续期；控制路径有界且不会被正文堵塞；畸形帧、超时与有限线程等待；退出和 stdout/stderr EOF 分开；ClosingUnconfirmed 保留 owner；未知结果不重试新操作；fixture 不泄漏生产入口。

独立运行必须使用本目录下新临时目录及个人环境隔离，自行记录 PID、可核验的进程关系、exe 摘要、双向原始帧、单调时序、停止请求、退出码与输出关闭。程序自报 pass 只是输入，不能替代观察；无法独立观察的字段列为未验证。真实临时宿主不等于正式安装版；查询成功不证明 HTTP、持久化或执行能力。

本片准备阶段：所有诊断 not_run，M-02 整项未验收、G1 未通过；P-02/J-00/G0 仍 blocked，两产品图 0/2，原 84 验收全部 not_run。不自动推进 M03 等后续实现。
