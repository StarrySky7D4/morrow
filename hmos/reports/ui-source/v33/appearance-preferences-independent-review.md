# v33 外观偏好独立审查

日期2026-10-09；只读审查 `AppearancePreferences.ets`、56项实际源执行故障测试及报告、接入时正在变化的Index与安装的API26 preferences声明。没有修改模型、Index、测试或Git，没有运行SDK/设备。

审查基线模型：19,067字节，SHA256 `497158b595f7ef3d13bab3b9ba6cda9ac38b7e84884791bed4ae6ea96564f6e4`。这是修复前快照，后续模型修复需重新验收。

## 实质发现：跨页面实例的活跃存储调用没有屏障

**P1**：模型的 `pending/queue` 在实例内（约224-291行），`dispose()`仅置disposed（320行）；已发出的platform put/flush不会被取消。Index新页创建新模型，并未按preferences namespace共享活跃调用屏障或传递未知写入状态。旧owner guard能够挡住旧响应更新UI，但不能挡住已发native存储操作的迟到效果。

以真实ETS模型和受控共享storage port复现：

1. A恢复已确认opacity76，然后put30，flush#1捕获30并挂起；dispose A。
2. B是同namespace新模型。B.restore返回true/current30，而disk仍76，说明B已经把未flush缓存作为已确认值。
3. B.put50、flush#2与readback全部完成，B.save返回true/current50，状态“✓ 外观设置已在本机确认。”。
4. 最后旧flush#1完成，disk变30；A正确进入write-unknown，但B仍ready/current50/已确认，cache也仍50。

实际观察保存在 `appearance-preferences-independent-review-repro.json`。这证明现有模型及接入没有强制满足其“单一活跃writer”前提；**没有声称真实ArkData服务在设备上发生该乱序**，也没有把受控端口当成设备持久性证明。该合法端口调度需要被架构排除或由平台实际合同及设备证据排除。

已立即通知Root与appearance_preferences。建议进程内同namespace共享live-operation barrier及unknown交接：新实例在旧调用仍活跃时不能restore为confirmed或写入；旧dispose不能释放仍活跃的屏障；terminal后先核对原值/issued字节并处理unknown，再准入新写入。应覆盖旧put挂起、旧flush挂起、旧readback挂起、dispose/替换、queued候选与显式恢复。既有56项只覆盖同实例owner变化/重试和实例内pending，未覆盖新实例重建接管。

## 已核对的保护与范围

- 读取失败、空/非string/坏JSON不映射为缺失；缺失由实际hasSync证明。候选完整校验，坏原值不自动默认覆盖。
- JSON span合并只改已知字段，未知value包括大整数、嵌套对象和escaped内容保留原raw；材质按稳定id保存未知字段，改名/重排不附给另一个id。JSON.parse先验证语法；重复根字段、被编辑材质字段（包括转义同名）拒绝。未知property名称可规范转义写出，报告声明的是value原跨度保留，未冒称原JSON全文逐字保留。
- 实例内put→flush→exact readback后才晋升current；最新candidate独立，较早ACK不覆盖。已发后owner变化进入write-unknown，不继续旧queued写入。显式recover不put、不自动保存新候选，unknown恢复有flush ACK及精确再次读取。
- 单实例fault corpus重新执行：`appearance-preferences-independent-a1-result.json`，**56/56 PASS**，0fail/skip/cancel，7执行输入+4参考前后摘要一致；419.5602 ms，日志SHA256 `cf82d42cd20e1dcd177b72c49e8f0bde2932fe6473ae39b260d42afdbe98549a`。该通过不消除上述跨实例新向量。
- API26本地声明提供getPreferencesSync、hasSync、getSync、put Promise与flush Promise；put仅修改实例、flush持久化。MAX_VALUE_LENGTH声明16MB，模型4MiB是应用自己的更小预算。没有明显API存在性冲突，但TypeScript实际源执行不是ArkTS规则/SDK编译证明；最终assembleHap仍必需。
- root字段验证和4MiB预算是HMOS现行UI合同，字体asset/平台字体验证不属于这个model；后续字体应使用独立FontPreference合同。模型无跨进程CAS/断电或真实设备存储证明，这些边界保持开放。

当前结论：**既有56项通过；跨实例live调用交接缺口必须落实或明确排除后才能声称新生命周期已安全接入。** 本报告本身没有做修复。
