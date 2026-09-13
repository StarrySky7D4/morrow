# test.48：SDK 固定 guest 兼容候选与非对齐读取修复

日期2026-09-13。应用 `0.1.9-test.48+53`，核心／宿主／审计／运行时／SDK 源码包 test.48；工作台 guest 源码包仍 test.13，重新链接当前 SDK 构建。数据库格式14不变。第一方 AGPL-3.0-only；无新增 unsafe，仅本地开发。

## 交付与兼容边界

新增 `sdk/compat/guest-v1-rc1`，固定13份裸 Wasm、13份完整插件包、八份契约快照、源码摘要和构建来源。36文件清单的根 SHA256 为 `cdda1d8fde36f984f95d193753eaf40b2f75db4bb119986ca1cc733710000266`；目录另有清单文件，共37文件、2,364,532字节。原件从 clean `19fa955d9b7a4eed1cd9b40fb0f787defbe4a33b` 对应SDK test.11源码在本轮重新编译并一次性捕获，不冒称此前发行原包或可复现构建证明。

普通兼容入口不编译guest、不重打包或刷新摘要。Python先验证固定清单和文件，当前Rust宿主测试检查根摘要、条目集合、文件摘要、包内模块与裸Wasm一致性；直接执行通过摘要检查的同一份内存缓冲，消除校验后重开文件的窗口。SDK、runtime、UI三个现有验证入口都先执行旧原件检查。Git attributes固定原件字节，防止checkout行尾转换。根摘要锚定版本控制，不是发布者签名。

实际契约为guest ABI2、runtime7、task3、UI1、dependency-call1；配置名中的v1不是旧guest ABI1。包加载原摘要严格匹配保留，未接受任意旧摘要或放宽契约。UI完整执行form→当前解码→当前事件→旧guest edit→当前解码。Native ABI、Rust源码API、宿主内部结构、复杂UI、异步长期服务与全平台仍不属于稳定承诺。详见[兼容规则](../docs/PLUGIN_SDK_COMPATIBILITY.md)。

## 测试内容

固定内容／转换／UI共9项（三语言各3项）：七类内容命令、历史幂等、七种能力撤销拒绝、执行前取消及真实提交期间取消不冒称回滚；转换每语言15个输入／handler组合，包含空值、Unicode、全字节及64KiB，核对业务失败，另有3种注册拒绝；UI每语言4次输出与3种非法输入业务失败，验证重复事件、旧修订和关闭后拒绝。未把它称为全部UI节点或跨generation矩阵，也不是Flutter像素验收。

固定依赖共3项：原C/C++/Rust模块输入真实动态字节，经批准slot到另一原Rust模块；锁持久重开、含NUL/0xff、无内容grant拒提交、授权后幂等、提供者停用使旧结果和旧实例失效、新实例不继承授权、临时共享资源回收均验证。没有把单层固定样例外推为完整依赖图审计重放。

Python有11项清单负面／完整性测试和3项契约同步测试：改变模块或包、重算清单不能绕根摘要、缺项、重复、额外文件、路径逃逸、空清单、错误pin，以及实际同步CLI的dependency-only漂移拒绝与临时库修复。修复原同步脚本漏检dependency_call.capnp的问题，未更改任何契约原件。

## 验证中发现并修复的问题

首次固定包测试编译遇共享helper引用路径错误；依赖测试曾把usage当作共享借用，均只修测试实现。独立审查纠正了提供者ID断言，并指出hash后重新读取文件的问题；现执行已校验的同一缓冲区。

首次完整SDK验证在ffi::ui_tests::ui_event_ownership_and_exact_u64失败。独立诊断证明同一原event偏移0成功，偏移1直接Capnp返回UnalignedSegment。当前SDK改用统一安全OwnedSegments reader，覆盖请求／响应／任务／UI四入口；消息上限、Some(max_bytes/8)遍历预算、nesting16及精确单帧消费保留。Capnp在正文分配前校验1..511段、checked总字数和预算，正文分配受64／128KiB上限限制，段表附加内存也有界。既有dependency专用读取预算未改变。

新增5项非对齐回归，验证所有0..7字节偏移、原始UInt64与帧关联、截断、尾随、拼接帧、伪造单段u32最大长度及511段超总量拒绝。此处没有新增unsafe、启用capnp全局unaligned feature、替换golden或修改冻结包。原28项专项与严格Clippy通过，随后执行完整SDK和当前guest资格检查。

