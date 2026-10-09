# v33 外观偏好修复后独立复核

2026-10-09。只读审查与实际ETS源执行；没有修改产品、Index或测试，没有运行SDK、设备或Git。

修复前P1的受控复现与56项测试证据保留在 `appearance-preferences-independent-review.md`、`appearance-preferences-independent-review-repro.json` 及 `appearance-preferences-independent-a1-*`，未覆盖或改称通过。

## 结论与冻结来源

**原跨页面live存储调用交接缺口，在此次实际源与受控端口范围内已修复。** 新页在旧调用实际终止前不能读取、写入或显式恢复；dispose不会假取消已发调用或释放namespace owner。旧未知写入的原值、已确认current及issued literal跨实例保留，终止后仍要求显式recover。recover只核对观察到的值，不put、不回滚、不自动重放原值或新候选；候选保留为未保存预览，另行save后才确认。

冻结模型 `hmos/entry/src/main/ets/model/AppearancePreferences.ets`：22,653字节，SHA256 `849abce141efb3656db058a13bb8240e5bff04d001fe8d51558eebde6ddf41e5`。故障测试35,238字节，SHA256 `170ca2ec4994ff6d1fdd481d08beac3e7f92f74a02f7edcdd55fde1e56cb230d`；actual ETS harness2,962字节，SHA256 `2abe0eb9e32580c713aa6ee2c7501dea74271c46b2da96857bd0fee5680e82f5`。

## 独立执行证据

- `appearance-preferences-independent-fixed-a1-result.json`：**61/61 PASS**，0 fail/skip/cancel，402.0061 ms；7个执行输入、4个参考源前后完全一致。日志SHA256 `d0163858acfff4c77d64de2c88d7f0e7e30436f65f6f95963a05bcaf05e7d085`。
- `appearance-preferences-independent-fixed-repro-a1.json`：额外 **6/6 PASS**。分别挂起旧页面的preflight read、put、flush、readback、显式恢复flush和恢复readback，dispose旧页面并创建同一实际受控backend/namespace的新模型。
- 每种向量都验证新页restore/save/recover没有增加任何port调用，busy持续到旧实际Promise终止，也不会显示已确认状态。新candidate opacity50在阻塞和恢复中保留；显式恢复不增加put，随后单独save才让disk/cache/current同为50。JSON保留各阶段view、调用序列、输入前后摘要，验证过程未修改模型。

修复前旧flush向量曾观察到“新页已确认50、旧flush迟到后disk30”。修复后的相同调度只能先等待旧flush终止，保留Unknown并显式核对30，再由新页单独保存50；不再允许新页50与旧活跃flush并发。

## 其余源审查

- JSON span保留未知value原literal，包括大整数、嵌套对象、转义与字符串分隔符。稳定材质id控制未知字段归属；重复字段包括转义同名拒绝。已知字段更新会规范JSON key和格式，未声称原JSON全文逐字保留。
- 完整候选验证、坏原值不可默认替换、put→flush→exact readback、最新candidate与较早ACK隔离、owner变化后不续发旧调用和显式恢复不重放，在既有故障集和新增交接向量中均保持。
- 新增Map/Set在进程内按port namespace共享实际live owner；Index读取时明确传 `appearancePreferenceName`，与 `getPreferencesSync(...name: appearancePreferenceName)` 对齐。owner一直保留到read/put/flush/readback的真实Promise返回，后台或dispose均不释锁。不同实际namespace可独立工作。
- API26本地声明包含getPreferencesSync、hasSync、getSync、put Promise、flush Promise。实际Index只把hasSync为false作为缺失，非string拒绝；模型4MiB预算小于本地声明16MB值上限。未发现API存在性冲突；运行TypeScript4.9.5-r4转译实际ETS不等于ArkTS严格检查或assembleHap成功。

## 仍然开放的验证边界

此结论仅是实际ETS执行与受控存储/owner调度证明；SDK编译、ArkUI渲染、真实ArkData写入顺序/持久性、设备重启及断电均 **NOT_RUN**。进程内namespace屏障不提供跨进程CAS；同一个实际后端必须使用一致namespace。进程结束后内存Unknown元数据不保留，设备重启需要验证实际持久化结果，不能由这些测试宣称解决。

未发现需要再次修改冻结模型的实质数据损失缺陷。最终SDK及设备门禁应使用此冻结hash和最终Index来源，不能沿用修复前摘要。
