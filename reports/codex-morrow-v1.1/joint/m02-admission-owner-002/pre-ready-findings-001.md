# 冻结前源码发现 · 2026-09-29

来源：只读 `native_session_owner_002/src/authority.rs`，当时 SHA-256 `6ccda05f1b433224f56caa75bf755db171d2bc652fdc0ed6f43d98013a376993`。该文件仍在开发，不作为最终候选；未运行宿主。已向宿主发送以下两项具体发现。

1. **Revoked 持久状态无法再读**。第50行 `load_owner` 的阶段集合包含 LaunchPending/Preparing/Active/Closing/ClosingUnconfirmed/Released/NotStarted/Unknown，缺少 Revoked。第157行 `poll` 直接写 `o.phase=s.phase.clone()`；真实 session 撤权时进入 Revoked。由此下一次 inspect/load_owner 会拒绝自己刚写的记录。需合法映射或纳入合法状态，并验证持久撤权、runtime applied、查询、停止与释放的连贯路径。

2. **claim 未完整核对固定批准上下文**。第100行批准时 `admission.bind_authority(&g.encode_to_vec())`（当时config空，state=1），但第65/101行 live 仅保存 Admission。第114行仅比较 issuer/profile/slot/state/config，未比较plugin/role/operation/artifact/schema/generation等与原批准期待值，也未重算上下文。若记录内容经规范重编码变更而config摘要值保持不变，字段本身不会因已进入初始hash而自动获得验证。建议保留不可反序列化的完整原批准期待值，在claim事务里与重读记录完整匹配；或规范重算context并与原admission摘要一致校验。此要求落实固定绑定语义，不扩张为同用户强攻击者防护。

状态：待宿主修正与producer证明；正式联合接受须复核最终封存代码/输入。此文保留原发现，不随后续修复覆盖。
