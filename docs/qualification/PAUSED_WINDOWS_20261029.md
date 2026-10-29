# Windows 验收暂停与开发分支快照 · 2026-10-29

用户要求“暂停任务并推送到github”。后续实现与验收已暂停，原先已运行的 SDK 构建719正常收尾；暂停后没有启动新构建或产品用例。VM047 已保存，注册 VM 共4台，其他三台状态未改变。恢复须等待用户指令。

本次仅发布开发分支 codex/local-continue-20261020 的源码、测试及公开报告，提交带 [skip ci]。完整 P2、当前源码 P3 复核、SDK26 / G04 / SDK_FREEZE、源码整体验收和发布资格仍未完成。

## 本次保留的进度

构建与纯逻辑验证绑定源码 a038cc73bb370e1cd5b028aa022c744192005fd10ec6dec8a489819aaf12c993；原47步和正式窗口结果绑定较早源码 5fc113edec1e26baa7db613458f9fd37fbfc1b6ee655113c738b3211289be1b6。两者不互相替代。259份暂停前的修改／新增文件与构建清单逐一一致；新增报告和 Git 字节保留规则会改变仓库整体摘要，不宣称发布提交已重新通过实机验收。

| 项目 | 已确认结果 | 限制 |
| --- | --- | --- |
| [P2 四组件冷编译052](evidence/pause-20261029/roles052-cold-build.json) | P/C/R/S 各自使用独立空输出目录，全部重新编译；程序摘要及只读允许表核对一致 | case703尚未派发，不能称为实机通过 |
| [固定策略身份修正](evidence/pause-20261029/policy703-independent-preflight.json) | 单一目标 SID 生成二进制表示；独立 .NET 编解码验证；18项原纯逻辑方法及31个破坏输入通过 | 未调用 LSA；修正后的实际权限生命周期待新用例验证 |
| [SDK 默认程序719](evidence/pause-20261029/sdk719-default-hosts-build.json) | Host、Supervisor、历史观察器三程序重新编译，所有原编译进程正常返回 | 依赖缓存明确复用；不称为冷编译，也没有启动产品验收 |
| [正式入口 Kernel722](evidence/pause-20261029/kernel722-current-main.json) | 当前完整 main 编译成功 | 原生启动器没有重编；新窗口尚未启动 |
| [原47步／九组638](evidence/pause-20261029/case638-original47-nine.json) | 正常流程与同 owner 权限恢复通过 | 绑定5fc113源码，不覆盖最新a038源码 |
| [正式文本保存／历史／重开678](evidence/pause-20261029/case678-text-save-history-reopen.json) | 一个 Rust 插件的41字节文本，空格、换行、中文及表情一致；保存和重开历史通过，6个原进程退出为0 | 绑定5fc113；只覆盖该本地内容库文本路径，不代表完整SDK |

此前的持久上传历史、原任务补记、4种正式窗口故障、宿主严格检查与定向回归、目录SDK专项证据仍在原报告中，各自保留源码与范围，不累加为项目通过率。内部父进程↔Runner和v8关闭协议继续使用 Cap’n Proto；JSON只用于审计记录。

## 保留的失败与暂停现场

[case684](evidence/pause-20261029/case684-policy-binding-unknown.json) 暴露固定策略文本SID与二进制SID不一致，结果继续为 Unknown，case_clean=false、policy_clean=false。此次修正不能倒推旧现场已经清理。VM046 已完整校验离线归档并取消注册，原磁盘保留；没有联系、重放或终止该旧 owner。

case639 和 case655 的 Unknown 也保留。[VM047 保存回执](evidence/pause-20261029/vm047-saved-at-pause.json)确认其干净环境已保存；[703准备记录](evidence/pause-20261029/case703-preparation-NOT-RUN.json)仅是准备，原47步／九门槛未派发，也未读取其用例凭据或改变权限。

编译准备中遇到的空间不足、派生脚本检查错误均保留；仅继续尚未开始的 C/R/S 编译，没有重放已完成的 P。原703封装器末尾派生失败已在首次执行前补齐，修复记录只属于公开准备，不能作运行凭证。

## 空间与后续入口

缓存仍在 D:\build_cache。本轮清理只删除确认可再生的旧编译叶文件和已卸载的派生安装介质；旧 C/S 输出采用无损压缩并逐文件核验。源码、程序、DLL/PDB/Wasm、证据、原 VM 磁盘及用户指定保留物不受影响。每项逻辑大小和实测空闲变化单独记录，因并发编译不将空闲变化相加为确定总收益。

1. 恢复后先核对发布引入的报告／Git属性差异、真实源码及产物。当前封存703仍要求a038整体摘要；必须证明源码关系并重新绑定准备输入，不能略过摘要检查直接运行。
2. 核对 VM047 身份与 Saved 状态，保持最多4台注册 VM，刷新环境就绪证据后才进入新的原47步／九门槛。
3. 补齐剩余 P2 故障、策略生命周期和当前源码 P3 复核，再推进完整 Windows SDK。已准备的720/721冻结样例、723内置包、724/725窗口流程均未运行。
4. 保持 source_accepted=false、release_eligible=false，直至所有要求获得匹配源码的实际证据；其他平台按用户要求暂不验收。

[可视化进度报告](../../reports/project-progress-2026-10-26/index.html) · [公开证据清单](evidence/pause-20261029/manifest.json) · [暂停摘要](evidence/pause-20261029/pause-summary.json)