完整首次失败日志 `build/test48-sdk-verification-initial.log`，直接诊断 `build/test48-sdk-alignment-diagnostic.log`，修复回归 `build/test48-sdk-alignment-{fixed,regressions,clippy}.log`。契约同步测试首轮曾错误期待CRLF不变，改为准确的LF规范化断言后通过，未放松生产检查。

## 最终验收

| 范围 | 最终结果 |
| --- | --- |
| 核心release + fault-injection | 362/362 PASS |
| 宿主release + fault-injection | 修复后重跑105/105 PASS |
| 运行时release + packages | 修复后重跑226/226 PASS，含12项固定原件测试 |
| 审计release + fault-injection | 65/65 PASS |
| SDK完整本地测试 | 42/42 PASS，含5项非对齐回归；C/C++原生适配与15份独立core golden一致性PASS |
| 四个核心组件all-targets严格Clippy | 全部PASS；SDK本地、当前Wasm及三语言构建脚本严格检查也PASS |
| 固定原件完整性／同步工具 | 14/14 Python测试PASS；执行前后36文件根摘要一致 |
| 当前SDK三语言Wasm | 标准runtime资格PASS；正文增量、UI及实际依赖调用均PASS |
| Flutter实际宿主集成 | 9文件41项PASS |
| 插件UI实际渲染 | renderer8项＋三语言guest表单3项PASS；三语言分别8个UI任务、5个输出、3个业务失败 |

Rust计数经独立--list核对，包含故障child入口，不重复累计子进程stdout和重复验证轮次。当前guest在修复后重编译执行，与固定旧原件的12项检查分别成立。完整SDK入口初次失败及专项诊断如上保留；所有最终结果使用修复后源码。核心／审计未受SDK读取实现变更影响，其原全量通过结果继续适用。

Windows Release实际编译60.2秒，版本 `0.1.9-test.48+53`，程序自检退出0。四项通过：原生blur 0/1/12/40及关闭API；真实静音WAV解码与时钟推进；seek／播放互斥／恢复不自动播放；实际Rust工作台无Flutter错误渲染。玻璃检查不等于桌面背景像素比较。

实际截图 `build/workbench-host/test48-final-6a7422d3ce92464280c5ce3747a0ceb6/result.png` 已查看，侧栏、设置和卡片正常，无加载错误占位。自检库格式14、integrity_check=ok；1个Ready、1个published归档、10分片和1个归档根；费用账本1档、1,585,298逻辑字节，与原件计费一致。保留6份Evidence/6引用、34唯一块/144引用，缺块、孤块和缺失捕获归档均0。

5个产物SHA256已独立重算，Windows内宿主和工作台包与本轮生产副本逐字节一致：

| 产物 | SHA256 |
| --- | --- |
| `morrow_studio.exe` | `27d909fde9bc877378cb033631563031564445058ce89a1a193dca3cc978df4d` |
| `morrow-workbench-host.exe` | `89791501a848370e22a42d3bfc91ea40f61558734fd6290e3c66cc93a02457dc` |
| `workbench.morrowplugin` | `bfec6cf2c53298fd947cf8bcea2a468a829cb902b3954265ffc79c024375ad16` |
| `morrow-content-replay.exe` | `d06589d68209c9beeda0b8babcf4e4979493879d6aefa3da91067fa368036a66` |
| `morrow-audit-check.exe` | `15cab7cbb298a2e5595cbbf891d83d1460853bb1901bfea4412efee44cc3303c` |

元数据 `build/test48-final-verification.json`。日志：`build/test48-{core,runtime,host,audit}-{full,clippy,list}.log`；修复后完整重跑另为 `test48-runtime-final-full.log`、`test48-host-final-full.log`。SDK完整记录 `test48-sdk-verification.log`，当前guest为 `test48-sdk-current-{guests,content,ui,dependency}.log`，Python为 `test48-sdk-python-tests.log`；Flutter和Windows见 `test48-flutter-tests.log`、`test48-windows-{build,qualification,inspection}.log`。本轮未进行Android/Web/macOS/Linux/iOS产品运行验收，不继承旧版构建结论。

阶段结论 **PASS_SCOPED**：基础guest兼容候选有实际旧原件执行门槛，当前SDK合法非对齐输入问题已修；完整插件系统目标继续。


## 剩余目标

本轮建立guest兼容候选及真实回归门槛，不是插件系统完成。继续推进开发模板／打包诊断、通用插件管理和UI扩展、已发布历史可审计清理、持久长期任务与依赖图证据、旧数据迁移及各声明平台端到端验收。SDK可以按能力稳定，宿主存储和完整产品仍按M0–M7门槛推进。
