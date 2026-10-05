# C09 可信目录相对选择与原owner接线

更新：2026-10-05。本阶段源码已安装，本阶段限定Windows Release／locked／offline实际通过新20、factory14、原owner九组115和原件42方法，各组计数保持独立，不作为完整SDK冻结。Workbench只完成库编译检查，不提供产品运行资格。当前状态见 [项目状态](../../docs/PROJECT_STATUS.md)，接口边界见 [目录/blob SDK](../../docs/PLUGIN_DIRECTORY_BLOB_SDK.md)。C08/C09新增仍在772466发布检查点之后本地未commit／push，不产生新安装包或Release。

## 选择范围与原授权

新宿主入口 `IoWorker::capture_directory_under(anchor: File, relative: DirectoryRelativePath, limits: CaptureLimits)`沿用原ManagedHostOwner、Manager、HostRuntime、ManagedInstance、IoBinding和原时间。可信宿主提名已经打开的anchor及相对raw UTF-16分量；请求在原worker进行prepare、身份、exact FileList、取消／deadline和费用检查，自动使用C08 `getrandom 0.4.3`的新鲜秘密，不接受guest自报授权时间或新grant。

旧 `capture_directory(file, limits, secret)`与 `capture_directory_fresh(file, limits)`保持bare-File语义；它们仅证明传入对象。新Under路径的证明从这个opened anchor开始，包含实际保留的每个中间目录至leaf；不证明anchor以上祖先、native picker时刻与当前对象对应，也不证明传入anchor的共享打开策略或用户选择真实性。

`DirectoryRelativePath::new(Vec<Vec<u16>>)`持有原始分量，不做Unicode转换、规范化或绝对路径重开；允许孤立surrogate。每分量最多255 UTF-16 units，分量长度及保留容量合计最多8192 bytes，外层段数及容量最多32。空段、`.`／`..`、NUL、控制字符、路径分隔符、设备名、末尾点／空格等非法拼写关闭。这些语法检查不是文件系统grant。

## 原资源、native边界与释放

leaf和每个保留祖先各占一个原FileList资源：root加N个分量必须满足原共享上限8，通常最多7个分量；其他IO资源进一步减少可用深度，32段语法上限不保证能够打开。链资源在native打开前整体预留，保留输入容量、链bookkeeping和查询／打开费用按原ledger收取；已经admitted的bytes费用不因错误、取消、partial open或清理退款。此allowance不是allocator、OS handle或进程RSS的完整上限。

逐段打开使用上一层持有句柄作为RootDirectory，raw UTF-16单分量作为ObjectName，FILE_OPEN只打开已有目录，不create／truncate。NoReparse选项与返回句柄的目录属性、reparse和volume/file identity检查共同关闭重解析链。子句柄使用固定只读访问与Share READ，末级增加listing权；这些设置不改变已经传入anchor的sharing策略。Windows接口机制见 [NtCreateFile](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntcreatefile) 与 [OBJECT_ATTRIBUTES](https://learn.microsoft.com/en-us/windows/win32/api/ntdef/ns-ntdef-_object_attributes)。官方接口说明不提供本项目运行资格。

capture／分页前后复核所有持有祖先和leaf的同句柄身份，不以名字重新定位对象；native查询与打开、私有解析／编码、取消谓词和实际drop都在原clock锁外，采样与对应授权验证在同一短原clock step完成。取消只否决后续动作，不能抢占正在进行的同步OS调用。保留句柄及重复身份检查不等于文件系统原子快照；普通目录编辑仍可能被观察到。失败／撤权／deadline／终态／owner退出释放实际链、lease与spool之后才归还观察额度，Unknown不自动重放或重建选择。

## Workbench宿主入口

Windows `start_directory(options, anchor, relative, limits, handler)`要求exact FileList和原声明的handler，复用原start_task的prepare、超时、持久任务、Stop及owner归还生命周期。每个selection只允许一个pending／未读结果；成功消费Capture才允许下一页，typed结果与pending种类核对，终页／Finish／失败请求原Stop，实际join仍由原owner返回证明。foreign session／epoch／cursor不能删除其他选择或自动重试。

Workbench首次check001因离线缺asn1-rs 0.7.2实际exit101，原日志保留；准备299份公开locked归档、使用独立新cache／target后，第二次check002实际exit0（71.282秒），命令为Release x86_64 `--locked --offline --lib`。8条来自未改Workbench模块的dead_code警告保留，无编译错误；这不是strict -D warnings资格，它仅检查编译；受保护Storage／Session产品执行明确NOT_RUN，不新增测试storage seam。这个入口只接受可信宿主提名，不证明native picker、产品授权UI或生产端到端流程。

## 实际限定验证与证据边界

| 当前C09范围 | 实际结果与计数边界 |
|---|---|
| 新raw UTF-16路径单元范围 | 8方法PASS |
| 新相对选择integration | 12方法PASS；junction两个子案例只计一个方法，新方法合20 |
| 原C08 factory | 14方法当前重新PASS；过滤另存，不继承旧执行 |
| 原owner九组 | 115方法当前实际PASS，含原目录／取消／native范围，不额外累加其中方法 |
| 原始42定向回归 | base9、dependency3、region7、reader普通主9、shared14当前PASS；region实际84过滤、两个专用child helper跳过。reader raw10全通过含1个child helper，其原main9计入42，child不加原42信用 |
| 测试进程及方法 | 17个credited成功进程，15个唯一实际consumed testexe；meaningful191＋reader child1＝raw192。另1个zero-match进程失败保留、0方法信用；命令数、重跑、junction子案例和helper不增加方法 |
| Workbench Windows | check001 offline依赖缺失exit101保留；check002 Release x86_64 locked/offline库check实际exit0。保留8条来自未改模块的dead_code警告、无编译错误；只编译，不运行受保护Storage／Session产品路径 |
| C09 network100／Clippy | NOT_RUN；C08历史network100或lint结果不能替代当前资格 |
| ProtectedSession／GUI／picker以上provenance／non-Windows | NOT_RUN；不计入普通合成Windows范围 |

成功测试使用当前实际执行文件和命令，不依据文件名、旧target或历史PASS推定。raw stdout／stderr、退出码、失败attempt、源码与实际产物SHA256、SDK327两份及index前后守恒记录可恢复；当前source116保持。成功Workbench check使用独立公开锁定依赖缓存，旧cache和原runtime test images保持。记录证明声明输入和执行范围，不扩展为所有transitive compiler在每个时间点的完整证明。

C06/C07/C08报告保持各自scope，不重写旧C08结果，也不以本次结果追加历史方法数。SDK327／冻结57和原guest/provider不重建或重封。已推送检查点为772466，C08/C09新增仍本地未commit／push，没有新Release。

## 仍开放的完整目标

公开Core/guest FileList和conditional Replace仍Unsupported；没有新增C/C++/Wasm目录guest、公开import或公共UI。目录codec的C/C++接口只是载荷接口，不是宿主选择授权。完整SDK26／G04继续OPEN：anchor以上与picker provenance、产品Workbench执行、目录request/schema和feature/import/helper profile协商、blob耐久后端／上传／watch／rename及恢复、其他平台仍需独立实现与验收。Unknown不能由页面、ACK、EOF或held chain变成业务成功或自动重放许可。
